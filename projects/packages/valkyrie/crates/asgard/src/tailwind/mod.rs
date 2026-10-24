//! Tailwind utility 收集与可选构建。

pub mod style_collector;

pub use style_collector::{
    StyleCollector, collect_from_components, collect_push_calls_from_script, style_collector_push, write_content_manifest,
};
