#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]

mod support;

use code_o_matic::registry::Registry;

use serde_json::json;
use support::{call, registered, tmp_config};

#[tokio::test]
async fn read_tool_reads_file_slice() {
    let (_dir, config) = tmp_config();
    std::fs::write(config.repo_root.join("sample.txt"), "line1\nline2\nline3\n").unwrap();
    let mut api = Registry::new();
    code_o_matic::register_builtins(&mut api, &config);
    // when: read is dispatched with an offset and limit
    let result = api
        .tools
        .dispatch_call(&call("read", json!({"path":"sample.txt","offset":2,"limit":1})))
        .await
        .expect("read ok");
    // then: the requested line is returned
    assert_eq!(result, "line2");
}

#[tokio::test]
async fn read_tool_missing_path_errors() {
    let (_dir, api) = registered();
    // when: read is dispatched with no path
    let err = api.tools.dispatch_call(&call("read", json!({}))).await.expect_err("should error");
    // then: an invalid-args error mentions path
    assert!(err.to_string().contains("path required"));
}

#[tokio::test]
async fn write_tool_writes_file() {
    let (_dir, config) = tmp_config();
    let mut api = Registry::new();
    code_o_matic::register_builtins(&mut api, &config);
    // when: write is dispatched with path and content
    let result = api
        .tools
        .dispatch_call(&call("write", json!({"path":"out.txt","content":"hello"})))
        .await
        .expect("write ok");
    let written = std::fs::read_to_string(config.repo_root.join("out.txt")).unwrap();
    // then: the file is written with a byte-count report
    assert_eq!(result, "wrote 5 bytes to out.txt");
    assert_eq!(written, "hello");
}

#[tokio::test]
async fn edit_tool_replaces_first_occurrence() {
    let (dir, config) = tmp_config();
    let _ = dir;
    std::fs::write(config.repo_root.join("f.txt"), "foo bar foo").unwrap();
    let mut api = Registry::new();
    code_o_matic::register_builtins(&mut api, &config);
    // when: edit is dispatched with a single-match old_string
    let result = api
        .tools
        .dispatch_call(&call("edit", json!({"path":"f.txt","old_string":"foo","new_string":"baz"})))
        .await
        .expect("edit ok");
    let written = std::fs::read_to_string(config.repo_root.join("f.txt")).unwrap();
    // then: the first occurrence is replaced
    assert!(result.contains("replaced"));
    assert_eq!(written, "baz bar foo");
}

#[tokio::test]
async fn edit_tool_old_string_not_found_errors() {
    let (_dir, config) = tmp_config();
    std::fs::write(config.repo_root.join("f.txt"), "hello").unwrap();
    let mut api = Registry::new();
    code_o_matic::register_builtins(&mut api, &config);
    // when: edit is dispatched with an absent old_string
    let err = api
        .tools
        .dispatch_call(&call("edit", json!({"path":"f.txt","old_string":"xyz","new_string":"abc"})))
        .await
        .expect_err("should error");
    // then: an error reports the missing old_string
    assert!(err.to_string().contains("old_string not found"));
}

#[tokio::test]
async fn bash_find_lists_files() {
    let (_dir, config) = tmp_config();
    std::fs::write(config.repo_root.join("a.txt"), "").unwrap();
    std::fs::create_dir_all(config.repo_root.join("sub")).unwrap();
    std::fs::write(config.repo_root.join("sub").join("c.txt"), "").unwrap();
    let mut api = Registry::new();
    code_o_matic::register_builtins(&mut api, &config);
    // when: bash lists files using find
    let result = api
        .tools
        .dispatch_call(&call("bash", json!({"command":"find . -type f"})))
        .await
        .expect("bash ok");
    let entries: Vec<&str> = result.split('\n').filter(|s| !s.is_empty()).collect();
    // then: files at any depth are listed
    assert!(entries.contains(&"./a.txt"));
    assert!(entries.contains(&"./sub/c.txt"));
}

#[tokio::test]
async fn bash_tool_runs_arbitrary_command() {
    let (_dir, api) = registered();
    // when: bash is dispatched with a chained, redirected, previously-blocked command
    let result = api
        .tools
        .dispatch_call(&call(
            "bash",
            json!({"command":"printf 'ok' > /tmp/com_scope_test; cat /tmp/com_scope_test"}),
        ))
        .await
        .expect("bash ok");
    // then: redirection and arbitrary commands run without guards
    assert_eq!(result.trim(), "ok");
}

#[tokio::test]
async fn grep_finds_matching_lines() {
    let (_dir, config) = tmp_config();
    std::fs::write(config.repo_root.join("a.rs"), "fn main() {}\nlet foo = 1;\n").unwrap();
    std::fs::write(config.repo_root.join("b.rs"), "no match here\n").unwrap();
    let mut api = Registry::new();
    code_o_matic::register_builtins(&mut api, &config);
    let result =
        api.tools.dispatch_call(&call("grep", json!({"pattern":"foo"}))).await.expect("grep ok");
    // then: the matching line is reported as path:lineno:text
    assert!(result.contains("a.rs:2:let foo = 1;"));
    assert!(!result.contains("b.rs"));
}

#[tokio::test]
async fn grep_honours_include_filter() {
    let (_dir, config) = tmp_config();
    std::fs::write(config.repo_root.join("a.rs"), "needle\n").unwrap();
    std::fs::write(config.repo_root.join("a.md"), "needle\n").unwrap();
    let mut api = Registry::new();
    code_o_matic::register_builtins(&mut api, &config);
    let result = api
        .tools
        .dispatch_call(&call("grep", json!({"pattern":"needle","include":".rs"})))
        .await
        .expect("grep ok");
    // then: only the .rs file appears
    assert!(result.contains("a.rs:1:needle"));
    assert!(!result.contains("a.md"));
}

#[tokio::test]
async fn grep_invalid_pattern_errors() {
    let (_dir, api) = registered();
    let err = api
        .tools
        .dispatch_call(&call("grep", json!({"pattern":"([unclosed"})))
        .await
        .expect_err("should error");
    // then: an invalid-args error is returned
    assert!(err.to_string().contains("invalid pattern"));
}

#[tokio::test]
async fn find_lists_matching_files() {
    let (_dir, config) = tmp_config();
    std::fs::write(config.repo_root.join("app.zig"), "").unwrap();
    std::fs::create_dir_all(config.repo_root.join("src")).unwrap();
    std::fs::write(config.repo_root.join("src").join("lib.zig"), "").unwrap();
    std::fs::write(config.repo_root.join("README.md"), "").unwrap();
    let mut api = Registry::new();
    code_o_matic::register_builtins(&mut api, &config);
    let result =
        api.tools.dispatch_call(&call("find", json!({"pattern":".zig"}))).await.expect("find ok");
    // then: zig files at any depth are listed
    assert!(result.contains("app.zig"));
    assert!(result.contains("src/lib.zig"));
    assert!(!result.contains("README.md"));
}

#[tokio::test]
async fn ls_lists_directory_entries() {
    let (_dir, config) = tmp_config();
    std::fs::write(config.repo_root.join("b.txt"), "").unwrap();
    std::fs::write(config.repo_root.join("a.txt"), "").unwrap();
    std::fs::create_dir_all(config.repo_root.join("sub")).unwrap();
    let mut api = Registry::new();
    code_o_matic::register_builtins(&mut api, &config);
    let result =
        api.tools.dispatch_call(&call("ls", json!({"directory":"."}))).await.expect("ls ok");
    // then: entries are sorted, with directories marked
    let lines: Vec<&str> = result.split('\n').collect();
    assert!(lines.contains(&"a.txt"));
    assert!(lines.contains(&"b.txt"));
    assert!(lines.contains(&"sub/"));
}

#[test]
fn read_schema_carries_usage_guidance() {
    let (_dir, api) = registered();
    let schema = api.tools.schemas().into_iter().find(|s| s["name"] == "read").expect("read tool");
    let desc = schema["description"].as_str().expect("description");
    // then: the schema teaches the model how to page through large files
    assert!(desc.contains("continue with offset until complete"));
    assert!(desc.contains("Use offset/limit for large files"));
}

#[test]
fn bash_schema_guides_instead_of_leaking_arbitrary_calls() {
    let (_dir, api) = registered();
    let schema = api.tools.schemas().into_iter().find(|s| s["name"] == "bash").expect("bash tool");
    let desc = schema["description"].as_str().expect("description");
    // then: bash is framed as the fallback, not the default for everything
    assert!(desc.contains("Use for operations no dedicated tool covers"));
}

#[tokio::test]
async fn edit_tool_applies_multiple_edits_in_one_call() {
    let (_dir, config) = tmp_config();
    std::fs::write(config.repo_root.join("f.txt"), "a = 1\nb = 2\n").unwrap();
    let mut api = Registry::new();
    code_o_matic::register_builtins(&mut api, &config);
    // when: edit is dispatched with an edits array covering two changes
    let result = api
        .tools
        .dispatch_call(&call(
            "edit",
            json!({
                "path": "f.txt",
                "edits": [
                    {"oldText": "a = 1", "newText": "a = 10"},
                    {"oldText": "b = 2", "newText": "b = 20"}
                ]
            }),
        ))
        .await
        .expect("edit ok");
    let content = std::fs::read_to_string(config.repo_root.join("f.txt")).unwrap();
    // then: both edits are applied in a single round trip
    assert_eq!(content, "a = 10\nb = 20\n");
    assert!(result.contains("applied 2 edits"));
}

#[tokio::test]
async fn edit_errors_when_oldtext_is_ambiguous() {
    let (_dir, config) = tmp_config();
    std::fs::write(config.repo_root.join("f.txt"), "foo bar foo").unwrap();
    let mut api = Registry::new();
    code_o_matic::register_builtins(&mut api, &config);
    // when: an edit targets text that appears twice
    let err = api
        .tools
        .dispatch_call(&call(
            "edit",
            json!({"path":"f.txt","edits":[{"oldText":"foo","newText":"baz"}]}),
        ))
        .await
        .expect_err("should error");
    // then: the error urges more context to disambiguate
    assert!(err.to_string().contains("disambiguate"));
    assert!(err.to_string().contains("appears 2 times"));
}

#[tokio::test]
async fn edit_single_form_still_works() {
    let (_dir, config) = tmp_config();
    std::fs::write(config.repo_root.join("f.txt"), "hello world").unwrap();
    let mut api = Registry::new();
    code_o_matic::register_builtins(&mut api, &config);
    let result = api
        .tools
        .dispatch_call(&call(
            "edit",
            json!({"path":"f.txt","old_string":"world","new_string":"there"}),
        ))
        .await
        .expect("edit ok");
    assert_eq!(std::fs::read_to_string(config.repo_root.join("f.txt")).unwrap(), "hello there");
    assert!(result.contains("replaced world -> there"));
}
