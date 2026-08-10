//! inout: minimal rust-native ai agent — io on the command line.
//!
//! v1 scope: conversation loop, native tools, single agent, jsonl history.
//! not v1: subagents, hub, mcp, browser, debug, parallel tool calls.

#![allow(missing_docs)]
#![allow(missing_debug_implementations)]

mod agent;

#[cfg(feature = "sessions")]
pub mod sessions;
#[cfg(feature = "skills")]
pub mod skills;

pub mod system_prompt;
pub mod history;
pub mod llm;
pub mod state;
pub mod tui;

pub use agent::Agent;
