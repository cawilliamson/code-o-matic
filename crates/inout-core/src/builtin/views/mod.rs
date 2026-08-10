//! view-spec computation for the tui context and full viewers.

mod context;
mod full;

use std::sync::Arc;

use crate::extension::ViewBuilder;

/// register the context and full views against `api`.
pub fn register(api: &mut crate::extension::ExtensionApi) {
    api.views.register(
        "context".into(),
        ViewBuilder { title: "context viewer".into(), func: Arc::new(context::build) },
    );
    api.views.register(
        "full".into(),
        ViewBuilder {
            title: "full view — all llm traffic".into(),
            func: Arc::new(full::build),
        },
    );
}
