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
        .dispatch_call(&call("bash", json!({"command":"printf 'ok' > /tmp/com_scope_test; cat /tmp/com_scope_test"})))
        .await
        .expect("bash ok");
    // then: redirection and arbitrary commands run without guards
    assert_eq!(result.trim(), "ok");
}
