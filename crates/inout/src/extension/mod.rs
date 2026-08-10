//! extension loading api.
//!
//! an extension is a compiled-in rust module. it registers tools, hooks, and
//! providers against a shared api object during agent construction.

pub mod commands;
pub mod views;

use std::sync::Arc;

use crate::tools::ToolRegistry;
use crate::types::LlmRequest;

pub use commands::{
    Command, CommandAction, CommandContext, CommandHandler, CommandRegistry, CommandResult,
};
pub use views::{build_view, ViewBlock, ViewBuilder, ViewRegistry, ViewSpec, ViewTurn};

/// the api object passed to every extension during registration.
pub struct ExtensionApi {
    /// register tools for the agent loop.
    pub tools: ToolRegistry,
    /// registry of named tui views built by extensions.
    pub views: ViewRegistry,
    /// registry of slash commands registered by extensions.
    pub commands: CommandRegistry,
    /// emit an event onto the observability bus.
    pub observe: Arc<dyn Fn(String) + Send + Sync>,
    /// read-only hook to inspect an llm request before it leaves the agent.
    pub before_provider_payload: Arc<dyn Fn(&LlmRequest) + Send + Sync>,
}

impl std::fmt::Debug for ExtensionApi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExtensionApi")
            .field("tools", &self.tools)
            .field("views", &self.views)
            .field("commands", &self.commands)
            .field("observe", &"<dyn Fn>")
            .finish_non_exhaustive()
    }
}

impl ExtensionApi {
    /// create a no-op api for tests or when observability is disabled.
    pub fn noop() -> Self {
        Self {
            tools: ToolRegistry::new(),
            views: ViewRegistry::new(),
            commands: CommandRegistry::new(),
            observe: Arc::new(|_| {}),
            before_provider_payload: Arc::new(|_| {}),
        }
    }
}

