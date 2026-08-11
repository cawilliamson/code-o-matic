//! context view: sliding-window token inspector for one conversation.

use serde_json::Value;

use crate::registry::{ViewBlock, ViewSpec, ViewTurn};

pub(super) fn build(snap: &Value) -> anyhow::Result<ViewSpec> {
    let messages = snap.get("messages").and_then(Value::as_array).cloned().unwrap_or_default();
    let max_turns = snap.get("max_turns").and_then(Value::as_u64).unwrap_or(20) as usize;

    let turn_starts: Vec<usize> = messages
        .iter()
        .enumerate()
        .filter(|(_, m)| m.get("role").and_then(Value::as_str) == Some("user"))
        .map(|(i, _)| i)
        .collect();
    let total_turns = turn_starts.len();
    let window_start = total_turns.saturating_sub(max_turns);

    let total_tokens = messages.iter().map(msg_chars).sum::<usize>() / 4;

    let mut turns = Vec::new();
    for (t, &start) in turn_starts.iter().enumerate() {
        let end = if t + 1 < total_turns { turn_starts[t + 1] } else { messages.len() };
        let user_content = messages[start].get("content").and_then(Value::as_str).unwrap_or("");
        let tok = slice_chars(&messages, start, end) / 4;
        let blocks = build_blocks(&messages, start, end);
        turns.push(ViewTurn {
            msg_index: start,
            msg_count: end - start,
            preview: preview(user_content),
            tokens_est: tok,
            in_window: t >= window_start,
            blocks,
        });
    }

    Ok(ViewSpec {
        turns,
        total_tokens,
        limit_tokens: crate::config::CONTEXT_LIMIT_TOKENS,
        context_pct: 0u8,
    })
}

fn tokens(text: &str) -> usize {
    text.len() / 4
}

fn preview(text: &str) -> String {
    text.chars().take(60).collect()
}

fn msg_chars(m: &Value) -> usize {
    let content_len = m.get("content").and_then(Value::as_str).map(str::len).unwrap_or(0);
    let tool_chars: usize = m
        .get("tool_calls")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .map(|tc| {
                    tc.get("arguments_json").and_then(Value::as_str).map(str::len).unwrap_or(0)
                })
                .sum()
        })
        .unwrap_or(0);
    content_len + tool_chars
}

fn slice_chars(messages: &[Value], start: usize, end: usize) -> usize {
    messages[start..end.min(messages.len())].iter().map(msg_chars).sum()
}

fn tool_name_for(messages: &[Value], id: &str) -> String {
    for m in messages {
        if let Some(tc) = m.get("tool_calls").and_then(Value::as_array) {
            for item in tc {
                if item.get("id").and_then(Value::as_str) == Some(id) {
                    return item.get("name").and_then(Value::as_str).unwrap_or("").to_string();
                }
            }
        }
    }
    String::new()
}

fn build_blocks(messages: &[Value], start: usize, end: usize) -> Vec<ViewBlock> {
    let mut blocks = Vec::new();
    for m in &messages[start..end.min(messages.len())] {
        let role = m.get("role").and_then(Value::as_str).unwrap_or("");
        let content = m.get("content").and_then(Value::as_str).unwrap_or("");
        match role {
            "user" => {
                if !content.is_empty() {
                    blocks.push(ViewBlock::UserText {
                        text: content.to_string(),
                        tokens: tokens(content),
                    });
                }
            }
            "assistant" => {
                if !content.is_empty() {
                    blocks.push(ViewBlock::AssistantText {
                        text: content.to_string(),
                        tokens: tokens(content),
                    });
                }
                if let Some(tc) = m.get("tool_calls").and_then(Value::as_array) {
                    for item in tc {
                        let name = item.get("name").and_then(Value::as_str).unwrap_or("");
                        let input =
                            item.get("arguments_json").and_then(Value::as_str).unwrap_or("");
                        blocks.push(ViewBlock::ToolCall {
                            name: name.to_string(),
                            input_json: input.to_string(),
                            tokens: tokens(input),
                        });
                    }
                }
            }
            "tool" => {
                let id = m.get("tool_call_id").and_then(Value::as_str).unwrap_or("");
                let name = tool_name_for(messages, id);
                blocks.push(ViewBlock::ToolResult {
                    tool_name: name,
                    content: content.to_string(),
                    tokens: tokens(content),
                });
            }
            _ => {}
        }
    }
    blocks
}
