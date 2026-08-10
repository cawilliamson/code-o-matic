//! inout sessions: durable session trees, branching, compaction and continuity.

#![allow(missing_docs)]

pub mod commands;
pub mod compaction;
pub mod continuity;
pub mod entry;
pub mod jsonl_repo;
pub mod repo;

use std::sync::Arc;

use inout_core::extension::ExtensionApi;

use self::commands::{register_session_commands, CommandState};
use self::compaction::CompactionSettings;
use self::jsonl_repo::JsonlSessionRepo;

/// register the sessions subsystem against `api`.
pub fn register(api: &mut ExtensionApi) {
    (api.observe)(String::from("extension_loaded:sessions"));

    let repo = match JsonlSessionRepo::new_blocking(".inout/sessions") {
        Ok(repo) => Arc::new(repo),
        Err(e) => {
            (api.observe)(format!("sessions_repo_error:{e}"));
            return;
        }
    };

    let state = CommandState { repo, compaction: CompactionSettings::default() };

    register_session_commands(api, state);
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use inout_testing::{scenario, then, when};
    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn extension_registers_commands() {
        let mut s = scenario!("extensions", "Command registry surface is defined", "Command registers and dispatches");
        let mut api = ExtensionApi::noop();
        when!(s, "the sessions subsystem is registered", {
            register(&mut api);
            let names = api.commands.names();
            then!(s, "the session, branch, switch, and compact commands are registered", {
                assert!(names.contains(&"sessions".to_string()));
                assert!(names.contains(&"branch".to_string()));
                assert!(names.contains(&"switch".to_string()));
                assert!(names.contains(&"compact".to_string()));
            });
        });
    }
}
