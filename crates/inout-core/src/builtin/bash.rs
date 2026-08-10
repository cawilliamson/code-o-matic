//! bash tool: run a shell command with safety guards.

use std::path::PathBuf;

use async_trait::async_trait;
use serde_json::{json, Value};

use super::common::required_str;
use crate::config::Config;
use crate::extension::ExtensionApi;
use crate::tools::{Tool, ToolError};
use crate::types::PermissionClass;

const BLOCKED: [&str; 12] = [
    "rm", "dd", "mkfs", "parted", "fastboot", "shutdown", "reboot", "sudo", "curl", "wget",
    "chmod", "chown",
];

/// register the bash tool against `api`.
pub fn register(api: &mut ExtensionApi, config: &Config) {
    api.tools.register(BashTool::new(config));
}

struct BashTool {
    config: crate::config::BashConfig,
    repo_root: PathBuf,
}

impl BashTool {
    fn new(config: &Config) -> Self {
        Self { config: config.bash.clone(), repo_root: config.repo_root.clone() }
    }
}

#[async_trait]
impl Tool for BashTool {
    fn name(&self) -> &str {
        "bash"
    }
    fn schema(&self) -> Value {
        json!({
            "name": "bash",
            "type": "object",
            "properties": {
                "command": { "type": "string" }
            },
            "required": ["command"]
        })
    }
    fn permission_class(&self) -> PermissionClass {
        PermissionClass::Shell
    }
    async fn run(&self, args: Value) -> Result<String, ToolError> {
        let raw = required_str(&args, "command")?.trim().to_string();
        if raw.is_empty() {
            return Err(ToolError::InvalidArgs("command required".into()));
        }
        if raw.contains('`') || raw.contains("$(") {
            return Err(ToolError::InvalidArgs("command substitution is not allowed".into()));
        }
        if raw.contains("rm -rf") {
            return Err(ToolError::InvalidArgs("rm -rf is not allowed".into()));
        }
        let words: Vec<&str> = raw.split(' ').collect();
        let binary = words[0];
        if BLOCKED.contains(&binary) {
            return Err(ToolError::InvalidArgs(format!("{binary} is blocked")));
        }
        if !self.config.full {
            if words.iter().any(|w| w.contains('>')) {
                return Err(ToolError::InvalidArgs(
                    "shell redirection is not allowed in safe mode".into(),
                ));
            }
            if !self.config.allowlist.iter().any(|b| b == binary) {
                return Err(ToolError::InvalidArgs(format!(
                    "{binary} is not in the allowlist"
                )));
            }
        }
        run_shell(&self.config, &self.repo_root, &raw).await
    }
}

async fn run_shell(
    config: &crate::config::BashConfig,
    repo_root: &std::path::Path,
    raw: &str,
) -> Result<String, ToolError> {
    let child = tokio::process::Command::new("sh")
        .arg("-c")
        .arg(raw)
        .current_dir(repo_root)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;
    let timeout = std::time::Duration::from_secs(config.timeout_secs.max(1));
    let output = match tokio::time::timeout(timeout, child.wait_with_output()).await {
        Ok(res) => res.map_err(ToolError::Io)?,
        Err(_) => return Err(ToolError::Command("command timed out".into())),
    };
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let code = output.status.code().unwrap_or(-1);
    if code != 0 {
        return Err(ToolError::Command(format!("exit {code}: {stderr}")));
    }
    Ok(stdout)
}
