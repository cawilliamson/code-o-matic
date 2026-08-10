//! inout: minimal rust-native ai agent — io on the command line.
//!
//! v1 scope: conversation loop, native tools, single agent, jsonl history.
//! not v1: subagents, hub, mcp, browser, debug, parallel tool calls.

#![allow(missing_docs)]
#![allow(missing_debug_implementations)]

mod agent;

pub mod builtin;
pub mod config;
pub mod extension;
pub mod history;
pub mod hooks;
pub mod jail;
pub mod llm;
#[cfg(feature = "sessions")]
pub mod sessions;
#[cfg(feature = "skills")]
pub mod skills;
pub mod state;
pub mod system_prompt;
pub mod tools;
pub mod tui;
pub mod types;

pub use agent::Agent;
pub use builtin::register_builtins;
pub use config::{BashConfig, Config};
pub use extension::{
    build_view, Command, CommandAction, CommandContext, CommandHandler, CommandRegistry,
    CommandResult, ExtensionApi, ViewBlock, ViewBuilder, ViewRegistry, ViewSpec, ViewTurn,
};
pub use hooks::HookBus;
pub use jail::{Jail, JailError};
pub use tools::{Tool, ToolCall, ToolError, ToolRegistry};
pub use types::{ContentBlock, LlmRequest, LlmResponse, Message, PermissionClass, Role, Usage};
