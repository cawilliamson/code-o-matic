//! the agent: conversation loop, native tools, state machine.

use std::sync::Arc;

use anyhow::Result;
use crate::config::Config;
use crate::registry::Registry;
use crate::tools::ToolRegistry;

use crate::history::History;
use crate::llm::LlmClient;
use crate::state::State;

/// the agent: config, history, state machine, llm client, tool registry.
pub struct Agent {
    /// shared config.
    pub config: Arc<Config>,
    /// conversation history.
    pub history: History,
    /// agent state machine.
    pub state: State,
    /// llm provider client.
    pub llm: Box<dyn LlmClient>,
    /// registered tools.
    pub tools: ToolRegistry,
    /// registered tui views.
    pub views: crate::ViewRegistry,
    /// registered slash commands.
    pub commands: crate::CommandRegistry,
    /// whether built-in subsystems have been registered.
    pub initialized: bool,
}

impl Agent {
    /// build an agent with the given config and llm client. built-ins are not
    /// registered yet — call `init_builtins` before running a turn.
    pub fn new(config: Config, llm: Box<dyn LlmClient>) -> Self {
        let repo_root = config.repo_root.clone();
        let max_turns = config.max_turns;

        let tools = ToolRegistry::new();
        let views = crate::ViewRegistry::new();
        let commands = crate::CommandRegistry::new();
        let prompt = std::env::var("COM_SYSTEM_PROMPT")
            .unwrap_or_else(|_| super::system_prompt::default_system_prompt(&repo_root));
        let mut history = History::new(max_turns);
        history.set_system_prompt(prompt);
        Self {
            config: Arc::new(config),
            history,
            state: State::AwaitingUser,
            llm,
            tools,
            views,
            commands,
            initialized: false,
        }
    }

    /// register all built-in subsystems against this agent.
    ///
    /// wires the built-in tools, views and slash commands into the agent and
    /// rebuilds the system prompt now that tools are known. sessions and skills
    /// are included when their feature flags are enabled.
    pub fn init_builtins(&mut self) {
        let mut api = Registry {
            tools: std::mem::take(&mut self.tools),
            views: std::mem::take(&mut self.views),
            commands: std::mem::take(&mut self.commands),
        };
        crate::register_builtins(&mut api, &self.config);

        // register the optional subsystems. their commands/views/tools
        // override builtin ones on name conflict.
        #[cfg(feature = "sessions")]
        crate::sessions::register(&mut api);
        #[cfg(feature = "skills")]
        crate::skills::register(&mut api);

        self.tools = api.tools;
        self.views = api.views;
        self.commands = api.commands;

        // rebuild system prompt now that tools are registered.
        let repo_root = self.config.repo_root.clone();
        let prompt = std::env::var("COM_SYSTEM_PROMPT")
            .unwrap_or_else(|_| super::system_prompt::default_system_prompt(&repo_root));
        self.history.set_system_prompt(prompt);

        self.initialized = true;
    }

    /// build a tui view by name by invoking the registered builder with a
    /// conversation snapshot. returns `None` if no view with that name is
    /// registered or the builder errors.
    pub fn build_view(&self, name: &str) -> Option<crate::ViewSpec> {
        let builder = self.views.get(name)?;
        let snapshot = self.build_conversation_snapshot();
        crate::build_view(builder, &snapshot).ok()
    }

    /// build the context-viewer spec. shorthand for `build_view("context")`.
    pub fn build_context_view(&self) -> Option<crate::ViewSpec> {
        self.build_view("context")
    }

    /// serialize the current history as a json conversation snapshot.
    pub fn build_conversation_snapshot(&self) -> serde_json::Value {
        use serde_json::json;
        let messages: Vec<serde_json::Value> = self
            .history
            .messages
            .iter()
            .map(|m| {
                let tool_calls: Vec<serde_json::Value> = m
                    .tool_calls
                    .iter()
                    .map(|tc| {
                        json!({
                            "id": tc.id,
                            "name": tc.name,
                            "arguments_json": serde_json::to_string(&tc.arguments).unwrap_or_default(),
                        })
                    })
                    .collect();
                json!({
                    "role": match m.role {
                        crate::history::Role::System => "system",
                        crate::history::Role::User => "user",
                        crate::history::Role::Assistant => "assistant",
                        crate::history::Role::Tool => "tool",
                    },
                    "content": m.content,
                    "tool_calls": tool_calls,
                    "tool_call_id": m.tool_call_id.clone().unwrap_or_default(),
                    "reasoning": m.reasoning,
                    "system_prompt": self.history.system_prompt.clone().unwrap_or_default(),
                })
            })
            .collect();
        json!({
            "messages": messages,
            "max_turns": self.config.max_turns,
        })
    }

    /// run a single user turn to completion, returning the final assistant text.
    pub async fn run_turn(&mut self, user_msg: String) -> Result<String> {
        self.state = State::Thinking;
        self.history.append_user(user_msg);

        loop {
            let req = self.history.to_request(&self.config.model, &self.tools.schemas());
            let resp = self.llm.complete(req).await?;

            if resp.tool_calls.is_empty() {
                self.state = State::Responding;
                self.history.append_assistant(resp.content.clone());
                return Ok(resp.content);
            }

            self.state = State::ToolRunning;
            self.history.append_assistant_with_tools(resp.content.clone(), resp.tool_calls.clone());

            for call in &resp.tool_calls {
                let result =
                    self.tools.dispatch_call(call).await.unwrap_or_else(|e| format!("error: {e}"));
                self.history.append_tool_result(call.id.clone(), result);
            }

            self.state = State::Thinking;
        }
    }
}
