//! shared helpers for the built-in tools.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::tools::ToolError;

/// read a required string argument.
pub fn required_str<'a>(args: &'a Value, key: &str) -> Result<&'a str, ToolError> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| ToolError::InvalidArgs(format!("{key} required")))
}

/// read an optional positive integer argument, defaulting when absent.
pub fn opt_usize(args: &Value, key: &str, def: usize) -> usize {
    args.get(key)
        .and_then(|v| v.as_u64().or_else(|| v.as_i64().map(|i| i as u64)))
        .filter(|&n| n > 0)
        .map(|n| n as usize)
        .unwrap_or(def)
}

/// resolve a tool-supplied path: absolute paths used as-is, relative paths
/// joined to the repo root (the working base). no validation — any dir is allowed.
pub fn resolve_path(repo_root: &Path, path: &str) -> PathBuf {
    let p = Path::new(path);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        repo_root.join(p)
    }
}

/// resolve `path` against the repo root and read it as a string.
pub fn read_at(repo_root: &Path, path: &str) -> Result<String, ToolError> {
    std::fs::read_to_string(resolve_path(repo_root, path)).map_err(ToolError::Io)
}
