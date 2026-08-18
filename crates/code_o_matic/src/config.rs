//! shared configuration structs.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// token context window the agent assumes for its model: 256k, matching the
/// configured context length on the twobobs gateway (bob1/bob2).
pub const CONTEXT_LIMIT_TOKENS: usize = 256 * 1024;

/// cap on characters kept from a single tool result or context file, so one
/// oversized output can't blow the context window. ~20k tokens at worst.
pub const MAX_TOOL_RESULT_CHARS: usize = 80_000;

/// truncate `s` to at most `max` characters, cut on a char boundary, and append
/// a visible marker so the model knows content was dropped.
pub(crate) fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    // byte offset just past the `max`-th character (never splits a char)
    let cut = s.char_indices().nth(max).map(|(i, _)| i).unwrap_or(s.len());
    format!("{}…[truncated]", &s[..cut])
}

/// global agent configuration.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    /// root directory the agent treats as the project.
    pub repo_root: PathBuf,
    /// llm provider identifier (twobobs gateway).
    pub llm_provider: String,
    /// model identifier passed to the provider.
    pub model: String,
    /// models the endpoint advertises, discovered at launch (may be empty).
    pub available_models: Vec<String>,
    /// maximum conversation turns before compaction.
    pub max_turns: usize,
    /// bash execution policy.
    pub bash: BashConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            repo_root: PathBuf::from("."),
            llm_provider: String::from("twobobs"),
            model: String::from("deepseek-v4-flash-abliterated"),
            available_models: Vec::new(),
            max_turns: 20,
            bash: BashConfig::default(),
        }
    }
}

/// bash tool policy.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BashConfig {
    /// command timeout in seconds.
    pub timeout_secs: u64,
}

impl Default for BashConfig {
    fn default() -> Self {
        Self { timeout_secs: 30 }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn truncate_keeps_short_input_unchanged() {
        assert_eq!(truncate_chars("hello", 10), "hello");
        assert_eq!(truncate_chars("hello", 5), "hello");
    }

    #[test]
    fn truncate_marks_dropped_content() {
        // 'z' never appears in "[truncated]" so the count is clean
        let out = truncate_chars(&"z".repeat(100), 10);
        assert!(out.contains("[truncated]"), "missing marker: {out}");
        assert_eq!(out.matches('z').count(), 10);
    }

    #[test]
    fn truncate_cuts_on_char_boundary() {
        // each "é" is 2 utf-8 bytes; truncation must not split one
        let s = "é".repeat(50);
        let out = truncate_chars(&s, 5);
        assert_eq!(out.matches('é').count(), 5);
        assert!(out.ends_with("[truncated]"));
    }

    #[test]
    fn truncate_handles_ascii_prefix_with_multibyte_tail() {
        let s = format!("{}\u{1F600}", "x".repeat(20)); // 20 ascii + 1 emoji
        let out = truncate_chars(&s, 3);
        assert_eq!(out, "xxx…[truncated]");
    }
}
