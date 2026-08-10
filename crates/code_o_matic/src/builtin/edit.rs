//! edit tool: replace the first occurrence of old_string in a file.

use async_trait::async_trait;
use serde_json::{json, Value};

use super::common::required_str;
use crate::config::Config;
use crate::registry::Registry;
use crate::jail::Jail;
use crate::tools::{Tool, ToolError};
use crate::types::PermissionClass;

/// register the edit tool against `api`.
pub fn register(api: &mut Registry, config: &Config) {
    api.tools.register(EditTool::new(Jail::new(config.repo_root.clone())));
}

struct EditTool {
    jail: Jail,
}

impl EditTool {
    const fn new(jail: Jail) -> Self {
        Self { jail }
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
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "relative path to the file inside the repo" },
                "old_string": { "type": "string", "description": "exact text to replace" },
                "new_string": { "type": "string", "description": "replacement text" }
            },
            "required": ["path", "old_string", "new_string"]
        })
    }
    fn permission_class(&self) -> PermissionClass {
        PermissionClass::Write
    }
    async fn run(&self, args: Value) -> Result<String, ToolError> {
        let path = required_str(&args, "path")?;
        let old_string = required_str(&args, "old_string")?;
        let new_string = required_str(&args, "new_string")?;
        let resolved = self.jail.resolve(path).map_err(|e| ToolError::Jail(e.to_string()))?;
        let content = std::fs::read_to_string(&resolved)?;
        let idx = content
            .find(old_string)
            .ok_or_else(|| ToolError::InvalidArgs("old_string not found".into()))?;
        let updated = format!(
            "{}{}{}",
            &content[..idx],
            new_string,
            &content[idx + old_string.len()..]
        );
        std::fs::write(&resolved, updated)?;
        Ok(format!("replaced {old_string} -> {new_string} in {path}"))
    }
}
