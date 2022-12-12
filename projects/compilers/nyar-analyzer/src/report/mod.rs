//! 前端无关的报告 / SSG / hydrate 岛契约（平台层）。

mod island;
mod literal;
mod spec;

pub use island::{IslandKind, awsl_island_kind};
pub use literal::series_to_init_literal;
pub use spec::{ColSeriesItem, HydratedChartSpec, StaticPageSpec};
