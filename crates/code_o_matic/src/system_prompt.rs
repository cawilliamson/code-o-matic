//! system prompt built from the registered tool schemas.

use std::path::Path;

use crate::tools::ToolRegistry;

/// build the default system prompt describing the available tools.
///
/// prepends the project `SOUL.md` personality and `AGENTS.md` instructions
/// when present at the repo root, then lists every registered tool schema.
pub(crate) fn default_system_prompt(tools: &ToolRegistry, repo_root: &Path) -> String {
    let tool_docs: Vec<String> = tools
        .schemas()
        .iter()
        .filter_map(|s| {
            let name = s.get("name").and_then(|n| n.as_str())?;
            let desc = s.get("description").and_then(|d| d.as_str()).unwrap_or("(no description)");
            let required: Vec<String> = s
                .get("required")
                .and_then(|r| r.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default();
            let props: Vec<String> = s
                .get("properties")
                .and_then(|p| p.as_object())
                .map(|obj| {
                    obj.iter()
                        .map(|(k, v)| {
                            let ty = v.get("type").and_then(|t| t.as_str()).unwrap_or("any");
                            let pdesc = v.get("description").and_then(|d| d.as_str()).unwrap_or("");
                            let req = if required.contains(k) { " (required)" } else { "" };
                            format!("    {k}: {ty}{req} — {pdesc}")
                        })
                        .collect()
                })
                .unwrap_or_default();
            let props_str = props.join("\n");
            Some(format!("  {name}: {desc}\n{props_str}"))
        })
        .collect();
    let tool_docs_str = tool_docs.join("\n\n");

    let mut preface = String::new();
    if let Some(personality) = read_optional(repo_root, "SOUL.md") {
        preface.push_str(&format!("personality (SOUL.md):\n{personality}\n\n"));
    }
    if let Some(instructions) = read_optional(repo_root, "AGENTS.md") {
        preface.push_str(&format!("project instructions (AGENTS.md):\n{instructions}\n\n"));
    }

    let core = format!(
        "you are Code-o-matic (com), a minimal rust-native coding agent. you operate inside the \
         repo at {repo_root}. all file access is jailed to the repo root. be terse and direct.\n\n\
         available tools:\n{tool_docs_str}",
        tool_docs_str = tool_docs_str,
        repo_root = repo_root.display(),
    );
    if preface.is_empty() {
        core
    } else {
        format!("{preface}{core}")
    }
}

fn read_optional(root: &Path, name: &str) -> Option<String> {
    std::fs::read_to_string(root.join(name)).ok()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::tools::ToolRegistry;

    #[test]
    fn loads_agents_and_soul_into_prompt() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("AGENTS.md"), "no print statements").unwrap();
        std::fs::write(tmp.path().join("SOUL.md"), "be dry and terse").unwrap();
        let prompt = default_system_prompt(&ToolRegistry::new(), tmp.path());
        assert!(prompt.contains("AGENTS.md"));
        assert!(prompt.contains("no print statements"));
        assert!(prompt.contains("SOUL.md"));
        assert!(prompt.contains("be dry and terse"));
    }

    #[test]
    fn ignores_missing_context_files() {
        let tmp = tempfile::tempdir().unwrap();
        let prompt = default_system_prompt(&ToolRegistry::new(), tmp.path());
        assert!(!prompt.contains("AGENTS.md"));
        assert!(!prompt.contains("SOUL.md"));
        assert!(prompt.contains("Code-o-matic"));
    }
}
