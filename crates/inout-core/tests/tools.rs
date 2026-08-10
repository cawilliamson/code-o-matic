#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]

mod support;

use inout_core::extension::ExtensionApi;
use inout_testing::{scenario, then, when};
use serde_json::json;
use support::{call, registered, tmp_config};

#[tokio::test]
async fn read_tool_reads_file_slice() {
    let mut s = scenario!("builtins", "builtin tools", "read returns a line slice");
    let (_dir, config) = tmp_config();
    std::fs::write(config.repo_root.join("sample.txt"), "line1\nline2\nline3\n").unwrap();
    let mut api = ExtensionApi::noop();
    inout_core::register_builtins(&mut api, &config);
    when!(s, "read is dispatched with an offset and limit", {
        let result =
            api.tools.dispatch_call(&call("read", json!({"path":"sample.txt","offset":2,"limit":1})))
                .await
                .expect("read ok");
        then!(s, "the requested line is returned", {
            assert_eq!(result, "line2");
        });
    });
}

#[tokio::test]
async fn read_tool_missing_path_errors() {
    let mut s = scenario!("builtins", "builtin tools", "read requires path");
    let (_dir, api) = registered();
    when!(s, "read is dispatched with no path", {
        let err = api.tools.dispatch_call(&call("read", json!({}))).await.expect_err("should error");
        then!(s, "an invalid-args error mentions path", {
            assert!(err.to_string().contains("path required"));
        });
    });
}

#[tokio::test]
async fn write_tool_writes_file() {
    let mut s = scenario!("builtins", "builtin tools", "write creates a file");
    let (_dir, config) = tmp_config();
    let mut api = ExtensionApi::noop();
    inout_core::register_builtins(&mut api, &config);
    when!(s, "write is dispatched with path and content", {
        let result =
            api.tools.dispatch_call(&call("write", json!({"path":"out.txt","content":"hello"})))
                .await
                .expect("write ok");
        let written = std::fs::read_to_string(config.repo_root.join("out.txt")).unwrap();
        then!(s, "the file is written with a byte-count report", {
            assert_eq!(result, "wrote 5 bytes to out.txt");
            assert_eq!(written, "hello");
        });
    });
}

#[tokio::test]
async fn edit_tool_replaces_first_occurrence() {
    let mut s = scenario!("builtins", "builtin tools", "edit replaces first match");
    let (dir, config) = tmp_config();
    let _ = dir;
    std::fs::write(config.repo_root.join("f.txt"), "foo bar foo").unwrap();
    let mut api = ExtensionApi::noop();
    inout_core::register_builtins(&mut api, &config);
    when!(s, "edit is dispatched with a single-match old_string", {
        let result = api
            .tools
            .dispatch_call(&call("edit", json!({"path":"f.txt","old_string":"foo","new_string":"baz"})))
            .await
            .expect("edit ok");
        let written = std::fs::read_to_string(config.repo_root.join("f.txt")).unwrap();
        then!(s, "the first occurrence is replaced", {
            assert!(result.contains("replaced"));
            assert_eq!(written, "baz bar foo");
        });
    });
}

#[tokio::test]
async fn edit_tool_old_string_not_found_errors() {
    let mut s = scenario!("builtins", "builtin tools", "edit reports missing old_string");
    let (_dir, config) = tmp_config();
    std::fs::write(config.repo_root.join("f.txt"), "hello").unwrap();
    let mut api = ExtensionApi::noop();
    inout_core::register_builtins(&mut api, &config);
    when!(s, "edit is dispatched with an absent old_string", {
        let err = api
            .tools
            .dispatch_call(&call("edit", json!({"path":"f.txt","old_string":"xyz","new_string":"abc"})))
            .await
            .expect_err("should error");
        then!(s, "an error reports the missing old_string", {
            assert!(err.to_string().contains("old_string not found"));
        });
    });
}

#[tokio::test]
async fn grep_tool_filters_matching_lines() {
    let mut s = scenario!("builtins", "builtin tools", "grep returns matching lines");
    let (_dir, config) = tmp_config();
    std::fs::write(config.repo_root.join("f.txt"), "apple\nbanana\napricot\n").unwrap();
    let mut api = ExtensionApi::noop();
    inout_core::register_builtins(&mut api, &config);
    when!(s, "grep is dispatched with a case-sensitive pattern", {
        let result =
            api.tools.dispatch_call(&call("grep", json!({"path":"f.txt","pattern":"ap"})))
                .await
                .expect("grep ok");
        then!(s, "matching lines are returned", {
            assert_eq!(result, "apple\napricot");
        });
    });
}

#[tokio::test]
async fn glob_tool_lists_matching_files() {
    let mut s = scenario!("builtins", "builtin tools", "glob lists matching entries");
    let (_dir, config) = tmp_config();
    std::fs::write(config.repo_root.join("a.txt"), "").unwrap();
    std::fs::write(config.repo_root.join("b.rs"), "").unwrap();
    std::fs::create_dir_all(config.repo_root.join("sub")).unwrap();
    std::fs::write(config.repo_root.join("sub").join("c.txt"), "").unwrap();
    let mut api = ExtensionApi::noop();
    inout_core::register_builtins(&mut api, &config);
    when!(s, "glob is dispatched with a txt pattern", {
        let result =
            api.tools.dispatch_call(&call("glob", json!({"path":"","pattern":"*.txt"})))
                .await
                .expect("glob ok");
        let entries: Vec<&str> = result.split('\n').filter(|s| !s.is_empty()).collect();
        then!(s, "txt files at any depth are listed", {
            assert!(entries.contains(&"a.txt"));
            assert!(entries.contains(&"sub/c.txt"));
            assert!(!entries.contains(&"b.rs"));
        });
    });
}

#[tokio::test]
async fn bash_tool_blocked_binary_rejected() {
    let mut s = scenario!("builtins", "builtin tools", "bash rejects a blocked binary");
    let (_dir, api) = registered();
    when!(s, "bash is dispatched with a blocked binary", {
        let err =
            api.tools.dispatch_call(&call("bash", json!({"command":"rm -f x"}))).await.expect_err("should block");
        then!(s, "the block message names the binary", {
            assert!(err.to_string().contains("rm is blocked"));
        });
    });
}

#[tokio::test]
async fn bash_tool_runs_allowed_command() {
    let mut s = scenario!("builtins", "builtin tools", "bash runs an allowed command");
    let (_dir, api) = registered();
    when!(s, "bash is dispatched with an allowlisted command", {
        let result =
            api.tools.dispatch_call(&call("bash", json!({"command":"echo hello"}))).await.expect("bash ok");
        then!(s, "the command output is returned", {
            assert_eq!(result.trim(), "hello");
        });
    });
}
