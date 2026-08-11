//! lean system prompt. tools are additionally declared to the model natively
//! via the `tools` field on the request body; the prompt carries usage
//! guidelines so the model calls tools only when it must, never speculatively.

use std::path::Path;

/// build the default system prompt for a repo root and active tool set.
///
/// injects the project `SOUL.md` personality and `AGENTS.md` instructions when
/// present and non-empty at the repo root, then emits usage guidelines derived
/// from the currently active tools. guideline content is conditional on which
/// tools exist, mirroring how a lean model is steered away from wasteful calls.
pub(crate) fn default_system_prompt(repo_root: &Path, active_tools: &[&str]) -> String {
    let has = |name: &str| active_tools.contains(&name);

    let mut guidelines: Vec<String> = Vec::new();
    if has("bash") && !has("grep") && !has("find") && !has("ls") {
        guidelines.push("use bash for file operations like ls, rg, find".to_string());
    }
    guidelines.push("be concise in your responses".to_string());
    guidelines.push("show file paths clearly when working with files".to_string());
    guidelines.push(
        "do not call a tool unless you need to inspect or change state; answer from what you already know first"
            .to_string(),
    );

    let mut preface = String::new();
    if let Some(personality) = read_optional(repo_root, "SOUL.md") {
        preface.push_str(&format!("personality (SOUL.md):\n{personality}\n\n"));
    }
    if let Some(instructions) = read_optional(repo_root, "AGENTS.md") {
        preface.push_str(&format!("project instructions (AGENTS.md):\n{instructions}\n\n"));
    }

    let core = format!(
        "You are Code-o-matic (com), a Rust-native coding agent working in the repo at {repo_root}. \
         Be terse.\n\nGuidelines:\n{}",
        guidelines.iter().map(|g| format!("- {g}")).collect::<Vec<_>>().join("\n"),
        repo_root = repo_root.display(),
    );
    if preface.is_empty() {
        core
    } else {
        format!("{preface}{core}")
    }
}

// read a context file, skipping empty/whitespace-only content.
fn read_optional(root: &Path, name: &str) -> Option<String> {
    let raw = std::fs::read_to_string(root.join(name)).ok()?;
    if raw.trim().is_empty() {
        return None;
    }
    Some(raw)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn loads_agents_and_soul_into_prompt() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("AGENTS.md"), "no print statements").unwrap();
        std::fs::write(tmp.path().join("SOUL.md"), "be dry and terse").unwrap();
        let prompt = default_system_prompt(tmp.path(), &["read", "bash", "edit", "write"]);
        assert!(prompt.contains("AGENTS.md"));
        assert!(prompt.contains("no print statements"));
        assert!(prompt.contains("SOUL.md"));
        assert!(prompt.contains("be dry and terse"));
    }

    #[test]
    fn ignores_missing_context_files() {
        let tmp = tempfile::tempdir().unwrap();
        let prompt = default_system_prompt(tmp.path(), &["read"]);
        assert!(!prompt.contains("project instructions (AGENTS.md):"));
        assert!(!prompt.contains("personality (SOUL.md):"));
        assert!(prompt.contains("Code-o-matic"));
    }

    #[test]
    fn ignores_empty_context_files() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("AGENTS.md"), "").unwrap();
        std::fs::write(tmp.path().join("SOUL.md"), "   \n  \n").unwrap();
        let prompt = default_system_prompt(tmp.path(), &["read"]);
        assert!(!prompt.contains("project instructions (AGENTS.md):"));
        assert!(!prompt.contains("personality (SOUL.md):"));
        assert!(prompt.contains("Code-o-matic"));
    }

    #[test]
    fn discourages_speculative_tool_use() {
        let tmp = tempfile::tempdir().unwrap();
        let prompt = default_system_prompt(tmp.path(), &["read", "bash", "edit", "write"]);
        assert!(prompt.contains("do not call a tool unless you need to inspect or change state"));
    }

    #[test]
    fn steers_file_ops_to_bash_when_no_dedicated_tools() {
        let tmp = tempfile::tempdir().unwrap();
        let prompt = default_system_prompt(tmp.path(), &["read", "bash"]);
        assert!(prompt.contains("use bash for file operations like ls, rg, find"));
    }

    #[test]
    fn drops_bash_file_ops_hint_when_grep_find_ls_exist() {
        let tmp = tempfile::tempdir().unwrap();
        let prompt = default_system_prompt(tmp.path(), &["read", "bash", "grep", "find", "ls"]);
        assert!(!prompt.contains("use bash for file operations like ls, rg, find"));
    }
}
