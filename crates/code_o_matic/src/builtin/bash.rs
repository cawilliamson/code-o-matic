//! bash tool: run an arbitrary shell command.

use std::path::PathBuf;

use async_trait::async_trait;
use serde_json::{json, Value};

use super::common::required_str;
use crate::config::Config;
use crate::registry::Registry;
use crate::tools::{Tool, ToolError};
use crate::types::PermissionClass;

/// register the bash tool against `api`.
pub fn register(api: &mut Registry, config: &Config) {
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
            "description": "Execute a bash command in the repository root. Returns stdout and stderr. Output is truncated to the last 2000 lines or 50KB. Use for operations no dedicated tool covers (e.g. git, tests, builds, arbitrary shell).",
            "type": "object",
            "properties": {
                "command": { "type": "string", "description": "shell command to execute" }
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
        run_shell(&self.config, &self.repo_root, &raw).await
    }
}

async fn run_shell(
    config: &crate::config::BashConfig,
    repo_root: &std::path::Path,
    raw: &str,
) -> Result<String, ToolError> {
    let mut child = tokio::process::Command::new("sh")
        .arg("-c")
        .arg(raw)
        .current_dir(repo_root)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;
    // take the output pipes first and read them on their own tasks so a slow or
    // chatty command is drained concurrently instead of deadlocking on a full pipe.
    let stdout_pipe = child.stdout.take();
    let stderr_pipe = child.stderr.take();
    // keep the child inside a shared slot so an raii guard can kill it if this
    // future is dropped before the command exits (e.g. the turn is interrupted by
    // esc). nothing else contends for this lock, so holding it across `wait` is fine.
    let shared = std::sync::Arc::new(tokio::sync::Mutex::new(Some(child)));
    struct KillOnDrop(std::sync::Arc<tokio::sync::Mutex<Option<tokio::process::Child>>>);
    impl Drop for KillOnDrop {
        fn drop(&mut self) {
            // never blocks a runtime thread: if the lock is contended we skip and
            // leave the command to finish in the background (harmless).
            let Ok(mut guard) = self.0.try_lock() else { return };
            if let Some(mut c) = guard.take() {
                let _ = c.start_kill();
            }
        }
    }
    let _guard = KillOnDrop(shared.clone());

    let stdout_task = tokio::task::spawn(read_pipe(stdout_pipe));
    let stderr_task = tokio::task::spawn(read_pipe(stderr_pipe));
    let timeout = std::time::Duration::from_secs(config.timeout_secs.max(1));
    let status = {
        let mut guard = shared.lock().await;
        let child = guard.as_mut().expect("bash child present");
        match tokio::time::timeout(timeout, child.wait()).await {
            Ok(s) => s.map_err(ToolError::Io)?,
            Err(_) => {
                let _ = child.start_kill();
                return Err(ToolError::Command("command timed out".into()));
            }
        }
    };
    // the child has exited, so both pipes have hit eof and the read tasks are done.
    let stdout = match stdout_task.await {
        Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(_) => String::new(),
    };
    let stderr = match stderr_task.await {
        Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(_) => String::new(),
    };
    drop(shared);
    let code = status.code().unwrap_or(-1);
    if code != 0 {
        return Err(ToolError::Command(format!("exit {code}: {stderr}")));
    }
    Ok(stdout)
}

use tokio::io::{AsyncRead, AsyncReadExt};

async fn read_pipe<R: AsyncRead + Unpin>(mut pipe: Option<R>) -> Vec<u8> {
    let mut buf = Vec::new();
    if let Some(h) = pipe.as_mut() {
        let _ = h.read_to_end(&mut buf).await;
    }
    buf
}
