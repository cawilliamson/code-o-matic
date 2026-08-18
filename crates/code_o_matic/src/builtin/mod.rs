//! first-party built-in tools, views and slash commands.
//!
//! these replace the old runtime-loaded `.rhai` scripts: same behaviour, all
//! native rust and therefore compilable in or out via feature flags. the
//! registries each subsystem registers into are unchanged, so the agent loop
//! and tui are unaffected.

mod bash;
mod common;
mod edit;
mod inspect;
mod read;
mod views;
mod write;

use std::sync::Arc;

use crate::config::Config;
use crate::registry::{Command, CommandAction, CommandContext, CommandHandler, CommandResult};

/// register every first-party builtin against `api`.
///
/// `config` supplies the repo root (the working base) and the bash policy.
pub fn register_builtins(api: &mut crate::registry::Registry, config: &Config) {
    read::register(api, config);
    write::register(api, config);
    edit::register(api, config);
    bash::register(api, config);
    inspect::register(api, config);
    views::register(api);

    let core_commands: [(&str, &str, CommandHandler); 8] = [
        (
            "help",
            "list available commands",
            native_command(|_| CommandResult {
                message: "commands: /help /clear /new /model /undo /quit /context /reasoning"
                    .into(),
                action: None,
            }),
        ),
        (
            "clear",
            "clear conversation history",
            native_command(|_| CommandResult {
                message: "history cleared".into(),
                action: Some(CommandAction::ClearHistory),
            }),
        ),
        (
            "new",
            "start a new conversation",
            native_command(|_| CommandResult {
                message: "new conversation started".into(),
                action: Some(CommandAction::ClearHistory),
            }),
        ),
        (
            "model",
            "list or switch model: /model [name]",
            native_command(|ctx| {
                let args = ctx.args.trim();
                if args.is_empty() {
                    if ctx.available_models.is_empty() {
                        return CommandResult {
                            message: format!(
                                "{} — no models discovered (check the endpoint or set COM_MODEL)",
                                ctx.model
                            ),
                            action: None,
                        };
                    }
                    CommandResult {
                        message: "select a model".into(),
                        action: Some(CommandAction::OpenModelPicker),
                    }
                } else {
                    CommandResult {
                        message: format!("switching model to {args}"),
                        action: Some(CommandAction::SetModel(args.to_string())),
                    }
                }
            }),
        ),
        (
            "undo",
            "drop the last turn",
            native_command(|_| CommandResult {
                message: "last turn dropped".into(),
                action: Some(CommandAction::UndoLastTurn),
            }),
        ),
        (
            "quit",
            "exit the agent",
            native_command(|_| CommandResult {
                message: "bye".into(),
                action: Some(CommandAction::Exit),
            }),
        ),
        (
            "context",
            "open context viewer",
            native_command(|_| CommandResult {
                message: "opening context view…".into(),
                action: Some(CommandAction::OpenView("context".into())),
            }),
        ),
        (
            "reasoning",
            "toggle reasoning display: /reasoning [on|off]",
            native_command(|ctx| {
                let label = if ctx.reasoning { "on" } else { "off" };
                let target = match ctx.args.trim() {
                    "" => Some(!ctx.reasoning),
                    "on" => Some(true),
                    "off" => Some(false),
                    _ => None,
                };
                match target {
                    Some(v) => CommandResult {
                        message: format!("reasoning {}", if v { "on" } else { "off" }),
                        action: Some(CommandAction::SetReasoning(v)),
                    },
                    None => CommandResult {
                        message: format!(
                            "reasoning is currently {label} — usage: /reasoning [on|off]"
                        ),
                        action: None,
                    },
                }
            }),
        ),
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
