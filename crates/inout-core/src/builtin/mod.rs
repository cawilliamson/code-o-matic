//! first-party built-in tools, views and slash commands.
//!
//! these replace the old runtime-loaded `.rhai` scripts: same behaviour, all
//! native rust and therefore compilable in or out via feature flags. the
//! registries each subsystem registers into are unchanged, so the agent loop
//! and tui are unaffected.

mod bash;
mod common;
mod edit;
mod glob;
mod grep;
mod read;
mod views;
mod write;

use std::sync::Arc;

use crate::config::Config;
use crate::extension::{
    Command, CommandAction, CommandContext, CommandHandler, CommandResult,
};

/// register every first-party builtin against `api`.
///
/// `config` supplies the repo root (via a fresh jail) and the bash policy.
pub fn register_builtins(api: &mut crate::extension::ExtensionApi, config: &Config) {
    read::register(api, config);
    write::register(api, config);
    edit::register(api, config);
    grep::register(api, config);
    glob::register(api, config);
    bash::register(api, config);
    views::register(api);

    let core_commands: [(&str, &str, CommandHandler); 9] = [
        ("help", "list available commands", native_command(|_| {
            CommandResult {
                message: "commands: /help /clear /new /model /undo /exit /reload /context /full"
                    .into(),
                action: None,
            }
        })),
        ("clear", "clear conversation history", native_command(|_| {
            CommandResult {
                message: "history cleared".into(),
                action: Some(CommandAction::ClearHistory),
            }
        })),
        ("new", "start a new conversation", native_command(|_| {
            CommandResult {
                message: "new conversation started".into(),
                action: Some(CommandAction::ClearHistory),
            }
        })),
        ("model", "switch model: /model <name>", native_command(|ctx| {
            let args = ctx.args.trim();
            if args.is_empty() {
                CommandResult { message: "usage: /model <name>".into(), action: None }
            } else {
                CommandResult {
                    message: format!("switching model to {args}"),
                    action: Some(CommandAction::SetModel(args.to_string())),
                }
            }
        })),
        ("undo", "drop the last turn", native_command(|_| {
            CommandResult {
                message: "last turn dropped".into(),
                action: Some(CommandAction::UndoLastTurn),
            }
        })),
        ("exit", "exit the agent", native_command(|_| {
            CommandResult { message: "bye".into(), action: Some(CommandAction::Exit) }
        })),
        ("reload", "reload extensions", native_command(|_| {
            CommandResult {
                message: "reloading extensions…".into(),
                action: Some(CommandAction::ReloadExtensions),
            }
        })),
        ("context", "open context viewer", native_command(|_| {
            CommandResult {
                message: "opening context view…".into(),
                action: Some(CommandAction::OpenView("context".into())),
            }
        })),
        ("full", "show full llm traffic inline", native_command(|_| {
            CommandResult {
                message: "rendering full view inline…".into(),
                action: Some(CommandAction::OpenView("full".into())),
            }
        })),
    ];
    for (name, description, handler) in core_commands {
        api.commands.register(Command {
            name: name.to_string(),
            description: description.to_string(),
            handler,
        });
    }
}

/// wrap a plain `Box<dyn Fn>` body as a `CommandHandler::Rust`.
fn native_command(
    f: impl Fn(&CommandContext) -> CommandResult + Send + Sync + 'static,
) -> CommandHandler {
    CommandHandler::Rust(Arc::new(move |ctx| Ok(f(ctx))))
}
