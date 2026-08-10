//! grep tool: return lines from a file containing a substring pattern.

use async_trait::async_trait;
use serde_json::{json, Value};

use super::common::{read_via_jail, required_str};
use crate::config::Config;
use crate::extension::ExtensionApi;
use crate::jail::Jail;
use crate::tools::{Tool, ToolError};
use crate::types::PermissionClass;

/// register the grep tool against `api`.
pub fn register(api: &mut ExtensionApi, config: &Config) {
    api.tools.register(GrepTool::new(Jail::new(config.repo_root.clone())));
}

struct GrepTool {
    jail: Jail,
}

impl GrepTool {
    const fn new(jail: Jail) -> Self {
        Self { jail }
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
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "relative path to the file" },
                "pattern": { "type": "string", "description": "substring to search for" },
                "case_sensitive": { "type": "boolean", "description": "match case sensitively (default true)" }
            },
            "required": ["path", "pattern"]
        })
    }
    fn permission_class(&self) -> PermissionClass {
        PermissionClass::Read
    }
    async fn run(&self, args: Value) -> Result<String, ToolError> {
        let path = required_str(&args, "path")?;
        let pattern = required_str(&args, "pattern")?;
        let case_sensitive = args.get("case_sensitive").and_then(Value::as_bool).unwrap_or(true);
        let content = read_via_jail(&self.jail, path)?;
        let needle = if case_sensitive {
            pattern.to_string()
        } else {
            pattern.to_lowercase()
        };
        let matched: Vec<&str> = content
            .split('\n')
            .filter(|line| {
                let hay = if case_sensitive {
                    (*line).to_string()
                } else {
                    line.to_lowercase()
                };
                hay.contains(&needle)
            })
            .collect();
        Ok(matched.join("\n"))
    }
}
