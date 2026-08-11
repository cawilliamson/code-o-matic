#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]

use code_o_matic::tools::{Tool, ToolError, ToolRegistry};
use code_o_matic::types::PermissionClass;

use serde_json::{json, Value};

/// a dummy tool used only for registry tests.
struct Dummy {
    name: &'static str,
}

#[async_trait::async_trait]
impl Tool for Dummy {
    fn name(&self) -> &'static str {
        self.name
    }

    fn schema(&self) -> Value {
        json!({"name": self.name})
    }

    async fn run(&self, _args: Value) -> Result<String, ToolError> {
        Ok(String::from("ok"))
    }

    fn permission_class(&self) -> PermissionClass {
        PermissionClass::Read
    }
}

#[tokio::test]
async fn register_and_dispatch_tool() {
    let mut registry = ToolRegistry::new();
    registry.register(Dummy { name: "dummy" });
    registry.set_active(vec![String::from("dummy")]).unwrap();
    // when: the registered tool is dispatched
    let result = registry.dispatch("dummy", json!({})).await.unwrap();
    // then: the tool result content is returned
    assert_eq!(result, "ok");
}

#[tokio::test]
async fn dispatch_unknown_tool_fails() {
    let registry = ToolRegistry::new();
    // when: an unregistered tool name is dispatched
    let err = registry.dispatch("missing", json!({})).await.unwrap_err();
    // then: the call returns an invalid-args error
    assert!(matches!(err, ToolError::InvalidArgs(_)));
}

#[tokio::test]
async fn active_schemas_only_return_active_tools() {
    let mut registry = ToolRegistry::new();
    registry.register(Dummy { name: "a" });
    registry.register(Dummy { name: "b" });
    registry.set_active(vec![String::from("a")]).unwrap();
    // when: active_schemas is queried
    let schemas = registry.active_schemas();
    // then: only the active tool's schema is returned
    assert_eq!(schemas.len(), 1);
    assert_eq!(schemas[0]["name"], "a");
}

#[tokio::test]
async fn duplicate_active_tool_is_rejected() {
    let mut registry = ToolRegistry::new();
    registry.register(Dummy { name: "a" });
    // when: set_active is called with a duplicated tool name
    let err = registry.set_active(vec![String::from("a"), String::from("a")]).unwrap_err();
    // then: the call returns an invalid-args error
    assert!(matches!(err, ToolError::InvalidArgs(_)));
}

