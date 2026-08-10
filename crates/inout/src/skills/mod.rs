//! inout skills: markdown skill files with yaml frontmatter, trigger
//! matching, and system prompt injection.

#![allow(missing_docs)]
#![cfg_attr(test, allow(clippy::unwrap_used))]

pub mod budget;
pub mod commands;
pub mod loader;
pub mod scope;
pub mod skill;
pub mod trace;
pub mod trigger;

use std::collections::HashSet;
use std::sync::{Arc, RwLock};

use inout_core::extension::ExtensionApi;

use self::commands::{register_skill_commands, CommandState};
use self::loader::load_all_skills;
use self::scope::detect_domain_scope;

/// register the skills subsystem against `api`.
pub fn register(api: &mut ExtensionApi) {
    (api.observe)(String::from("extension_loaded:skills"));

    let skills = load_all_skills(&[]);
    let domain_scope: HashSet<String> = detect_domain_scope().into_iter().collect();

    let state = CommandState {
        skills: Arc::new(RwLock::new(skills)),
        domain_scope: Arc::new(RwLock::new(domain_scope)),
        trace: Arc::new(RwLock::new(crate::skills::trace::SkillTrace::new())),
    };

    register_skill_commands(api, state);
}

#[cfg(test)]
mod tests {
    use inout_testing::{scenario, then, when};
    use super::*;

    #[test]
    fn skills_registers_commands() {
        let mut s = scenario!("extensions", "Rust extension can register multiple surface items", "Single extension registers tool, command, and hook");
        let mut api = ExtensionApi::noop();
        when!(s, "the skills subsystem is registered", {
            register(&mut api);
            then!(s, "slash commands are registered", {
                assert!(!api.commands.names().is_empty());
            });
        });
    }
}
