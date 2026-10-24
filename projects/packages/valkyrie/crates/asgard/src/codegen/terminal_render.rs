//! RenderIR → 终端盒模型布局（ASCII 字符单元格，用于 CLI / TUI 渲染）。

use serde_json::Value;

use crate::{
    awsl::{
        LoweredComponent, RenderAttr, RenderAttrValue, RenderIfNode, RenderLoopNode, RenderModule, RenderNode, RenderNodeId, RenderRegionId,
        RenderTextSegment, TemplateNodeKind,
        render_ir::{attr_value_source, region_nodes, text_segments_source},
    },
    codegen::expr_eval::ExprContext,
};

/// 终端单元格：一个字符位置的最小渲染单元。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalCell {
    /// 行坐标（0-indexed）。
    pub row: i32,
    /// 列坐标（0-indexed）。
    pub col: i32,
    /// 字符内容（单字符；空格用于清空）。
    pub ch: char,
    /// 前景色（ANSI 颜色码，0 = 默认）。
    pub fg: i32,
    /// 背景色（ANSI 颜色码，0 = 默认）。
    pub bg: i32,
}

/// 终端布局区域：节点在终端中的位置与尺寸。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalLayout {
    /// 起始行。
    pub row: i32,
    /// 起始列。
    pub col: i32,
    /// 宽度（列数）。
    pub width: i32,
    /// 高度（行数）。
    pub height: i32,
}

/// 可聚焦元素的视觉样式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusableKind {
    /// 按钮（带方括号 `[ label ]`）。
    Button,
    /// 列表项（带选择指示符 `>`，用于上下选择）。
    Item,
    /// 复选框（带 `[x]` / `[ ]`，按 Space 切换绑定）。
    Checkbox,
    /// 单选项（带 `(x)` / `( )`，按 Enter/Space 选中并写入绑定）。
    Radio,
}

impl FocusableKind {
    /// 编码到 focusables.bin 的标签字节。
    pub fn tag(self) -> u8 {
        match self {
            FocusableKind::Button => 0,
            FocusableKind::Item => 1,
            FocusableKind::Checkbox => 2,
            FocusableKind::Radio => 3,
        }
    }
}

/// 可聚焦按钮单元（登记事件绑定）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalFocusable {
    /// 按钮占据的区域。
    pub layout: TerminalLayout,
    /// 按钮显示文本。
    pub label: String,
    /// `@click` 事件对应的 export 名（如 "on_tap"）。
    pub event_name: String,
    /// 视觉样式（按钮或列表项）。
    pub kind: FocusableKind,
}

/// 终端渲染结果。
#[derive(Debug, Clone, Default)]
pub struct TerminalRenderResult {
    /// 所有需要绘制的单元格。
    pub cells: Vec<TerminalCell>,
    /// 所有可聚焦按钮（用于事件循环派发）。
    pub focusables: Vec<TerminalFocusable>,
}

/// 将已降级的组件 RenderIR 渲染为终端单元格集合。
///
/// 遍历 `component.render_ir`，维护当前光标位置与剩余可用宽度/高度，
/// 按 Column / Row / Box / Text / Button 等原语的盒模型语义布局。
pub fn render_terminal_ir(component: &LoweredComponent, data: &Value, rows: i32, cols: i32) -> TerminalRenderResult {
    let mut ctx = TerminalRenderCtx::new(component, data, rows, cols);
    let mut cur_row = 0i32;
    for &root_id in &component.render_ir.roots {
        let used = ctx.render_node(&component.render_ir, root_id, cur_row, 0, cols, rows - cur_row);
        cur_row += used;
    }
    TerminalRenderResult { cells: ctx.cells, focusables: ctx.focusables }
}

struct TerminalRenderCtx {
    expr: ExprContext,
    rows: i32,
    cols: i32,
    cells: Vec<TerminalCell>,
    focusables: Vec<TerminalFocusable>,
}

impl TerminalRenderCtx {
    fn new(component: &LoweredComponent, data: &Value, rows: i32, cols: i32) -> Self {
        Self { expr: ExprContext::from_bindings(&component.script_bindings, data), rows, cols, cells: Vec::new(), focusables: Vec::new() }
    }

    fn render_node(&mut self, module: &RenderModule, node_id: RenderNodeId, row: i32, col: i32, width: i32, height: i32) -> i32 {
        if width <= 0 || height <= 0 {
            return 0;
        }
        match module.node(node_id) {
            RenderNode::Element(element) => {
                self.render_tag(module, &element.tag, element.kind, &element.attrs, element.children, row, col, width, height)
            }
            RenderNode::Component(component) => self.render_tag(
                module,
                &component.tag,
                TemplateNodeKind::Component,
                &component.attrs,
                component.children,
                row,
                col,
                width,
                height,
            ),
            RenderNode::Text(text) => self.render_text(module, &text.segments, row, col, width),
            RenderNode::If(render_if) => {
                if self.expr.eval_truthy(module.expr_source(render_if.condition)) {
                    self.render_region(module, render_if.then_region, row, col, width, height)
                }
                else {
                    self.render_region(module, render_if.else_region, row, col, width, height)
                }
            }
            RenderNode::Loop(render_loop) => self.render_loop(module, render_loop, row, col, width, height),
            RenderNode::Fragment(fragment) => self.render_region(module, fragment.children, row, col, width, height),
        }
    }

    fn render_region(&mut self, module: &RenderModule, region: RenderRegionId, row: i32, col: i32, width: i32, height: i32) -> i32 {
        let mut cur_row = row;
        let mut remaining = height;
        for &node_id in region_nodes(module, region) {
            if remaining <= 0 {
                break;
            }
            let used = self.render_node(module, node_id, cur_row, col, width, remaining);
            cur_row += used;
            remaining -= used;
        }
        cur_row - row
    }

    fn render_tag(
        &mut self,
        module: &RenderModule,
        tag: &str,
        kind: TemplateNodeKind,
        attrs: &[RenderAttr],
        children: RenderRegionId,
        row: i32,
        col: i32,
        width: i32,
        height: i32,
    ) -> i32 {
        if tag.eq_ignore_ascii_case("column") {
            return self.render_region(module, children, row, col, width, height);
        }
        if tag.eq_ignore_ascii_case("row") {
            return self.render_row(module, children, row, col, width, height);
        }
        if tag.eq_ignore_ascii_case("box") {
            return self.render_box(module, children, row, col, width, height);
        }
        if tag.eq_ignore_ascii_case("text") {
            return self.render_region(module, children, row, col, width, height);
        }
        if tag.eq_ignore_ascii_case("button") {
            return self.render_button(module, attrs, children, row, col, width);
        }
        if tag.eq_ignore_ascii_case("list") {
            return self.render_region(module, children, row, col, width, height);
        }
        if tag.eq_ignore_ascii_case("item") {
            return self.render_item(module, attrs, children, row, col, width);
        }
        if tag.eq_ignore_ascii_case("flex") || tag.eq_ignore_ascii_case("slot") {
            return self.render_region(module, children, row, col, width, height);
        }
        match kind {
            TemplateNodeKind::Component | TemplateNodeKind::HostView => self.render_text_str(tag, row, col, width),
            TemplateNodeKind::Intrinsic => self.render_region(module, children, row, col, width, height),
        }
    }

    fn render_row(&mut self, module: &RenderModule, children: RenderRegionId, row: i32, col: i32, width: i32, height: i32) -> i32 {
        let child_ids = region_nodes(module, children);
        let count = child_ids.len() as i32;
        if count == 0 {
            return 0;
        }
        let child_width = width / count;
        let mut cur_col = col;
        let mut max_height = 0i32;
        for (index, &child_id) in child_ids.iter().enumerate() {
            let w = if index as i32 == count - 1 { width - child_width * (count - 1) } else { child_width };
            let used = self.render_node(module, child_id, row, cur_col, w, height);
            cur_col += w;
            if used > max_height {
                max_height = used;
            }
        }
        max_height
    }

    fn render_box(&mut self, module: &RenderModule, children: RenderRegionId, row: i32, col: i32, width: i32, height: i32) -> i32 {
        self.draw_box_border(row, col, width, height);
        if width >= 2 && height >= 2 {
            self.render_region(module, children, row + 1, col + 1, width - 2, height - 2);
        }
        height
    }

    fn draw_box_border(&mut self, row: i32, col: i32, width: i32, height: i32) {
        if width < 1 || height < 1 {
            return;
        }
        self.set_cell(row, col, '+');
        if width > 1 {
            self.set_cell(row, col + width - 1, '+');
        }
        if height > 1 {
            self.set_cell(row + height - 1, col, '+');
            if width > 1 {
                self.set_cell(row + height - 1, col + width - 1, '+');
            }
        }
        let last_col = if width > 1 { col + width - 1 } else { col };
        for c in (col + 1)..last_col {
            self.set_cell(row, c, '-');
            if height > 1 {
                self.set_cell(row + height - 1, c, '-');
            }
        }
        let last_row = if height > 1 { row + height - 1 } else { row };
        for r in (row + 1)..last_row {
            self.set_cell(r, col, '|');
            if width > 1 {
                self.set_cell(r, col + width - 1, '|');
            }
        }
    }

    fn render_text(&mut self, module: &RenderModule, segments: &[RenderTextSegment], row: i32, col: i32, width: i32) -> i32 {
        let text = self.eval_text_segments(module, segments);
        self.render_text_str(&text, row, col, width)
    }

    fn render_text_str(&mut self, text: &str, row: i32, col: i32, width: i32) -> i32 {
        let max_col = col + width;
        let mut cur_col = col;
        for ch in text.chars() {
            if cur_col >= max_col {
                break;
            }
            self.set_cell(row, cur_col, ch);
            cur_col += 1;
        }
        1
    }

    fn render_button(&mut self, module: &RenderModule, attrs: &[RenderAttr], children: RenderRegionId, row: i32, col: i32, width: i32) -> i32 {
        let label = self.extract_text(module, children);
        let display = format!("[ {} ]", label);
        let display_width = display.chars().count() as i32;
        self.render_text_str(&display, row, col, width);
        let event_name = self.extract_event_name(module, attrs);
        self.focusables.push(TerminalFocusable {
            layout: TerminalLayout { row, col, width: display_width, height: 1 },
            label,
            event_name,
            kind: FocusableKind::Button,
        });
        1
    }

    /// 渲染列表项：上下选择的基本单元。
    ///
    /// 列表项占一整行，显示文本为 `  label`（左侧留两格供 C 运行时绘制 `>` 选择指示符）。
    /// 注册为 [`FocusableKind::Item`]，参与 2D 方向导航与 Tab 线性步进。
    fn render_item(&mut self, module: &RenderModule, attrs: &[RenderAttr], children: RenderRegionId, row: i32, col: i32, width: i32) -> i32 {
        let label = self.extract_text(module, children);
        let display = format!("  {}", label);
        let display_width = display.chars().count() as i32;
        self.render_text_str(&display, row, col, width);
        let event_name = self.extract_event_name(module, attrs);
        self.focusables.push(TerminalFocusable {
            layout: TerminalLayout { row, col, width: display_width, height: 1 },
            label,
            event_name,
            kind: FocusableKind::Item,
        });
        1
    }

    fn render_loop(&mut self, module: &RenderModule, render_loop: &RenderLoopNode, row: i32, col: i32, width: i32, height: i32) -> i32 {
        let items = self.expr.eval_value(module.expr_source(render_loop.items));
        let Some(array) = items.as_array()
        else {
            return 0;
        };
        let mut cur_row = row;
        let mut remaining = height;
        for (index, item) in array.iter().enumerate() {
            if remaining <= 0 {
                break;
            }
            self.expr.push_loop(&render_loop.item_var, &render_loop.index_var, item.clone(), index);
            let used = self.render_region(module, render_loop.body_region, cur_row, col, width, remaining);
            self.expr.pop_loop();
            cur_row += used;
            remaining -= used;
        }
        cur_row - row
    }

    fn extract_text(&self, module: &RenderModule, region: RenderRegionId) -> String {
        let mut out = String::new();
        for &node_id in region_nodes(module, region) {
            match module.node(node_id) {
                RenderNode::Text(text) => out.push_str(&self.eval_text_segments(module, &text.segments)),
                RenderNode::Element(element) => out.push_str(&self.extract_text(module, element.children)),
                RenderNode::Component(component) => out.push_str(&self.extract_text(module, component.children)),
                RenderNode::Fragment(fragment) => out.push_str(&self.extract_text(module, fragment.children)),
                _ => {}
            }
        }
        out
    }

    fn eval_text_segments(&self, module: &RenderModule, segments: &[RenderTextSegment]) -> String {
        let mut out = String::new();
        for segment in segments {
            match segment {
                RenderTextSegment::Static(text) => out.push_str(text),
                RenderTextSegment::Expr(expr_id) => out.push_str(&self.expr.eval_string(module.expr_source(*expr_id))),
            }
        }
        out
    }

    fn extract_event_name(&self, module: &RenderModule, attrs: &[RenderAttr]) -> String {
        for attr in attrs {
            if attr.is_event && (attr.name == "@click" || attr.name == "on:click") {
                return attr_value_source(module, &attr.value);
            }
        }
        String::new()
    }

    fn set_cell(&mut self, row: i32, col: i32, ch: char) {
        if row < 0 || col < 0 || row >= self.rows || col >= self.cols {
            return;
        }
        self.cells.push(TerminalCell { row, col, ch, fg: 0, bg: 0 });
    }
}

/// focusables.bin 的魔数，用于 C 运行时校验。
pub const FOCUSABLES_MAGIC: &[u8] = b"ASGARDFX\x01";

/// 将 focusable 列表编码为 `focusables.bin` 二进制 sidecar。
///
/// 格式：`ASGARDFX\x01`(9B) + count(u32 LE) + 每个 focusable：
/// row(i32 LE) + col(i32 LE) + width(i32 LE) + height(i32 LE) + kind(u8)
/// + label_len(u32 LE) + label_utf8 + event_len(u32 LE) + event_utf8
pub fn encode_focusables(focusables: &[TerminalFocusable]) -> Vec<u8> {
    let mut out = Vec::with_capacity(64 + focusables.len() * 64);
    out.extend_from_slice(FOCUSABLES_MAGIC);
    out.extend_from_slice(&(focusables.len() as u32).to_le_bytes());
    for f in focusables {
        out.extend_from_slice(&f.layout.row.to_le_bytes());
        out.extend_from_slice(&f.layout.col.to_le_bytes());
        out.extend_from_slice(&f.layout.width.to_le_bytes());
        out.extend_from_slice(&f.layout.height.to_le_bytes());
        out.push(f.kind.tag());
        let label = f.label.as_bytes();
        out.extend_from_slice(&(label.len() as u32).to_le_bytes());
        out.extend_from_slice(label);
        let event = f.event_name.as_bytes();
        out.extend_from_slice(&(event.len() as u32).to_le_bytes());
        out.extend_from_slice(event);
    }
    out
}
#[cfg(test)]
#[path = "../../tests/path_wired/codegen_terminal_render.rs"]
mod codegen_terminal_render_tests;
