#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]

use inout::history::LlmResponse;
use inout::llm::{LlmClient, ReplayLlmClient};
use inout::Agent;
use inout_core::config::Config;
use inout_core::tools::ToolCall;
use inout_testing::{scenario, then, when};
use serde_json::json;

#[tokio::test]
async fn agent_dispatches_read_tool_then_responds() {
    let mut s = scenario!("core", "Agent loop full turn", "Full turn with tool use");
    let tmp = tempfile::TempDir::new().unwrap();
    let repo = tmp.path().to_path_buf();
    std::fs::write(repo.join("hello.txt"), "world").unwrap();

    // turn 1: assistant requests `read` on hello.txt
    // turn 2: assistant sees tool result and produces final text
    let responses = vec![
        LlmResponse {
            content: "reading hello.txt".to_string(),
            tool_calls: vec![ToolCall {
                id: "call_1".to_string(),
                name: "read".to_string(),
                arguments: json!({ "path": "hello.txt" }),
            }],
        },
        LlmResponse { content: "the file contains: world".to_string(), tool_calls: vec![] },
    ];

    let llm: Box<dyn LlmClient> = Box::new(ReplayLlmClient::new(responses));
    let mut agent = Agent::new(Config { repo_root: repo, ..Config::default() }, llm);
    agent.load_extensions();

    when!(s, "run_turn is invoked with a user message", {
        let reply = agent.run_turn("read hello.txt".to_string()).await.unwrap();
        then!(s, "the provider is called, the tool is dispatched, and the final response is returned", {
            assert_eq!(reply, "the file contains: world");
            // history should contain: user, assistant(tool), tool, assistant(final)
            assert_eq!(agent.history.messages.len(), 4);
            assert_eq!(agent.history.messages[2].content, "world");
        });
    });
}

#[tokio::test]
async fn agent_rejects_jail_escape_via_tool() {
    let mut s = scenario!("security", "Jail path confinement", "Agent tool call escapes are rejected");
    let tmp = tempfile::TempDir::new().unwrap();
    let repo = tmp.path().to_path_buf();

    let responses = vec![
        LlmResponse {
            content: "reading /etc/passwd".to_string(),
            tool_calls: vec![ToolCall {
                id: "call_1".to_string(),
                name: "read".to_string(),
                arguments: json!({ "path": "/etc/passwd" }),
            }],
        },
        LlmResponse { content: "could not read".to_string(), tool_calls: vec![] },
    ];

    let llm: Box<dyn LlmClient> = Box::new(ReplayLlmClient::new(responses));
    let mut agent = Agent::new(Config { repo_root: repo, ..Config::default() }, llm);
    agent.load_extensions();

    when!(s, "run_turn is invoked with a tool call that escapes the jail", {
        let reply = agent.run_turn("read /etc/passwd".to_string()).await.unwrap();
        then!(s, "the tool error surfaces in history and the agent still completes", {
            assert_eq!(reply, "could not read");
            assert!(agent.history.messages[2].content.starts_with("error:"));
        });
    });
}
#[test]
fn agent_has_default_system_prompt() {
    use inout_core::config::Config;
    let mut s = scenario!("core", "Minimal configuration", "Config loads required fields");
    let dir = std::env::temp_dir();
    let cfg = Config { repo_root: dir.clone(), ..Config::default() };
    let llm: Box<dyn LlmClient> = Box::new(ReplayLlmClient::new(vec![]));
    let mut agent = Agent::new(cfg, llm);
    agent.load_extensions();
    when!(s, "an agent is constructed and extensions are loaded", {
        assert!(agent.history.system_prompt.is_some());
        let prompt = agent.history.system_prompt.as_ref().unwrap();
        then!(s, "the default system prompt mentions inout and the core tools", {
            assert!(prompt.contains("InOut"));
            assert!(prompt.contains("read"));
            assert!(prompt.contains("write"));
            assert!(prompt.contains("bash"));
        });
    });
}

#[test]
fn agent_system_prompt_in_request() {
    use inout_core::config::Config;
    let mut s = scenario!("core", "Minimal configuration", "Config loads required fields");
    let dir = std::env::temp_dir();
    let cfg = Config { repo_root: dir, ..Config::default() };
    let llm: Box<dyn LlmClient> = Box::new(ReplayLlmClient::new(vec![]));
    let mut agent = Agent::new(cfg, llm);
    agent.load_extensions();
    agent.history.append_user("hi".to_string());
    when!(s, "to_request is called on an agent with a loaded system prompt", {
        let req = agent.history.to_request("m", &[]);
        then!(s, "the system prompt is the first message and the user message follows", {
            assert_eq!(req.messages.len(), 2);
            assert!(req.messages[0].content.contains("InOut"));
            assert_eq!(req.messages[1].role, inout::history::Role::User);
        });
    });
}
