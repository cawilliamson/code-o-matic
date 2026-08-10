#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]

mod support;

use inout_testing::{scenario, then, when};
use serde_json::json;
use support::registered;

#[test]
fn context_view_builds_spec() {
    let mut s = scenario!("builtins", "builtin views", "context view builds a spec");
    let (_dir, api) = registered();
    let builder = api.views.get("context").expect("context view registered");
    let snapshot = json!({
        "messages": [
            {"role":"user","content":"hello","tool_calls":[],"tool_call_id":""},
            {"role":"assistant","content":"hi there","tool_calls":[],"tool_call_id":""},
            {"role":"user","content":"read foo.txt","tool_calls":[],"tool_call_id":""},
            {"role":"assistant","content":"","tool_calls":[{"id":"t1","name":"read","arguments_json":"{\"path\":\"foo.txt\"}"}],"tool_call_id":""},
            {"role":"tool","content":"file contents here","tool_calls":[],"tool_call_id":"t1"}
        ],
        "max_turns": 20
    });
    when!(s, "build_view runs over a conversation snapshot", {
        let spec = inout::build_view(builder, &snapshot).expect("view builds");
        then!(s, "the spec has two turns with tool blocks", {
            assert_eq!(spec.turns.len(), 2);
            assert!(spec.turns[0].preview.contains("hello"));
            let second = spec.turns[1].blocks.iter().collect::<Vec<_>>();
            assert!(second.iter().any(|b| matches!(b, inout::extension::ViewBlock::ToolCall { name, .. } if name == "read")));
            assert!(second.iter().any(|b| matches!(b, inout::extension::ViewBlock::ToolResult { tool_name, .. } if tool_name == "read")));
        });
    });
}

#[test]
fn commands_register_core_slash_commands() {
    let mut s = scenario!("builtins", "builtin commands", "core slash commands are registered");
    let (_dir, api) = registered();
    when!(s, "registry is inspected", {
        then!(s, "every core slash command is registered", {
            for cmd in ["help", "clear", "new", "model", "undo", "exit", "reload", "context", "full"] {
                assert!(api.commands.get(cmd).is_some(), "/{cmd} should be registered");
            }
        });
    });
}

#[test]
fn commands_clear_returns_clear_action() {
    let mut s = scenario!("builtins", "builtin commands", "clear returns a clear action");
    let (_dir, api) = registered();
    let ctx = inout::CommandContext {
        model: "test".into(),
        system_prompt: String::new(),
        args: String::new(),
        snapshot: json!({"messages":[],"max_turns":20}),
    };
    when!(s, "the clear command is dispatched", {
        let result = api.commands.dispatch("clear", &ctx).expect("dispatch clear");
        then!(s, "history is cleared", {
            assert_eq!(result.message, "history cleared");
            assert!(matches!(result.action, Some(inout::CommandAction::ClearHistory)));
        });
    });
}
