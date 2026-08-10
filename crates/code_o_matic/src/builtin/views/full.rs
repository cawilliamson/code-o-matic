//! full view: all llm traffic including system prompt, reasoning and tools.

use serde_json::Value;

use crate::registry::{ViewBlock, ViewTurn, ViewSpec};

pub(super) fn build(snap: &Value) -> anyhow::Result<ViewSpec> {
    let messages = snap.get("messages").and_then(Value::as_array).cloned().unwrap_or_default();
    let system_prompt = messages
        .first()
        .filter(|m| m.get("role").and_then(Value::as_str) == Some("system"))
        .and_then(|m| m.get("content").and_then(Value::as_str))
        .unwrap_or("")
        .to_string();

    let content_msgs: Vec<Value> = messages
        .iter()
        .enumerate()
        .filter(|(i, m)| !(i == &0 && m.get("role").and_then(Value::as_str) == Some("system")))
        .map(|(_, m)| m.clone())
        .collect();

    let mut turns = Vec::new();
    let mut total_tokens = 0usize;
    let limit_tokens = 128_000usize;

    let mut idx = 0usize;
    while idx < content_msgs.len() {
        let msg = &content_msgs[idx];
        if msg.get("role").and_then(Value::as_str) != Some("user") {
            idx += 1;
            continue;
        }
        let start = idx;
        let user_text = msg.get("content").and_then(Value::as_str).unwrap_or("").to_string();
        let user_tokens = tokens(&user_text);
        let mut blocks = vec![ViewBlock::UserText { text: user_text.clone(), tokens: user_tokens }];
        total_tokens += user_tokens;

        idx += 1;
        while idx < content_msgs.len()
            && content_msgs[idx].get("role").and_then(Value::as_str) != Some("user")
        {
            let m = &content_msgs[idx];
            let reasoning = m.get("reasoning").and_then(Value::as_str).unwrap_or("");
            if !reasoning.is_empty() {
                let r_tokens = tokens(reasoning);
                blocks.push(ViewBlock::AssistantText {
                    text: format!("[reasoning] {reasoning}"),
                    tokens: r_tokens,
                });
                total_tokens += r_tokens;
            }
            let content = m.get("content").and_then(Value::as_str).unwrap_or("");
            if !content.is_empty() {
                let c_tokens = tokens(content);
                blocks.push(ViewBlock::AssistantText {
                    text: content.to_string(),
                    tokens: c_tokens,
                });
                total_tokens += c_tokens;
            }
            if let Some(tc) = m.get("tool_calls").and_then(Value::as_array) {
                for item in tc {
                    let name = item.get("name").and_then(Value::as_str).unwrap_or("");
                    let arg_str = item.get("arguments_json").and_then(Value::as_str).unwrap_or("");
                    let tc_tokens = (name.len() + arg_str.len()) / 4;
                    blocks.push(ViewBlock::ToolCall {
                        name: name.to_string(),
                        input_json: arg_str.to_string(),
                        tokens: tc_tokens,
                    });
                    total_tokens += tc_tokens;
                }
            }
            if m.get("role").and_then(Value::as_str) == Some("tool") {
                let tr = m.get("content").and_then(Value::as_str).unwrap_or("");
                let tr_tokens = tokens(tr);
                blocks.push(ViewBlock::ToolResult {
                    tool_name: "tool".into(),
                    content: tr.to_string(),
                    tokens: tr_tokens,
                });
                total_tokens += tr_tokens;
            }
            idx += 1;
        }

        let turn_tokens = blocks.iter().map(block_tokens).sum();
        turns.push(ViewTurn {
            msg_index: start,
            msg_count: idx - start,
            preview: preview(&user_text),
            tokens_est: turn_tokens,
            in_window: true,
            blocks,
        });
    }

    if !system_prompt.is_empty() {
        let sp_tokens = tokens(&system_prompt);
        total_tokens += sp_tokens;
        turns.insert(
            0,
            ViewTurn {
                msg_index: 0,
                msg_count: 0,
                preview: "[system prompt]".into(),
                tokens_est: sp_tokens,
                in_window: true,
                blocks: vec![ViewBlock::AssistantText {
                    text: system_prompt,
                    tokens: sp_tokens,
                }],
            },
        );
    }

    let context_pct = ((total_tokens.saturating_mul(100))
        .checked_div(limit_tokens)
        .unwrap_or(0))
        .min(100) as u8;

    Ok(ViewSpec { turns, total_tokens, limit_tokens, context_pct })
}

fn tokens(text: &str) -> usize {
    text.len() / 4
}

fn preview(text: &str) -> String {
    text.chars().take(60).collect()
}

fn block_tokens(b: &ViewBlock) -> usize {
    match b {
        ViewBlock::UserText { tokens, .. }
        | ViewBlock::AssistantText { tokens, .. }
        | ViewBlock::ToolCall { tokens, .. }
        | ViewBlock::ToolResult { tokens, .. } => *tokens,
    }
}
