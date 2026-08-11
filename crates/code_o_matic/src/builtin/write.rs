//! write tool: create or overwrite a file.

use std::path::PathBuf;

use async_trait::async_trait;
use serde_json::{json, Value};

use super::common::{required_str, resolve_path};
use crate::config::Config;
use crate::registry::Registry;
use crate::tools::{Tool, ToolError};
use crate::types::PermissionClass;

/// register the write tool against `api`.
pub fn register(api: &mut Registry, config: &Config) {
    api.tools.register(WriteTool::new(config.repo_root.clone()));
}

struct WriteTool {
    root: PathBuf,
}

impl WriteTool {
    const fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

#[async_trait]
impl Tool for WriteTool {
    fn name(&self) -> &str {
        "write"
    }
    fn schema(&self) -> Value {
        json!({
            "name": "write",
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "relative path to write" },
                "content": { "type": "string", "description": "content to write" }
            },
            "required": ["path", "content"]
        })
    }
    fn permission_class(&self) -> PermissionClass {
        PermissionClass::Write
    }
    async fn run(&self, args: Value) -> Result<String, ToolError> {
        let path = required_str(&args, "path")?;
        let content = required_str(&args, "content")?;
        let resolved = resolve_path(&self.root, path);
        std::fs::write(&resolved, content)?;
        Ok(format!("wrote {} bytes to {path}", content.len()))
    }
}
