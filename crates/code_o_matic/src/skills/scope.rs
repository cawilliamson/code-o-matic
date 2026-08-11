//! stack auto-detection from project manifest files.

use std::collections::HashSet;
use std::path::Path;

/// manifest files and the skill names they imply.
const STACK_MANIFESTS: &[(&str, &[&str])] = &[
    ("Cargo.toml", &["rust"]),
    ("package.json", &["typescript", "react"]),
    ("pyproject.toml", &["python"]),
    ("go.mod", &["go"]),
    ("pom.xml", &["java"]),
];

/// detect the current project's domain scope by looking for manifest files.
///
/// walks upward from the current directory until a marker is found or the root
/// is reached. returns deduplicated skill names.
#[must_use]
pub fn detect_domain_scope() -> Vec<String> {
    let mut seen = HashSet::new();
    let mut dir = std::env::current_dir().unwrap_or_else(|_| Path::new(".").to_path_buf());

    loop {
        for (file, skills) in STACK_MANIFESTS {
            if dir.join(file).is_file() {
                for skill in *skills {
                    seen.insert(skill.to_string());
                }
            }
        }

        match dir.parent() {
            Some(parent) => dir = parent.to_path_buf(),
            None => break,
        }
    }

    let mut out: Vec<String> = seen.into_iter().collect();
    out.sort();
    out
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn cargo_toml_maps_to_rust_domain() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]\n").unwrap();
        let cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();
        // when: stack detection runs
        let scope = detect_domain_scope();
        std::env::set_current_dir(cwd).unwrap();
        // then: the domain scope includes rust
        assert!(scope.contains(&String::from("rust")));
    }
}
