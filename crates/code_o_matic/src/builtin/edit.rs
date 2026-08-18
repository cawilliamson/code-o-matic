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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn apply(content: &str, old: &str, new: &str) -> Result<String, ToolError> {
        apply_unique(content, old, new, "test.txt")
    }

    #[test]
    fn unique_match_is_replaced() {
        assert_eq!(apply("a b c", " b ", " X ").unwrap(), "a X c");
    }

    #[test]
    fn replaces_at_content_boundaries() {
        assert_eq!(apply("start tail", "start", "X").unwrap(), "X tail");
        assert_eq!(apply("head end", "end", "Y").unwrap(), "head Y");
    }

    #[test]
    fn absent_match_errors_with_guidance() {
        let err = apply("hello", "xyz", "q").unwrap_err();
        match err {
            ToolError::InvalidArgs(msg) => assert!(msg.contains("not found"), "{msg}"),
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn ambiguous_match_errors() {
        let err = apply("a a a", "a", "b").unwrap_err();
        match err {
            ToolError::InvalidArgs(msg) => assert!(msg.contains("times"), "{msg}"),
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn repeated_substring_counted_as_multiple_occurrences() {
        // "bb" occurs at index 0 and 2 in "bbbb" → ambiguous
        assert!(apply("bbbb", "bb", "X").is_err());
    }

    #[test]
    fn empty_old_text_is_rejected() {
        assert!(apply("abc", "", "x").is_err());
    }

    #[test]
    fn empty_replacement_is_a_deletion() {
        assert_eq!(apply("abc", "b", "").unwrap(), "ac");
    }

    #[test]
    fn multibyte_content_is_not_split() {
        assert_eq!(apply("éclair", "é", "E").unwrap(), "Eclair");
        assert_eq!(apply("café au lait", "café", "tea").unwrap(), "tea au lait");
    }

    #[test]
    fn single_edit_never_touches_later_occurrence() {
        // two "!" are ambiguous and must error
        assert!(apply("a!b!c", "!", "x").is_err(), "two ! are ambiguous");
        assert_eq!(apply("a!b!c", "b", "z").unwrap(), "a!z!c");
    }
}
