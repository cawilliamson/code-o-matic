//! view-spec computation for the tui context viewer.

mod context;

use std::sync::Arc;

use crate::registry::ViewBuilder;

/// register the context view against `api`.
pub fn register(api: &mut crate::registry::Registry) {
    api.views.register(
        "context".into(),
        ViewBuilder { title: "context viewer".into(), func: Arc::new(context::build) },
    );
}
