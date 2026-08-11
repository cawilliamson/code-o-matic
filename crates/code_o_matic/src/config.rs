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
    let cut = s.char_indices().take(max).map(|(i, _)| i).last().unwrap_or(0);
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
