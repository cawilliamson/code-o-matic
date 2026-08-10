//! tui view builder types and registry.

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;

/// a native tui view builder.
///
/// the builder receives a conversation snapshot json and returns a typed
/// `ViewSpec`. scripts used to own this computation; now it is native rust
/// while rendering and keyboard interaction stay in the tui.
#[derive(Clone)]
pub struct ViewBuilder {
    /// human-readable view title shown in the tui header.
    pub title: String,
    /// native builder: snapshot -> view spec.
    pub func: ViewBuilderFn,
}

/// type alias for a native view builder body, kept small for clippy.
type ViewBuilderFn = Arc<dyn Fn(&serde_json::Value) -> anyhow::Result<ViewSpec> + Send + Sync>;

impl std::fmt::Debug for ViewBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ViewBuilder").field("title", &self.title).finish_non_exhaustive()
    }
}

/// a typed view-spec block, produced by converting the builder's return into
/// a form the tui can render without holding scripting types.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum ViewBlock {
    /// user-authored text.
    UserText {
        /// text content.
        text: String,
        /// estimated token count.
        tokens: usize,
    },
    /// assistant-authored text.
    AssistantText {
        /// text content.
        text: String,
        /// estimated token count.
        tokens: usize,
    },
    /// a tool call the model requested.
    ToolCall {
        /// tool name.
        name: String,
        /// pretty-printed json input.
        input_json: String,
        /// estimated token count.
        tokens: usize,
    },
    /// a tool result returned to the model.
    ToolResult {
        /// tool name.
        tool_name: String,
        /// result content.
        content: String,
        /// estimated token count.
        tokens: usize,
    },
}

/// one turn in the viewer: a slice of the conversation plus its rendered blocks.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ViewTurn {
    /// index into the agent's message list where this turn starts.
    pub msg_index: usize,
    /// number of messages this turn spans (user + assistant + tool results).
    pub msg_count: usize,
    /// first ~60 chars of the user message, shown in the turn list.
    pub preview: String,
    /// estimated token cost for this turn.
    pub tokens_est: usize,
    /// whether this turn is within the active sliding window.
    pub in_window: bool,
    /// ordered content blocks for the detail pane.
    pub blocks: Vec<ViewBlock>,
}

/// a fully-resolved view spec ready for the tui to render.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct ViewSpec {
    /// turn entries in conversation order.
    pub turns: Vec<ViewTurn>,
    /// total estimated tokens across all messages.
    pub total_tokens: usize,
    /// configured context limit in tokens.
    pub limit_tokens: usize,
    /// context fill percentage (0–100).
    pub context_pct: u8,
}

/// registry of named tui views built by the binary.
#[derive(Clone, Default)]
pub struct ViewRegistry {
    views: HashMap<String, ViewBuilder>,
}

impl std::fmt::Debug for ViewRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ViewRegistry")
            .field("views", &self.views.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl ViewRegistry {
    /// create an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// register a view builder under `name`. last-registered wins on conflict.
    pub fn register(&mut self, name: String, builder: ViewBuilder) {
        self.views.insert(name, builder);
    }

    /// look up a view builder by name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&ViewBuilder> {
        self.views.get(name)
    }

    /// registered view names, sorted alphabetically.
    #[must_use]
    pub fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.views.keys().cloned().collect();
        names.sort();
        names
    }
}

/// invoke a registered view's builder with a conversation snapshot.
pub fn build_view(builder: &ViewBuilder, snapshot: &Value) -> anyhow::Result<ViewSpec> {
    (builder.func)(snapshot)
}
