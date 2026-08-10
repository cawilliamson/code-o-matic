//! shared helpers for the built-in tools.

use serde_json::Value;

use crate::jail::Jail;
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

/// resolve `path` against the jail and read it as a string.
pub fn read_via_jail(jail: &Jail, path: &str) -> Result<String, ToolError> {
    let resolved = jail.resolve(path).map_err(|e| ToolError::Jail(e.to_string()))?;
    std::fs::read_to_string(&resolved).map_err(ToolError::Io)
}
