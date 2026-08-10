//! registration surface for built-in agent functionality.
//!
//! everything that ships with code-o-matic is compiled in — there are no runtime
//! extensions. this module holds the command and view registries plus the
//! single `Registry` object used to wire built-in tools, views and slash
//! commands into the agent during construction.

pub mod commands;
pub mod views;

pub use commands::{
    Command, CommandAction, CommandContext, CommandHandler, CommandRegistry, CommandResult,
};
pub use views::{build_view, ViewBlock, ViewBuilder, ViewRegistry, ViewSpec, ViewTurn};

use crate::tools::ToolRegistry;

/// the object every built-in subsystem registers into during construction.
pub struct Registry {
    /// register tools for the agent loop.
    pub tools: ToolRegistry,
    /// registry of named tui views.
    pub views: ViewRegistry,
    /// registry of slash commands.
    pub commands: CommandRegistry,
}

impl std::fmt::Debug for Registry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Registry")
            .field("tools", &self.tools)
            .field("views", &self.views)
            .field("commands", &self.commands)
            .finish_non_exhaustive()
    }
}

impl Registry {
    /// create an empty registry for tests or headless construction.
    pub fn new() -> Self {
        Self {
            tools: ToolRegistry::new(),
            views: ViewRegistry::new(),
            commands: CommandRegistry::new(),
        }
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}
