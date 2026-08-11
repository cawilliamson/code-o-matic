//! read-only inspection tools: grep, find and ls.
//!
//! these replace shelling out to `rg`/`find`/`ls` for the common cases, and
//! their presence in the active set flips the prompt guideline away from
//! "use bash for file operations". output is capped so a huge tree can't blow
//! the context window.

use std::path::PathBuf;

use async_trait::async_trait;
use regex::Regex;
use serde_json::{json, Value};
use walkdir::WalkDir;

use super::common::{required_str, resolve_path};
use crate::config::Config;
use crate::registry::Registry;
use crate::tools::{Tool, ToolError};
use crate::types::PermissionClass;

/// cap on results returned by a single inspection call.
const MAX_RESULTS: usize = 200;

/// cap on characters returned by a single call.
const MAX_CHARS: usize = 40_000;

/// directories never descended into.
fn rejected_dir(name: &str) -> bool {
    matches!(name, ".git" | "target" | "node_modules" | ".com")
}

// collect relative paths under `root` respecting skip rules, sorted, capped.
// `wanted` filters file names (e.g. an extension or glob substring); None = all.
fn collect_paths(
    repo_root: &std::path::Path,
    root: &std::path::Path,
    recursive: bool,
    wanted: Option<&str>,
    out: &mut Vec<String>,
) {
    let depth = if recursive { usize::MAX } else { 1 };
    for entry in WalkDir::new(root).max_depth(depth).into_iter().filter_entry(|e| {
        let name = e.file_name().to_string_lossy().to_string();
        let is_triv = e.depth() == 0;
        !(e.file_type().is_dir() && !is_triv && rejected_dir(&name))
    }) {
        let Ok(entry) = entry else { continue };
        if !entry.file_type().is_file() {
            continue;
        }
        if let Some(pattern) = wanted {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.contains(pattern) {
                continue;
            }
        }
        let rel = entry
            .path()
            .strip_prefix(repo_root)
            .unwrap_or(entry.path())
            .to_string_lossy()
            .to_string();
        out.push(rel);
        if out.len() >= MAX_RESULTS {
            break;
        }
    }
}

fn capped(mut s: String) -> String {
    s.truncate(MAX_CHARS);
    s
}

/// truncate-and-join helper shared by find/ls.
fn join_results(mut v: Vec<String>) -> String {
    v.sort();
    v.dedup();
    let mut out = v.join("\n");
    if out.len() > MAX_CHARS {
        out.truncate(MAX_CHARS);
        out.push_str("\n…[truncated]");
    }
    out
}

/// register every inspection tool against `api`.
pub fn register(api: &mut Registry, config: &Config) {
    api.tools.register(GrepTool::new(config.repo_root.clone()));
    api.tools.register(FindTool::new(config.repo_root.clone()));
    api.tools.register(LsTool::new(config.repo_root.clone()));
}

struct GrepTool {
    root: PathBuf,
}

impl GrepTool {
    const fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

#[async_trait]
impl Tool for GrepTool {
    fn name(&self) -> &str {
        "grep"
    }
    fn schema(&self) -> Value {
        json!({
            "name": "grep",
            "description": "Regex-search file contents under a directory (recursively). Returns matching lines as path:lineno:text, capped. Path and include are relative to the repository root.",
            "type": "object",
            "properties": {
                "pattern": { "type": "string", "description": "regular expression to match" },
                "path": { "type": "string", "description": "directory or file to search, relative to repo root (default \".\")" },
                "include": { "type": "string", "description": "only search files whose name contains this substring (e.g. \".rs\")" }
            },
            "required": ["pattern"]
        })
    }
    fn permission_class(&self) -> PermissionClass {
        PermissionClass::Read
    }
    async fn run(&self, args: Value) -> Result<String, ToolError> {
        let pattern = required_str(&args, "pattern")?;
        let re =
            Regex::new(pattern).map_err(|_| ToolError::InvalidArgs("invalid pattern".into()))?;
        let path =
            args.get("path").and_then(Value::as_str).filter(|p| !p.is_empty()).unwrap_or(".");
        let include = args.get("include").and_then(Value::as_str);
        let mut lines = Vec::new();
        let mut rels = Vec::new();
        let root = resolve_path(&self.root, path);
        collect_paths(&self.root, &root, true, include, &mut rels);
        for rel in rels {
            let Ok(content) = std::fs::read_to_string(resolve_path(&self.root, &rel)) else {
                continue;
            };
            for (idx, line) in content.lines().enumerate() {
                if re.is_match(line) {
                    lines.push(format!("{rel}:{}:{}", idx + 1, line));
                }
                if lines.len() >= MAX_RESULTS {
                    break;
                }
            }
            if lines.len() >= MAX_RESULTS {
                break;
            }
        }
        Ok(capped(lines.join("\n")))
    }
}

struct FindTool {
    root: PathBuf,
}

impl FindTool {
    const fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

#[async_trait]
impl Tool for FindTool {
    fn name(&self) -> &str {
        "find"
    }
    fn schema(&self) -> Value {
        json!({
            "name": "find",
            "description": "Locate files under a directory whose name matches a substring, recursively. Returns relative paths, capped. Use instead of bash find for discovery.",
            "type": "object",
            "properties": {
                "pattern": { "type": "string", "description": "substring the file name must contain" },
                "directory": { "type": "string", "description": "directory to search, relative to repo root (default \".\")" }
            },
            "required": ["pattern"]
        })
    }
    fn permission_class(&self) -> PermissionClass {
        PermissionClass::Read
    }
    async fn run(&self, args: Value) -> Result<String, ToolError> {
        let pattern = required_str(&args, "pattern")?;
        let dir =
            args.get("directory").and_then(Value::as_str).filter(|p| !p.is_empty()).unwrap_or(".");
        let root = resolve_path(&self.root, dir);
        let mut rels = Vec::new();
        collect_paths(&self.root, &root, true, Some(pattern), &mut rels);
        Ok(join_results(rels))
    }
}

struct LsTool {
    root: PathBuf,
}

impl LsTool {
    const fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

#[async_trait]
impl Tool for LsTool {
    fn name(&self) -> &str {
        "ls"
    }
    fn schema(&self) -> Value {
        json!({
            "name": "ls",
            "description": "List entries in a directory. Returns relative paths, directories marked with \"/\", entries with differing kinds separated. Use instead of bash ls.",
            "type": "object",
            "properties": {
                "directory": { "type": "string", "description": "directory to list, relative to repo root (default \".\")" },
                "recursive": { "type": "boolean", "description": "recurse into subdirectories (default false)" }
            }
        })
    }
    fn permission_class(&self) -> PermissionClass {
        PermissionClass::Read
    }
    async fn run(&self, args: Value) -> Result<String, ToolError> {
        let dir =
            args.get("directory").and_then(Value::as_str).filter(|p| !p.is_empty()).unwrap_or(".");
        let recursive = args.get("recursive").and_then(Value::as_bool).unwrap_or(false);
        let root = resolve_path(&self.root, dir);
        let mut entries = Vec::new();
        let mut rels = Vec::new();
        if recursive {
            collect_paths(&self.root, &root, true, None, &mut rels);
            for rel in rels {
                entries.push(rel);
            }
        } else {
            let Ok(rd) = std::fs::read_dir(&root) else {
                return Ok(String::new());
            };
            let mut items: Vec<String> = rd
                .flatten()
                .map(|e| {
                    let name = e.file_name().to_string_lossy().to_string();
                    if e.path().is_dir() {
                        format!("{name}/")
                    } else {
                        name
                    }
                })
                .collect();
            items.sort();
            entries = items;
        }
        Ok(join_results(entries))
    }
}
