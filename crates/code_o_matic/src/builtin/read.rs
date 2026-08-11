//! read tool: read a file slice by line range.

use std::path::PathBuf;

use async_trait::async_trait;
use serde_json::{json, Value};

use super::common::{opt_usize, read_at, required_str};
use crate::config::Config;
use crate::registry::Registry;
use crate::tools::{Tool, ToolError};
use crate::types::PermissionClass;

/// register the read tool against `api`.
pub fn register(api: &mut Registry, config: &Config) {
    api.tools.register(ReadTool::new(config.repo_root.clone()));
}

struct ReadTool {
    root: PathBuf,
}

impl ReadTool {
    const fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

#[async_trait]
impl Tool for ReadTool {
    fn name(&self) -> &str {
        "read"
    }
    fn schema(&self) -> Value {
        json!({
            "name": "read",
            "description": "Read the contents of a text file. Output is truncated to 2000 lines or 50KB (whichever is hit first). Use offset/limit for large files; when you need the whole file, continue with offset until complete. Path is relative to the repository root.",
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "path to the file to read, relative to the repository root" },
                "offset": { "type": "integer", "description": "1-based starting line (default 1)" },
                "limit": { "type": "integer", "description": "maximum lines to return; 0 means all remaining lines" }
            },
            "required": ["path"]
        })
    }
    fn permission_class(&self) -> PermissionClass {
        PermissionClass::Read
    }
    async fn run(&self, args: Value) -> Result<String, ToolError> {
        let path = required_str(&args, "path")?;
        let content = read_at(&self.root, path)?;
        let offset = opt_usize(&args, "offset", 1).max(1);
        let limit = opt_usize(&args, "limit", 0);
        let lines: Vec<&str> = content.split('\n').collect();
        let total = lines.len();
        let start = offset - 1;
        if start >= total {
            return Ok(String::new());
        }
        let end = if limit == 0 { total } else { (start + limit).min(total) };
        Ok(lines[start..end].join("\n"))
    }
}
