#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]

use code_o_matic::config::Config;
use code_o_matic::history::LlmResponse;
use code_o_matic::llm::{LlmClient, ReplayLlmClient};
use code_o_matic::tools::ToolCall;
use code_o_matic::Agent;

use serde_json::json;
use std::sync::Arc;

#[tokio::test]
async fn agent_dispatches_read_tool_then_responds() {
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

    let llm: Arc<dyn LlmClient> = Arc::new(ReplayLlmClient::new(responses));
    let mut agent = Agent::new(Config { repo_root: repo, ..Config::default() }, llm);
    agent.init_builtins();

    // when: run_turn is invoked with a user message
    let reply = agent.run_turn("read hello.txt".to_string()).await.unwrap();
    // then: the provider is called, the tool is dispatched, and the final response is returned
    assert_eq!(reply, "the file contains: world");
    // history should contain: user, assistant(tool), tool, assistant(final)
    assert_eq!(agent.history.messages.len(), 4);
    assert_eq!(agent.history.messages[2].content, "world");
}

#[tokio::test]
async fn agent_reads_absolute_path() {
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
        LlmResponse { content: "done".to_string(), tool_calls: vec![] },
    ];

    let llm: Arc<dyn LlmClient> = Arc::new(ReplayLlmClient::new(responses));
    let mut agent = Agent::new(Config { repo_root: repo, ..Config::default() }, llm);
    agent.init_builtins();

    // when: run_turn is invoked with a tool call reading an absolute path
    let reply = agent.run_turn("read the hostname".to_string()).await.unwrap();
    // then: absolute paths outside the repo are readable (no jail restriction)
    assert_eq!(reply, "done");
    assert!(!agent.history.messages[2].content.starts_with("error:"));
}
#[test]
fn agent_has_default_system_prompt() {
    use code_o_matic::config::Config;

    let dir = std::env::temp_dir();
    let cfg = Config { repo_root: dir.clone(), ..Config::default() };
    let llm: Arc<dyn LlmClient> = Arc::new(ReplayLlmClient::new(vec![]));
    let mut agent = Agent::new(cfg, llm);
    agent.init_builtins();
    // when: an agent is constructed and extensions are loaded
    assert!(agent.history.system_prompt.is_some());
    let prompt = agent.history.system_prompt.as_ref().unwrap();
    // then: the default system prompt is the lean identity line, with tools
    // declared structurally (via request `tools`) rather than in the prompt
    assert!(prompt.contains("Code-o-matic"));
    assert!(!prompt.contains("available tools:"));
    assert!(!prompt.contains("read:"));
}

#[test]
fn agent_system_prompt_in_request() {
    use code_o_matic::config::Config;

    let dir = std::env::temp_dir();
    let cfg = Config { repo_root: dir, ..Config::default() };
    let llm: Arc<dyn LlmClient> = Arc::new(ReplayLlmClient::new(vec![]));
    let mut agent = Agent::new(cfg, llm);
    agent.init_builtins();
    agent.history.append_user("hi".to_string());
    // when: to_request is called on an agent with a loaded system prompt
    let req = agent.history.to_request("m", &[]);
    // then: the system prompt is the first message and the user message follows
    assert_eq!(req.messages.len(), 2);
    assert!(req.messages[0].content.contains("Code-o-matic"));
    assert_eq!(req.messages[1].role, code_o_matic::history::Role::User);
}
