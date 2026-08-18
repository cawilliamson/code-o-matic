//! slash command types and registry.

use std::collections::HashMap;
use std::sync::Arc;

/// context passed to a slash command handler. rust handlers receive it
/// directly.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct CommandContext {
    /// current model name.
    pub model: String,
    /// current system prompt.
    pub system_prompt: String,
    /// raw arguments string after the command name (may be empty).
    pub args: String,
    /// conversation snapshot — same shape as view builder input.
    pub snapshot: serde_json::Value,
    /// whether reasoning stream is shown.
    pub reasoning: bool,
    /// model ids the endpoint advertises (may be empty if discovery failed).
    pub available_models: Vec<String>,
}

/// the outcome a command handler returns. the tui interprets the action and
/// the message is shown to the user.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct CommandResult {
    /// human-readable text to display in the chat log.
    pub message: String,
    /// optional tui action for the agent to perform.
    pub action: Option<CommandAction>,
}

/// actions a slash command can request the tui/agent to perform.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum CommandAction {
    /// open a named view as an overlay.
    OpenView(String),
    /// clear conversation history.
    ClearHistory,
    /// switch the active model.
    SetModel(String),
    /// open the interactive model picker overlay.
    OpenModelPicker,
    /// set whether the reasoning stream is shown.
    SetReasoning(bool),
    /// exit the agent.
    Exit,
    /// append text as a user message and run a turn.
    RunTurn(String),
    /// drop the last conversation turn (user + assistant + tool results).
    UndoLastTurn,
}

/// a registered slash command. native rust handlers implement
/// `CommandHandler` directly.
#[derive(Clone)]
pub struct Command {
    /// command name without the leading `/` (e.g. `help`, `skill`).
    pub name: String,
    /// short description shown in `/help`.
    pub description: String,
    /// handler invoked with a `CommandContext`.
    pub handler: CommandHandler,
}

impl std::fmt::Debug for Command {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Command")
            .field("name", &self.name)
            .field("description", &self.description)
            .finish_non_exhaustive()
    }
}

/// type-erased command handler. native commands store a closure.
#[derive(Clone)]
pub enum CommandHandler {
    /// a rust closure.
    #[allow(clippy::type_complexity)]
    Rust(Arc<dyn Fn(&CommandContext) -> anyhow::Result<CommandResult> + Send + Sync>),
}

impl std::fmt::Debug for CommandHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Rust(_) => f.debug_tuple("Rust").field(&"<dyn Fn>").finish(),
        }
    }
}

/// registry of named slash commands.
#[derive(Clone, Default)]
pub struct CommandRegistry {
    commands: HashMap<String, Command>,
}

impl std::fmt::Debug for CommandRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_set().entries(self.commands.keys()).finish()
    }
}

impl CommandRegistry {
    /// create an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// register a command. last-registered wins on name conflict.
    pub fn register(&mut self, cmd: Command) {
        self.commands.insert(cmd.name.clone(), cmd);
    }

    /// look up a command by name (without leading `/`).
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Command> {
        self.commands.get(name)
    }

    /// registered command names, sorted alphabetically.
    #[must_use]
    pub fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.commands.keys().cloned().collect();
        names.sort();
        names
    }

    /// dispatch a command by name with the given context.
    pub fn dispatch(&self, name: &str, ctx: &CommandContext) -> anyhow::Result<CommandResult> {
        let cmd =
            self.commands.get(name).ok_or_else(|| anyhow::anyhow!("unknown command: /{name}"))?;
        match &cmd.handler {
            CommandHandler::Rust(f) => f(ctx),
        }
    }
}
