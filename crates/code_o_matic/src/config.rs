//! shared configuration structs.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// token context window the agent assumes for its model: 256k, matching the
/// configured context length on the twobobs gateway (bob1/bob2).
pub const CONTEXT_LIMIT_TOKENS: usize = 256 * 1024;

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
    /// whether to emit trace/spans to the observability bus.
    pub observability: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            repo_root: PathBuf::from("."),
            llm_provider: String::from("twobobs"),
            model: String::from("deepseek-v4-flash-abliterated"),
            max_turns: 20,
            bash: BashConfig::default(),
            observability: false,
        }
    }
}

/// bash tool policy.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BashConfig {
    /// if true, any binary on `$PATH` is allowed (dangerous).
    pub full: bool,
    /// allowed binaries in safe mode.
    pub allowlist: Vec<String>,
    /// command timeout in seconds.
    pub timeout_secs: u64,
}

impl Default for BashConfig {
    fn default() -> Self {
        Self { full: false, allowlist: Self::safe_defaults(), timeout_secs: 30 }
    }
}

impl BashConfig {
    /// default safe-mode allowlist.
    pub fn safe_defaults() -> Vec<String> {
        [
            "basename", "cat", "cp", "cargo", "date", "diff", "dirname", "du", "echo",
            "env", "find", "git", "head", "hostname", "ls", "mkdir", "mv", "printf",
            "pwd", "rg", "sed", "sort", "stat", "tail", "touch", "uname", "uniq", "wc",
            "which", "whoami",
        ]
        .iter()
        .map(ToString::to_string)
        .collect()
    }
}
