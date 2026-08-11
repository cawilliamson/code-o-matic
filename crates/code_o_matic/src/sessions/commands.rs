//! `/sessions`, `/branch`, `/switch` and `/compact` slash commands.

use std::sync::Arc;

use crate::registry::{Command, CommandContext, CommandHandler, CommandResult, Registry};

use crate::sessions::compaction::{compact, CompactionSettings};
use crate::sessions::entry::{EntryBase, SessionEntry, SessionInfoEntry};
use crate::sessions::jsonl_repo::JsonlSessionRepo;
use crate::sessions::repo::{ForkOptions, ListOptions, SessionRepo};

/// shared state for session command handlers.
#[derive(Clone, Debug)]
pub struct CommandState {
    /// underlying session repository.
    pub repo: Arc<JsonlSessionRepo>,
    /// compaction settings.
    pub compaction: CompactionSettings,
}

/// register session slash commands.
pub fn register_session_commands(api: &mut Registry, state: CommandState) {
    let names = ["sessions", "branch", "switch", "compact", "name", "changelog", "export"];
    let handlers: Vec<CommandHandler> = vec![
        CommandHandler::Rust(Arc::new({
            let state = state.clone();
            move |ctx: &CommandContext| sessions_list(ctx, &state)
        })),
        CommandHandler::Rust(Arc::new({
            let state = state.clone();
            move |ctx: &CommandContext| branch(ctx, &state)
        })),
        CommandHandler::Rust(Arc::new({
            let state = state.clone();
            move |ctx: &CommandContext| switch(ctx, &state)
        })),
        CommandHandler::Rust(Arc::new({
            let state = state.clone();
            move |ctx: &CommandContext| compact_cmd(ctx, &state)
        })),
        CommandHandler::Rust(Arc::new({
            let state = state.clone();
            move |ctx: &CommandContext| name_cmd(ctx, &state)
        })),
        CommandHandler::Rust(Arc::new({
            let state = state.clone();
            move |ctx: &CommandContext| changelog_cmd(ctx, &state)
        })),
        CommandHandler::Rust(Arc::new({
            let state = state.clone();
            move |ctx: &CommandContext| export_cmd(ctx, &state)
        })),
    ];

    for (name, handler) in names.iter().zip(handlers) {
        api.commands.register(Command {
            name: name.to_string(),
            description: format!("session command: {name}"),
            handler,
        });
    }
}

fn sessions_list(ctx: &CommandContext, state: &CommandState) -> anyhow::Result<CommandResult> {
    let opts = ListOptions {
        prefix: if ctx.args.trim().is_empty() { None } else { Some(ctx.args.trim().to_string()) },
        ..ListOptions::default()
    };

    let list = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(state.repo.list(opts))
    })?;

    let mut lines = Vec::new();
    lines.push("branches:".to_string());
    for info in list {
        lines.push(format!("  {} - {}", info.id, info.title));
    }

    Ok(CommandResult { message: lines.join("\n"), action: None })
}

fn branch(ctx: &CommandContext, state: &CommandState) -> anyhow::Result<CommandResult> {
    let name = ctx.args.trim();
    if name.is_empty() {
        return Ok(CommandResult { message: "usage: /branch <name>".to_string(), action: None });
    }

    let leaf_id = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(state.repo.get_metadata())
    })?
    .leaf_id;

    let new_leaf = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current()
            .block_on(state.repo.fork(&leaf_id, ForkOptions { name: name.to_string() }))
    })?;

    Ok(CommandResult { message: format!("created branch '{name}' at {new_leaf}"), action: None })
}

fn switch(ctx: &CommandContext, state: &CommandState) -> anyhow::Result<CommandResult> {
    let target = ctx.args.trim();
    if target.is_empty() {
        return Ok(CommandResult { message: "usage: /switch <leaf-id>".to_string(), action: None });
    }

    let current = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(state.repo.get_metadata())
    })?
    .leaf_id;

    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(state.repo.navigate_tree(&current, target))
    })?;

    Ok(CommandResult { message: format!("switched to {target}"), action: None })
}

fn name_cmd(ctx: &CommandContext, state: &CommandState) -> anyhow::Result<CommandResult> {
    let title = ctx.args.trim();
    if title.is_empty() {
        return Ok(CommandResult { message: "usage: /name <title>".to_string(), action: None });
    }
    let entry = SessionEntry::SessionInfo(SessionInfoEntry {
        base: fresh_base(),
        title: title.to_string(),
        goal: String::new(),
    });
    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(state.repo.append_entry(entry))
    })?;
    Ok(CommandResult { message: format!("named session: {title}"), action: None })
}

fn changelog_cmd(_ctx: &CommandContext, state: &CommandState) -> anyhow::Result<CommandResult> {
    let path = current_path(&state.repo)?;
    let mut lines = Vec::new();
    for entry in &path {
        match entry {
            SessionEntry::Message(m) => lines.push(format!("[{}] {}", m.role, m.content)),
            SessionEntry::ModelChange(m) => lines.push(format!("[model] {}", m.model)),
            SessionEntry::Compaction(c) => {
                lines.push(format!("[compacted {}] {}", c.compacted_count, c.summary))
            }
            SessionEntry::Label(l) => lines.push(format!("[label] {}", l.label)),
            SessionEntry::SessionInfo(s) => lines.push(format!("[session] {}", s.title)),
            SessionEntry::BranchSummary(b) => {
                lines.push(format!("[branch {}] {}", b.branch, b.summary))
            }
            _ => {}
        }
    }
    Ok(CommandResult { message: lines.join("\n"), action: None })
}

fn export_cmd(_ctx: &CommandContext, state: &CommandState) -> anyhow::Result<CommandResult> {
    let path = current_path(&state.repo)?;
    let json = serde_json::to_string_pretty(&path)?;
    Ok(CommandResult { message: json, action: None })
}

// the ordered entry path from root to the current leaf.
fn current_path(repo: &Arc<JsonlSessionRepo>) -> anyhow::Result<Vec<SessionEntry>> {
    let current = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(repo.get_metadata())
    })?
    .leaf_id;
    let nav = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(repo.navigate_tree(&current, &current))
    })?;
    Ok(nav.path)
}

// a new entry base: generated id, no parent (append attaches to the tip).
fn fresh_base() -> EntryBase {
    EntryBase { id: uuid::Uuid::new_v4().to_string(), parent_id: None, timestamp: now_ms() }
}

fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

fn compact_cmd(_ctx: &CommandContext, state: &CommandState) -> anyhow::Result<CommandResult> {
    let leaf_id = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(state.repo.get_metadata())
    })?
    .leaf_id;

    let ctx = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(state.repo.build_context(&leaf_id))
    })?;

    match compact(&state.compaction, &leaf_id, &ctx) {
        Ok((_before, after, entry)) => {
            tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(state.repo.append_entry(entry))
            })?;
            Ok(CommandResult {
                message: format!(
                    "compacted {} older messages: {}",
                    after.compacted_count, after.summary
                ),
                action: None,
            })
        }
        Err(e) => Ok(CommandResult { message: format!("compaction skipped: {e}"), action: None }),
    }
}
