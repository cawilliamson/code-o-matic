#![allow(dead_code)]
//! shared test helpers for the builtin integration suite.

use code_o_matic::config::Config;
use code_o_matic::registry::Registry;
use code_o_matic::tools::ToolCall;
use serde_json::Value;

pub fn tmp_config() -> (tempfile::TempDir, Config) {
    let dir = tempfile::tempdir().expect("tempdir");
    let config = Config { repo_root: dir.path().to_path_buf(), ..Config::default() };
    (dir, config)
}

pub fn registered() -> (tempfile::TempDir, Registry) {
    let (_dir, config) = tmp_config();
    let mut api = Registry::new();
    code_o_matic::register_builtins(&mut api, &config);
    (_dir, api)
}

pub fn call(name: &str, args: Value) -> ToolCall {
    ToolCall { id: "t1".into(), name: name.into(), arguments: args }
}
