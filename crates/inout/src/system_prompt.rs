//! system prompt built from the registered tool schemas.

use inout_core::tools::ToolRegistry;

/// build the default system prompt describing the available tools.
pub(crate) fn default_system_prompt(tools: &ToolRegistry, repo_root: &std::path::Path) -> String {
    let tool_docs: Vec<String> = tools
        .schemas()
        .iter()
        .filter_map(|s| {
            let name = s.get("name").and_then(|n| n.as_str())?;
            let desc = s.get("description").and_then(|d| d.as_str()).unwrap_or("(no description)");
            let required: Vec<String> = s
                .get("required")
                .and_then(|r| r.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default();
            let props: Vec<String> = s
                .get("properties")
                .and_then(|p| p.as_object())
                .map(|obj| {
                    obj.iter()
                        .map(|(k, v)| {
                            let ty = v.get("type").and_then(|t| t.as_str()).unwrap_or("any");
                            let pdesc = v.get("description").and_then(|d| d.as_str()).unwrap_or("");
                            let req = if required.contains(k) { " (required)" } else { "" };
                            format!("    {k}: {ty}{req} — {pdesc}")
                        })
                        .collect()
                })
                .unwrap_or_default();
            let props_str = props.join("\n");
            Some(format!("  {name}: {desc}\n{props_str}"))
        })
        .collect();
    let tool_docs_str = tool_docs.join("\n\n");
    format!(
        "you are InOut Agent (io), a minimal rust-native coding agent. you operate inside the \
         repo at {repo_root}. all file access is jailed to the repo root. be terse and direct.\n\n\
         available tools:\n{tool_docs_str}",
        tool_docs_str = tool_docs_str,
        repo_root = repo_root.display(),
    )
}
