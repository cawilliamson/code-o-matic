//! minimal system prompt. tools are declared to the model natively via the
//! `tools` field on the request body, so nothing tool-related lives here.

use std::path::Path;

/// build the lean default system prompt for a repo root.
///
/// injects the project `SOUL.md` personality and `AGENTS.md` instructions when
/// present and non-empty at the repo root, keeping the core itself to a single
/// identity line. the tool list is omitted: tools go over the wire structurally,
/// and context files are injected at full length.
pub(crate) fn default_system_prompt(repo_root: &Path) -> String {
    let mut preface = String::new();
    if let Some(personality) = read_optional(repo_root, "SOUL.md") {
        preface.push_str(&format!("personality (SOUL.md):\n{personality}\n\n"));
    }
    if let Some(instructions) = read_optional(repo_root, "AGENTS.md") {
        preface.push_str(&format!("project instructions (AGENTS.md):\n{instructions}\n\n"));
    }

    let core = format!(
        "You are Code-o-matic (com), a Rust-native coding agent working in the repo at {repo_root}. \
         Be terse.",
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
        let prompt = default_system_prompt(tmp.path());
        assert!(prompt.contains("AGENTS.md"));
        assert!(prompt.contains("no print statements"));
        assert!(prompt.contains("SOUL.md"));
        assert!(prompt.contains("be dry and terse"));
    }

    #[test]
    fn ignores_missing_context_files() {
        let tmp = tempfile::tempdir().unwrap();
        let prompt = default_system_prompt(tmp.path());
        assert!(!prompt.contains("project instructions (AGENTS.md):"));
        assert!(!prompt.contains("personality (SOUL.md):"));
        assert!(prompt.contains("Code-o-matic"));
    }

    #[test]
    fn ignores_empty_context_files() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("AGENTS.md"), "").unwrap();
        std::fs::write(tmp.path().join("SOUL.md"), "   \n  \n").unwrap();
        let prompt = default_system_prompt(tmp.path());
        assert!(!prompt.contains("project instructions (AGENTS.md):"));
        assert!(!prompt.contains("personality (SOUL.md):"));
        assert!(prompt.contains("Code-o-matic"));
    }
}
