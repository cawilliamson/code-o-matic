//! compiled first-party extensions loaded by the binary.

use inout_core::Extension;

pub(crate) fn compiled_extensions() -> Vec<Box<dyn Extension>> {
    vec![
        Box::new(inout_ext_skills::SkillsExtension),
        Box::new(inout_ext_sessions::SessionsExtension),
    ]
}
