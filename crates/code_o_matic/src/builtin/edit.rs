//! edit tool: apply one or more exact-text replacements to a file.

use std::path::PathBuf;

use async_trait::async_trait;
use serde_json::{json, Value};

use super::common::{required_str, resolve_path};
use crate::config::Config;
use crate::registry::Registry;
use crate::tools::{Tool, ToolError};
use crate::types::PermissionClass;

/// register the edit tool against `api`.
pub fn register(api: &mut Registry, config: &Config) {
    api.tools.register(EditTool::new(config.repo_root.clone()));
}

struct EditTool {
    root: PathBuf,
}

impl EditTool {
    const fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

#[async_trait]
impl Tool for EditTool {
    fn name(&self) -> &str {
        "edit"
    }
    fn schema(&self) -> Value {
        json!({
            "name": "edit",
            "description": "Apply exact-text replacements to a file. Provide an edits array of {oldText,newText} pairs; each oldText must match exactly (including surrounding whitespace and indentation) and must be unique in the file — include more context if it is ambiguous. Pass every change in one call rather than splitting across calls. Falls back to a single old_string/new_string pair for one-off edits. Path is relative to the repository root.",
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "path to the file to edit, relative to the repository root" },
                "edits": { "type": "array", "description": "replacements to apply, in order", "items": { "type": "object", "properties": { "oldText": { "type": "string" }, "newText": { "type": "string" } }, "required": ["oldText", "newText"] } },
                "old_string": { "type": "string", "description": "(single-edit form) exact text to replace" },
                "new_string": { "type": "string", "description": "(single-edit form) replacement text" }
            },
            "required": ["path"]
        })
    }
    fn permission_class(&self) -> PermissionClass {
        PermissionClass::Write
    }
    async fn run(&self, args: Value) -> Result<String, ToolError> {
        let path = required_str(&args, "path")?;
        let resolved = resolve_path(&self.root, path);
        let content = std::fs::read_to_string(&resolved)?;

        // multi-edit form: apply an ordered list of unique replacements.
        if let Some(edits) = args.get("edits").and_then(Value::as_array) {
            if !edits.is_empty() {
                let mut updated = content;
                for ed in edits {
                    let old_text = ed
                        .get("oldText")
                        .and_then(Value::as_str)
                        .ok_or_else(|| ToolError::InvalidArgs("each edit needs oldText".into()))?;
                    let new_text = ed
                        .get("newText")
                        .and_then(Value::as_str)
                        .ok_or_else(|| ToolError::InvalidArgs("each edit needs newText".into()))?;
                    updated = apply_unique(&updated, old_text, new_text, path)?;
                }
                std::fs::write(&resolved, updated)?;
                return Ok(format!("applied {} edits to {path}", edits.len()));
            }
        }

        // single-edit fallback: replace the first occurrence.
        let old_string = required_str(&args, "old_string")?;
        let new_string = required_str(&args, "new_string")?;
        let idx = content
            .find(old_string)
            .ok_or_else(|| ToolError::InvalidArgs("old_string not found".into()))?;
        let updated =
            format!("{}{}{}", &content[..idx], new_string, &content[idx + old_string.len()..]);
        std::fs::write(&resolved, updated)?;
        Ok(format!("replaced {old_string} -> {new_string} in {path}"))
    }
}

// replace `old` with `new`, requiring it to occur exactly once. errors on
// absence (helpless without more context) or ambiguity (multiple matches).
fn apply_unique(content: &str, old: &str, new: &str, path: &str) -> Result<String, ToolError> {
    if old.is_empty() {
        return Err(ToolError::InvalidArgs("oldText must not be empty".into()));
    }
    let mut start = None;
    let mut count = 0usize;
    let mut from = 0usize;
    while let Some(idx) = content[from..].find(old) {
        let abs = from + idx;
        count += 1;
        if count == 1 {
            start = Some(abs);
        }
        from = abs + old.len();
    }
    let start = match (count, start) {
        (1, Some(s)) => s,
        (0, _) => {
            return Err(ToolError::InvalidArgs(format!(
                "{path}: oldText not found — provide more context or create the file"
            )))
        }
        _ => {
            return Err(ToolError::InvalidArgs(format!(
                "{path}: oldText appears {count} times — include more surrounding context to disambiguate"
            )))
        }
    };
    Ok(format!("{}{}{}", &content[..start], new, &content[start + old.len()..]))
}
