#![allow(dead_code)]
//! shared test helpers for the builtin integration suite.

use inout_core::config::Config;
use inout_core::extension::ExtensionApi;
use inout_core::tools::ToolCall;
use serde_json::Value;

pub fn tmp_config() -> (tempfile::TempDir, Config) {
    let dir = tempfile::tempdir().expect("tempdir");
    let config = Config { repo_root: dir.path().to_path_buf(), ..Config::default() };
    (dir, config)
}

pub fn registered() -> (tempfile::TempDir, ExtensionApi) {
    let (_dir, config) = tmp_config();
    let mut api = ExtensionApi::noop();
    inout_core::register_builtins(&mut api, &config);
    (_dir, api)
}

pub fn call(name: &str, args: Value) -> ToolCall {
    ToolCall { id: "t1".into(), name: name.into(), arguments: args }
}
