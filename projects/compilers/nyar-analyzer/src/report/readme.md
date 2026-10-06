# report

Frontend-neutral report / SSG / hydrate island contracts (same layering model as `format` and `highlight`).

## Layers

| Layer | Location | Role |
|:---|:---|:---|
| Platform | `nyar-analyzer::report` | `IslandKind`, `ColSeriesItem`, `HydratedChartSpec`, `series_to_init_literal` |
| Implementation | `voa` | AWSL static render, WASM hydrate bundles, `asgard.plotter` inlining |
| Tool + skin | `legion` CLI + `*.report` AWSL projects | Emit facts and series data; pages and styling stay in AWSL |

## Island routing

| Path heuristic | `IslandKind` |
|:---|:---|
| `charts/**` | `Hydrated` (WASM + glue) |
| `pages/*-report.awsl`, `layout.awsl` | `Static` (SSG) |
| other | `Hydrated` (default) |

## Data contracts

- **`ColSeriesItem`** — bar series item (`key`, `label`, `value_text`, `fill`, `height_pct`); fields match `InteractiveColPlot` consumers.
- **`HydratedChartSpec`** — hydrate chart island from tools → voa (`route`, `title`, `series`).

Business aggregation (test pass counts, bench ordering, coverage truncation, …) stays in each tool's codegen, not in this module.

## Presentation helper

`series_to_init_literal` serializes a `ColSeriesItem` list into an AWSL `<script>`-injectable list literal (no business logic).
