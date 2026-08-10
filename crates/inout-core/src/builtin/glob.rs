//! glob tool: recursively list entries matching a simple glob pattern.

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::config::Config;
use crate::extension::ExtensionApi;
use crate::jail::Jail;
use crate::tools::{Tool, ToolError};
use crate::types::PermissionClass;

/// register the glob tool against `api`.
pub fn register(api: &mut ExtensionApi, config: &Config) {
    api.tools.register(GlobTool::new(Jail::new(config.repo_root.clone())));
}

struct GlobTool {
    jail: Jail,
}

impl GlobTool {
    const fn new(jail: Jail) -> Self {
        Self { jail }
    }
}

#[async_trait]
impl Tool for GlobTool {
    fn name(&self) -> &str {
        "glob"
    }
    fn schema(&self) -> Value {
        json!({
            "name": "glob",
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "relative directory under the jail root" },
                "pattern": { "type": "string", "description": "glob pattern supporting * and ?, matched against entry names" }
            },
            "required": ["path"]
        })
    }
    fn permission_class(&self) -> PermissionClass {
        PermissionClass::Read
    }
    async fn run(&self, args: Value) -> Result<String, ToolError> {
        let rel_path = args.get("path").and_then(Value::as_str).unwrap_or("");
        let pattern = args.get("pattern").and_then(Value::as_str).unwrap_or("*");
        let resolved = self.jail.resolve(rel_path).map_err(|e| ToolError::Jail(e.to_string()))?;
        let mut entries: Vec<String> = Vec::new();
        walk(&resolved, &self.jail.root, &mut entries)?;
        entries.sort();
        let matched: Vec<String> = entries
            .into_iter()
            .filter(|entry| {
                let name = entry.rsplit('/').next().unwrap_or(entry);
                match_glob(pattern, name)
            })
            .collect();
        Ok(matched.join("\n"))
    }
}

/// recursively collect every path under `root` as a repo-relative string.
fn walk(
    root: &std::path::Path,
    jail_root: &std::path::Path,
    out: &mut Vec<String>,
) -> std::io::Result<()> {
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        let rel = path.strip_prefix(jail_root).unwrap_or(&path);
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        if !rel_str.is_empty() {
            out.push(rel_str);
        }
        if path.is_dir() {
            walk(&path, jail_root, out)?;
        }
    }
    Ok(())
}

/// simple glob matcher supporting `*` and `?` (case sensitive).
fn match_glob(pattern: &str, name: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let n: Vec<char> = name.chars().collect();
    glob_rec(&p, 0, &n, 0)
}

fn glob_rec(p: &[char], pi: usize, n: &[char], ni: usize) -> bool {
    if pi == p.len() {
        return ni == n.len();
    }
    let pc = p[pi];
    if pc == '*' {
        let mut k = ni;
        while k <= n.len() {
            if glob_rec(p, pi + 1, n, k) {
                return true;
            }
            k += 1;
        }
        return false;
    }
    if ni == n.len() {
        return false;
    }
    if pc == '?' {
        return glob_rec(p, pi + 1, n, ni + 1);
    }
    if pc == n[ni] {
        return glob_rec(p, pi + 1, n, ni + 1);
    }
    false
}
