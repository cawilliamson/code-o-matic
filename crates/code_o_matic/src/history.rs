use serde::{Deserialize, Serialize};

use crate::tools::ToolCall;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub tool_calls: Vec<ToolCall>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub reasoning: String,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct History {
    pub messages: Vec<Message>,
    pub max_turns: usize,
    #[serde(default)]
    pub system_prompt: Option<String>,
}

impl History {
    pub fn new(max_turns: usize) -> Self {
        Self { messages: Vec::new(), max_turns, system_prompt: None }
    }

    pub fn with_system_prompt(max_turns: usize, system_prompt: String) -> Self {
        Self { messages: Vec::new(), max_turns, system_prompt: Some(system_prompt) }
    }

    pub fn set_system_prompt(&mut self, prompt: String) {
        self.system_prompt = Some(prompt);
    }

    pub fn append_user(&mut self, content: String) {
        self.messages.push(Message {
            role: Role::User,
            content,
            tool_calls: Vec::new(),
            tool_call_id: None,
            reasoning: String::new(),
        });
    }

    pub fn append_assistant(&mut self, content: String) {
        self.messages.push(Message {
            role: Role::Assistant,
            content,
            tool_calls: Vec::new(),
            tool_call_id: None,
            reasoning: String::new(),
        });
    }

    pub fn append_assistant_with_reasoning(
        &mut self,
        content: String,
        reasoning: String,
        tool_calls: Vec<ToolCall>,
    ) {
        self.messages.push(Message {
            role: Role::Assistant,
            content,
            tool_calls,
            tool_call_id: None,
            reasoning,
        });
    }

    pub fn append_assistant_with_tools(&mut self, content: String, tool_calls: Vec<ToolCall>) {
        self.messages.push(Message {
            role: Role::Assistant,
            content,
            tool_calls,
            tool_call_id: None,
            reasoning: String::new(),
        });
    }

    pub fn append_tool_result(&mut self, tool_call_id: String, content: String) {
        // cap oversized tool output so one tool call can't blow the context window.
        let content = crate::config::truncate_chars(&content, crate::config::MAX_TOOL_RESULT_CHARS);
        self.messages.push(Message {
            role: Role::Tool,
            content,
            tool_calls: Vec::new(),
            tool_call_id: Some(tool_call_id),
            reasoning: String::new(),
        });
    }

    pub fn to_request(&self, model: &str, tool_schemas: &[serde_json::Value]) -> LlmRequest {
        let mut messages: Vec<Message> = Vec::new();
        if let Some(prompt) = &self.system_prompt {
            if !prompt.is_empty() {
                messages.push(Message {
                    role: Role::System,
                    content: prompt.clone(),
                    tool_calls: Vec::new(),
                    tool_call_id: None,
                    reasoning: String::new(),
                });
            }
        }
        messages.extend(self.messages.iter().cloned());
        LlmRequest { model: model.to_string(), messages, tools: tool_schemas.to_vec() }
    }

    pub fn to_jsonl(&self) -> anyhow::Result<String> {
        Ok(self
            .messages
            .iter()
            .map(serde_json::to_string)
            .collect::<Result<Vec<_>, _>>()?
            .join("\n"))
    }

    pub fn from_jsonl(s: &str) -> anyhow::Result<Self> {
        let mut messages = Vec::new();
        for line in s.lines() {
            if line.is_empty() {
                continue;
            }
            messages.push(serde_json::from_str(line)?);
        }
        Ok(Self { messages, max_turns: 20, system_prompt: None })
    }

    /// drop `count` messages starting at `index`. used by the context viewer
    /// to remove a turn from history. preserves the system prompt.
    pub fn drop_range(&mut self, index: usize, count: usize) {
        if index >= self.messages.len() {
            return;
        }
        let end = (index + count).min(self.messages.len());
        self.messages.drain(index..end);
    }

    /// clear all conversation messages, preserving the system prompt.
    pub fn clear_messages(&mut self) {
        self.messages.clear();
    }

    /// drop the last conversation turn: find the last user message, remove
    /// it and everything after it. preserves the system prompt.
    pub fn drop_last_turn(&mut self) {
        // scan backwards for the last user message
        let last_user = self.messages.iter().rposition(|m| m.role == Role::User);
        if let Some(idx) = last_user {
            self.messages.truncate(idx);
        }
    }

    /// estimate the token size of the request `to_request` would produce.
    ///
    /// rough heuristic of 1 token per 4 characters, matching the rest of the
    /// codebase. includes the system prompt, tool schemas, message content
    /// and tool-call arguments so the figure reflects what is actually sent.
    pub fn estimated_request_tokens(&self, schemas: &[serde_json::Value]) -> usize {
        let schemas_chars = serde_json::to_string(schemas).map(|s| s.chars().count()).unwrap_or(0);
        let sys_chars = self.system_prompt.as_deref().map(|s| s.chars().count()).unwrap_or(0);
        let body_chars: usize = self
            .messages
            .iter()
            .map(|m| {
                let args: usize = m
                    .tool_calls
                    .iter()
                    .map(|tc| serde_json::to_string(&tc).map(|s| s.chars().count()).unwrap_or(0))
                    .sum();
                m.content.chars().count() + args
            })
            .sum();
        (schemas_chars + sys_chars + body_chars) / 4
    }

    /// drop the oldest turns until the estimated request fits `budget_tokens`.
    ///
    /// only whole turns (user plus assistant plus tool results) are removed
    /// so tool results keep their matching tool-call ids. returns the number
    /// of message entries dropped.
    pub fn prune_for_context(
        &mut self,
        schemas: &[serde_json::Value],
        budget_tokens: usize,
    ) -> usize {
        let mut dropped = 0;
        loop {
            if self.estimated_request_tokens(schemas) <= budget_tokens {
                break;
            }
            let Some(start) = self.messages.iter().position(|m| m.role == Role::User) else {
                break;
            };
            // a turn runs from this user message up to (not including) the next.
            let end = self.messages[start + 1..]
                .iter()
                .position(|m| m.role == Role::User)
                .map(|i| start + 1 + i)
                .unwrap_or(self.messages.len());
            if end == start {
                break;
            }
            self.messages.drain(start..end);
            dropped += end - start;
        }
        dropped
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LlmRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub tools: Vec<serde_json::Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LlmResponse {
    pub content: String,
    pub tool_calls: Vec<ToolCall>,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::tools::ToolCall;

    fn history_with_turns(turns: usize, fill: usize) -> History {
        let mut h = History::new(20);
        h.set_system_prompt("sys".repeat(fill));
        for i in 0..turns {
            h.append_user(format!("user turn {i} {}", "u".repeat(fill)));
            h.append_assistant_with_tools(
                format!("assistant {i} {} ", "a".repeat(fill)),
                vec![ToolCall {
                    id: format!("c{i}"),
                    name: "bash".to_string(),
                    arguments: serde_json::json!({"command": format!("ll {i} {}", "x".repeat(fill))}),
                }],
            );
            h.append_tool_result(format!("c{i}"), format!("out {i} {}", "o".repeat(fill)));
        }
        h
    }

    #[test]
    fn prune_drops_oldest_turns_to_fit_budget() {
        let mut h = history_with_turns(50, 1000);
        let schemas: Vec<serde_json::Value> = Vec::new();
        let before = h.messages.len();
        let before_tokens = h.estimated_request_tokens(&schemas);
        assert!(before_tokens > 1_000);
        let dropped = h.prune_for_context(&schemas, 1_000);
        // then: oldest turns were removed and the estimate now fits
        assert!(dropped > 0);
        assert!(h.estimated_request_tokens(&schemas) <= 1_000);
        assert!(h.messages.len() < before);
    }

    #[test]
    fn estimate_counts_system_and_tool_payloads() {
        let mut h = History::new(20);
        h.set_system_prompt("s".repeat(400));
        h.append_user("u".repeat(400));
        h.append_assistant_with_tools(
            String::new(),
            vec![ToolCall {
                id: "c".into(),
                name: "bash".to_string(),
                arguments: serde_json::json!({"command":"pwd"}),
            }],
        );
        h.append_tool_result("c".to_string(), "o".repeat(400));
        let schemas: Vec<serde_json::Value> = Vec::new();
        // then: system prompt, content and tool result are all counted
        assert!(h.estimated_request_tokens(&schemas) >= 300);
    }
}
