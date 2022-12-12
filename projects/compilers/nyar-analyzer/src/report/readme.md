# report

前端无关的报告 / SSG / hydrate 岛契约层（对齐 `format` / `highlight` 分层）。

## 层级

| 层 | 位置 | 职责 |
|:---|:---|:---|
| Platform | `nyar-analyzer::report` | `IslandKind` / `ColSeriesItem` / `HydratedChartSpec` / `series_to_init_literal` |
| Implementation | `voa` | AWSL 静态渲染、WASM hydrate 打包、`asgard.plotter` 内联 |
| Tool + skin | `legion` 等 CLI + `*.report` AWSL 工程 | 产出事实与 series 数据；页面/样式由 AWSL 自定义 |

## 岛分流

| 路径启发式 | `IslandKind` |
|:---|:---|
| `charts/**` | `Hydrated`（WASM + glue） |
| `pages/*-report.awsl`、`layout.awsl` | `Static`（SSG） |
| 其他 | `Hydrated`（默认） |

## 数据契约

- **`ColSeriesItem`**：柱图系列项（`key` / `label` / `value_text` / `fill` / `height_pct`），与 `InteractiveColPlot` 消费字段一致。
- **`HydratedChartSpec`**：工具 → voa 的 hydrate 图表岛（`route` + `title` + `series`）。

业务聚合（测试通过数、bench 排序、coverage 截断等）留在各工具 codegen，不在本模块实现。

## 呈现辅助

`series_to_init_literal` 将 `ColSeriesItem` 列表序列化为 AWSL `<script>` 可注入的 list 字面量（不含业务逻辑）。
