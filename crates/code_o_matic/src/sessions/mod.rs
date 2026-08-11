//! code-o-matic sessions: durable session trees, branching and compaction.

#![allow(missing_docs)]

pub mod commands;
pub mod compaction;
pub mod entry;
pub mod jsonl_repo;
pub mod repo;

use std::sync::Arc;

use crate::registry::Registry;

use self::commands::{register_session_commands, CommandState};
use self::compaction::CompactionSettings;
use self::jsonl_repo::JsonlSessionRepo;

/// register the sessions subsystem against `api`.
pub fn register(api: &mut Registry) {
    let Ok(repo) = JsonlSessionRepo::new_blocking(".com/sessions") else {
        // repo dir unavailable (e.g. read-only cwd) — sessions just stay unregistered.
        return;
    };
    let repo = Arc::new(repo);

    let state = CommandState { repo, compaction: CompactionSettings::default() };

    register_session_commands(api, state);
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {

    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn registers_commands() {
        let mut api = Registry::new();
        // when: the sessions subsystem is registered
        register(&mut api);
        let names = api.commands.names();
        // then: the session, branch, switch, and compact commands are registered
        assert!(names.contains(&"sessions".to_string()));
        assert!(names.contains(&"branch".to_string()));
        assert!(names.contains(&"switch".to_string()));
        assert!(names.contains(&"compact".to_string()));
    }
}
