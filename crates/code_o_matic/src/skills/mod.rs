//! code-o-matic skills: markdown skill files with yaml frontmatter, trigger
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

use crate::registry::Registry;

use self::commands::{register_skill_commands, CommandState};
use self::loader::load_all_skills;
use self::scope::detect_domain_scope;

/// register the skills subsystem against `api`.
pub fn register(api: &mut Registry) {
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

    use super::*;

    #[test]
    fn skills_registers_commands() {
        let mut api = Registry::new();
        // when: the skills subsystem is registered
        register(&mut api);
        // then: slash commands are registered
        assert!(!api.commands.names().is_empty());
    }
}
