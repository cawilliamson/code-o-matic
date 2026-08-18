#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]

mod support;

use serde_json::json;
use support::registered;

#[test]
fn context_view_builds_spec() {
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
    // when: build_view runs over a conversation snapshot
    let spec = code_o_matic::build_view(builder, &snapshot).expect("view builds");
    // then: the spec has two turns with tool blocks
    assert_eq!(spec.turns.len(), 2);
    assert!(spec.turns[0].preview.contains("hello"));
    let second = spec.turns[1].blocks.iter().collect::<Vec<_>>();
    assert!(second.iter().any(
        |b| matches!(b, code_o_matic::registry::ViewBlock::ToolCall { name, .. } if name == "read")
    ));
    assert!(second.iter().any(|b| matches!(b, code_o_matic::registry::ViewBlock::ToolResult { tool_name, .. } if tool_name == "read")));
}

#[test]
fn commands_register_core_slash_commands() {
    let (_dir, api) = registered();
    // when: registry is inspected
    // then: every core slash command is registered
    for cmd in ["help", "clear", "new", "model", "undo", "quit", "context", "reasoning"] {
        assert!(api.commands.get(cmd).is_some(), "/{cmd} should be registered");
    }
}

#[test]
fn commands_clear_returns_clear_action() {
    let (_dir, api) = registered();
    let ctx = code_o_matic::CommandContext {
        model: "test".into(),
        system_prompt: String::new(),
        args: String::new(),
        snapshot: json!({"messages":[],"max_turns":20}),
        reasoning: false,
        available_models: Vec::new(),
    };
    // when: the clear command is dispatched
    let result = api.commands.dispatch("clear", &ctx).expect("dispatch clear");
    // then: history is cleared
    assert_eq!(result.message, "history cleared");
    assert!(matches!(result.action, Some(code_o_matic::CommandAction::ClearHistory)));
}

#[test]
fn reasoning_cmd_reports_state_and_sets() {
    let (_dir, api) = registered();
    let base = code_o_matic::CommandContext {
        model: "m".into(),
        system_prompt: String::new(),
        args: String::new(),
        snapshot: json!({}),
        reasoning: false,
        available_models: Vec::new(),
    };

    // no arg toggles and reports the new state
    let r = api.commands.dispatch("reasoning", &base).unwrap();
    assert_eq!(r.message, "reasoning on");
    assert!(matches!(r.action, Some(code_o_matic::CommandAction::SetReasoning(true))));

    // explicit on/off
    let r = api
        .commands
        .dispatch("reasoning", &code_o_matic::CommandContext { args: "off".into(), ..base.clone() })
        .unwrap();
    assert_eq!(r.message, "reasoning off");
    assert!(matches!(r.action, Some(code_o_matic::CommandAction::SetReasoning(false))));

    let r = api
        .commands
        .dispatch("reasoning", &code_o_matic::CommandContext { args: "on".into(), ..base.clone() })
        .unwrap();
    assert_eq!(r.message, "reasoning on");

    // invalid arg reports current state without action
    let r = api
        .commands
        .dispatch(
            "reasoning",
            &code_o_matic::CommandContext { args: "bogus".into(), ..base.clone() },
        )
        .unwrap();
    assert!(r.message.contains("reasoning is currently off"));
    assert!(r.action.is_none());
}

#[test]
fn model_cmd_lists_discovered_and_marks_current() {
    let (_dir, api) = registered();
    let base = code_o_matic::CommandContext {
        model: "gpt-4o".into(),
        system_prompt: String::new(),
        args: String::new(),
        snapshot: json!({}),
        reasoning: false,
        available_models: vec!["gpt-4o".into(), "gpt-4o-mini".into(), "claude-3.5".into()],
    };
    // bare /model with discovered models opens the interactive picker
    let r = api.commands.dispatch("model", &base).expect("open picker");
    assert!(
        matches!(r.action, Some(code_o_matic::CommandAction::OpenModelPicker)),
        "should open picker: {r:?}"
    );
}

#[test]
fn model_cmd_switches_by_name() {
    let (_dir, api) = registered();
    let base = code_o_matic::CommandContext {
        model: "gpt-4o".into(),
        system_prompt: String::new(),
        args: "claude-3.5".into(),
        snapshot: json!({}),
        reasoning: false,
        available_models: vec!["gpt-4o".into(), "claude-3.5".into()],
    };
    let r = api.commands.dispatch("model", &base).expect("switch model");
    assert!(
        matches!(r.action, Some(code_o_matic::CommandAction::SetModel(ref m)) if m == "claude-3.5")
    );
}

#[test]
fn model_cmd_reports_empty_discovery() {
    let (_dir, api) = registered();
    let base = code_o_matic::CommandContext {
        model: "gpt-4o".into(),
        system_prompt: String::new(),
        args: String::new(),
        snapshot: json!({}),
        reasoning: false,
        available_models: Vec::new(),
    };
    let r = api.commands.dispatch("model", &base).expect("empty discovery");
    assert!(r.message.contains("no models discovered"));
}
