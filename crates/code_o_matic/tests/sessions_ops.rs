#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]

use code_o_matic::registry::{CommandContext, Registry};
use code_o_matic::sessions::commands::{register_session_commands, CommandState};
use code_o_matic::sessions::compaction::CompactionSettings;
use code_o_matic::sessions::entry::{EntryBase, MessageEntry, SessionEntry};
use code_o_matic::sessions::jsonl_repo::JsonlSessionRepo;
use code_o_matic::sessions::repo::SessionRepo;

use std::sync::Arc;

fn ctx(args: &str) -> CommandContext {
    CommandContext {
        model: "m".into(),
        system_prompt: String::new(),
        args: args.into(),
        snapshot: serde_json::json!({}),
    }
}

async fn seeded_repo(dir: &std::path::Path) -> Arc<JsonlSessionRepo> {
    let repo = JsonlSessionRepo::new(dir).await.unwrap();
    repo.append_entry(SessionEntry::Message(MessageEntry {
        base: EntryBase { id: uuid::Uuid::new_v4().to_string(), parent_id: None, timestamp: 1 },
        role: "user".into(),
        content: "refactor auth".into(),
    }))
    .await
    .unwrap();
    Arc::new(repo)
}

#[tokio::test(flavor = "multi_thread")]
async fn session_name_command_annotates_session() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = seeded_repo(tmp.path()).await;
    let mut api = Registry::new();
    register_session_commands(
        &mut api,
        CommandState { repo, compaction: CompactionSettings::default() },
    );

    let err = api.commands.dispatch("name", &ctx("")).unwrap();
    assert!(err.message.contains("usage"));

    let res = api.commands.dispatch("name", &ctx("my-session")).unwrap();
    assert!(res.message.contains("my-session"));
}

#[tokio::test(flavor = "multi_thread")]
async fn session_changelog_lists_entries() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = seeded_repo(tmp.path()).await;
    let mut api = Registry::new();
    register_session_commands(
        &mut api,
        CommandState { repo, compaction: CompactionSettings::default() },
    );

    let res = api.commands.dispatch("changelog", &ctx("")).unwrap();
    // then: the user message surfaces in the changelog
    assert!(res.message.contains("[user] refactor auth"));
}

#[tokio::test(flavor = "multi_thread")]
async fn session_export_serializes_path_as_json() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = seeded_repo(tmp.path()).await;
    let mut api = Registry::new();
    register_session_commands(
        &mut api,
        CommandState { repo, compaction: CompactionSettings::default() },
    );

    let res = api.commands.dispatch("export", &ctx("")).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&res.message).unwrap();
    // then: the export is a non-empty array of typed entries
    let arr = parsed.as_array().unwrap();
    assert!(!arr.is_empty());
    assert!(arr.iter().any(|e| e["type"] == "message"));
}
