//! RenderIR → 移动端 UI 二进制包；Release 编入宿主制品，debug 可另出侧车。

use std::collections::HashMap;

use crate::awsl::{
    BindingKind, LoweredComponent, RenderAttr, RenderAttrValue, RenderIfNode, RenderIr, RenderLoopNode, RenderModule, RenderNode,
    RenderRegionId, RenderTextSegment, ScriptBinding, SignalValueType, TemplateNodeKind,
    render_ir::{
        RenderBinding, RenderBindingId, RenderComponentNode, RenderElement, RenderExpr, RenderExprId, RenderExprKind, RenderNodeId,
        RenderPurity, RenderRegion, RenderText, RenderValueKind,
    },
};

/// Asgard UI wire v1 魔数（8 字节 ASCII 标签 `ASGARDUI`，即 asgard ui）。
pub const UI_BIN_MAGIC: &[u8] = b"ASGARDUI";

/// 宿主 native 逻辑段魔数（8 字节 ASCII `ASGARDNT`，即 asgard native）。
pub const HOST_NATIVE_MAGIC: &[u8] = b"ASGARDNT";

/// 将 native AOT 逻辑与 Asgard UI 包编入**单一平台制品**尾段（先逻辑段，后 UI 段）。
pub fn build_integrated_host_product(native_logic: &[u8], ui_package: &[u8]) -> Vec<u8> {
    let mut product = Vec::new();
    embed_host_native_section(&mut product, native_logic);
    embed_asgard_ui_section(&mut product, ui_package);
    product
}

/// 在平台制品末尾附加 native AOT 逻辑段。
pub fn embed_host_native_section(container: &mut Vec<u8>, native: &[u8]) {
    crate::codegen::section_framing::embed_native_section(container, native);
}

/// Asgard UI wire 版本字节（接在 8 字节 `ASGARDUI` 魔数之后）。
///
/// 该字节是**容器标签**而非 schema 版本号：读侧按它区分承载区段，
/// 因此保留非零值；不保留「无版本字节」的旧负载兼容读取。
pub const UI_BIN_VERSION_V2: u8 = 0x02;

/// Decoded Asgard UI package (wire v0 baseline).
#[derive(Debug, Clone)]
pub struct DecodedUiPackage {
    /// 负载中的容器标签字节（必须等于 [`UI_BIN_VERSION_V2`]）。
    pub version: u8,
    /// Decoded components.
    pub components: Vec<DecodedComponent>,
}

/// One decoded component entry.
#[derive(Debug, Clone)]
pub struct DecodedComponent {
    /// Route name.
    pub route_name: String,
    /// Widget name.
    pub name: String,
    /// Component ABI (empty for wire v1).
    pub abi: oak_awsl::ComponentAbi,
    /// Script bindings.
    pub bindings: Vec<ScriptBinding>,
    /// Render IR module.
    pub render_ir: RenderIr,
}

/// Wire decode failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    /// Truncated or malformed blob.
    InvalidFormat(String),
}

/// Decode an Asgard UI binary package：魔数后必须紧跟容器标签字节与 ABI 段。
pub fn decode_mobile_ui_package(bytes: &[u8]) -> Result<DecodedUiPackage, DecodeError> {
    if bytes.len() < UI_BIN_MAGIC.len() + 4 {
        return Err(DecodeError::InvalidFormat("buffer too short".into()));
    }
    if !bytes.starts_with(UI_BIN_MAGIC) {
        return Err(DecodeError::InvalidFormat("bad magic".into()));
    }
    let mut offset = UI_BIN_MAGIC.len();
    if bytes.get(offset) != Some(&UI_BIN_VERSION_V2) {
        return Err(DecodeError::InvalidFormat("missing UI bin container tag".into()));
    }
    offset += 1;
    let version = UI_BIN_VERSION_V2;
    let component_count = read_u32(bytes, &mut offset)? as usize;
    let mut components = Vec::with_capacity(component_count);
    for _ in 0..component_count {
        let route_name = read_string(bytes, &mut offset)?;
        let name = read_string(bytes, &mut offset)?;
        let abi = read_abi(bytes, &mut offset)?;
        let bindings = read_bindings(bytes, &mut offset)?;
        let render_ir = read_ir(bytes, &mut offset)?;
        components.push(DecodedComponent { route_name, name, abi, bindings, render_ir });
    }
    Ok(DecodedUiPackage { version, components })
}

fn read_u32(bytes: &[u8], offset: &mut usize) -> Result<u32, DecodeError> {
    if *offset + 4 > bytes.len() {
        return Err(DecodeError::InvalidFormat("unexpected EOF reading u32".into()));
    }
    let value = u32::from_le_bytes(bytes[*offset..*offset + 4].try_into().unwrap());
    *offset += 4;
    Ok(value)
}

fn read_string(bytes: &[u8], offset: &mut usize) -> Result<String, DecodeError> {
    let len = read_u32(bytes, offset)? as usize;
    if *offset + len > bytes.len() {
        return Err(DecodeError::InvalidFormat("unexpected EOF reading string".into()));
    }
    let value = std::str::from_utf8(&bytes[*offset..*offset + len]).map_err(|e| DecodeError::InvalidFormat(e.to_string()))?.to_string();
    *offset += len;
    Ok(value)
}

fn read_abi(bytes: &[u8], offset: &mut usize) -> Result<oak_awsl::ComponentAbi, DecodeError> {
    let mut abi = oak_awsl::ComponentAbi::default();
    let prop_count = read_u32(bytes, offset)? as usize;
    for _ in 0..prop_count {
        let name = read_string(bytes, offset)?;
        let _type_tag = bytes.get(*offset).copied().ok_or_else(|| DecodeError::InvalidFormat("EOF type tag".into()))?;
        *offset += 1;
        let flags = bytes.get(*offset).copied().ok_or_else(|| DecodeError::InvalidFormat("EOF flags".into()))?;
        *offset += 1;
        let required = flags & 1 != 0;
        let has_default = flags & 2 != 0;
        let default_expr = if has_default { Some(read_string(bytes, offset)?) } else { None };
        abi.properties.push(oak_awsl::AbiProperty { name, type_hint: None, required, default_expr, span: 0..0 });
    }
    let event_count = read_u32(bytes, offset)? as usize;
    for _ in 0..event_count {
        let name = read_string(bytes, offset)?;
        let param_count = read_u32(bytes, offset)? as usize;
        let mut params = Vec::with_capacity(param_count);
        for _ in 0..param_count {
            let param_name = read_string(bytes, offset)?;
            let _type_tag = bytes.get(*offset).copied().ok_or_else(|| DecodeError::InvalidFormat("EOF param tag".into()))?;
            *offset += 1;
            params.push(oak_awsl::AbiParam { name: param_name, type_hint: None });
        }
        abi.events.push(oak_awsl::AbiEvent { name, params, span: 0..0 });
    }
    Ok(abi)
}

fn read_bindings(bytes: &[u8], offset: &mut usize) -> Result<Vec<ScriptBinding>, DecodeError> {
    let count = read_u32(bytes, offset)? as usize;
    let mut bindings = Vec::with_capacity(count);
    for _ in 0..count {
        let name = read_string(bytes, offset)?;
        let init_expr = read_string(bytes, offset)?;
        let reactive = bytes.get(*offset).copied().ok_or_else(|| DecodeError::InvalidFormat("EOF reactive".into()))? != 0;
        *offset += 1;
        let value_type = match bytes.get(*offset).copied().ok_or_else(|| DecodeError::InvalidFormat("EOF value type".into()))? {
            1 => SignalValueType::I32,
            2 => SignalValueType::Utf8,
            3 => SignalValueType::Bool,
            _ => SignalValueType::Utf8,
        };
        *offset += 1;
        let kind = if reactive { BindingKind::ReactiveState } else { BindingKind::LocalConst };
        bindings.push(ScriptBinding { name, init_expr, kind, reactive, sig_var: String::new(), value_type });
    }
    Ok(bindings)
}

struct WireIrDecoder {
    module: RenderModule,
    expr_intern: HashMap<String, RenderExprId>,
}

impl WireIrDecoder {
    fn new() -> Self {
        Self { module: RenderModule::empty(), expr_intern: HashMap::new() }
    }

    fn finish(mut self, bytes: &[u8], offset: &mut usize) -> Result<RenderModule, DecodeError> {
        self.module.roots = self.read_region(bytes, offset)?;
        Ok(self.module)
    }

    fn read_region(&mut self, bytes: &[u8], offset: &mut usize) -> Result<Vec<RenderNodeId>, DecodeError> {
        let count = read_u32(bytes, offset)? as usize;
        let mut nodes = Vec::with_capacity(count);
        for _ in 0..count {
            nodes.push(self.read_node(bytes, offset)?);
        }
        Ok(nodes)
    }

    fn read_node(&mut self, bytes: &[u8], offset: &mut usize) -> Result<RenderNodeId, DecodeError> {
        let id = RenderNodeId(self.module.nodes.len() as u32);
        let kind = bytes.get(*offset).copied().ok_or_else(|| DecodeError::InvalidFormat("EOF node kind".into()))?;
        *offset += 1;
        match kind {
            1 => {
                let tag = read_string(bytes, offset)?;
                let node_kind = match bytes.get(*offset).copied().ok_or_else(|| DecodeError::InvalidFormat("EOF kind tag".into()))? {
                    1 => TemplateNodeKind::Component,
                    2 => TemplateNodeKind::Intrinsic,
                    _ => TemplateNodeKind::HostView,
                };
                *offset += 1;
                let attrs = read_attrs(self, bytes, offset)?;
                let child_nodes = self.read_region(bytes, offset)?;
                let children = self.alloc_region(child_nodes);
                let node = if node_kind == TemplateNodeKind::Component {
                    RenderNode::Component(crate::awsl::render_ir::RenderComponentNode { tag, attrs, children, span: 0..0 })
                }
                else {
                    RenderNode::Element(crate::awsl::render_ir::RenderElement { tag, kind: node_kind, attrs, children, span: 0..0 })
                };
                self.module.nodes.push(node);
            }
            2 => {
                let segments = read_text_segments(self, bytes, offset)?;
                self.module.nodes.push(RenderNode::Text(crate::awsl::render_ir::RenderText { segments, span: 0..0 }));
            }
            3 => {
                let condition = self.intern_expr(read_string(bytes, offset)?);
                let then_nodes = self.read_region(bytes, offset)?;
                let else_nodes = self.read_region(bytes, offset)?;
                let then_region = self.alloc_region(then_nodes);
                let else_region = self.alloc_region(else_nodes);
                self.module.nodes.push(RenderNode::If(RenderIfNode { condition, then_region, else_region, span: 0..0 }));
            }
            4 => {
                let items = self.intern_expr(read_string(bytes, offset)?);
                let item_var = read_string(bytes, offset)?;
                let item_binding = self.ensure_binding(&item_var);
                let body_nodes = self.read_region(bytes, offset)?;
                let body_region = self.alloc_region(body_nodes);
                self.module.nodes.push(RenderNode::Loop(RenderLoopNode {
                    items,
                    key: None,
                    item_binding,
                    index_binding: None,
                    item_var,
                    index_var: String::new(),
                    body_region,
                    span: 0..0,
                }));
            }
            other => return Err(DecodeError::InvalidFormat(format!("unknown node kind {other}"))),
        }
        Ok(id)
    }

    fn alloc_region(&mut self, nodes: Vec<RenderNodeId>) -> RenderRegionId {
        if nodes.is_empty() {
            return RenderRegionId::EMPTY;
        }
        let id = RenderRegionId(self.module.regions.len() as u32);
        self.module.regions.push(RenderRegion { nodes });
        id
    }

    fn intern_expr(&mut self, source: String) -> RenderExprId {
        let trimmed = source.trim().to_string();
        if let Some(id) = self.expr_intern.get(&trimmed).copied() {
            return id;
        }
        let id = RenderExprId(self.module.exprs.len() as u32);
        self.module.exprs.push(RenderExpr {
            kind: RenderExprKind::Template,
            source: trimmed.clone(),
            binding_refs: Vec::new(),
            value_kind: RenderValueKind::Unknown,
            purity: RenderPurity::Unknown,
            memoizable: false,
            event_call: None,
        });
        self.expr_intern.insert(trimmed, id);
        id
    }

    fn ensure_binding(&mut self, name: &str) -> RenderBindingId {
        let id = RenderBindingId(self.module.bindings.len() as u32);
        let expr_id = self.intern_expr(name.to_string());
        self.module.bindings.push(RenderBinding { name: name.to_string(), init_expr: expr_id, reactive: false, sig_var: String::new() });
        id
    }
}

fn read_ir(bytes: &[u8], offset: &mut usize) -> Result<RenderIr, DecodeError> {
    WireIrDecoder::new().finish(bytes, offset)
}

fn read_attrs(decoder: &mut WireIrDecoder, bytes: &[u8], offset: &mut usize) -> Result<Vec<RenderAttr>, DecodeError> {
    let count = read_u32(bytes, offset)? as usize;
    let mut attrs = Vec::with_capacity(count);
    for _ in 0..count {
        let name = read_string(bytes, offset)?;
        let is_event = bytes.get(*offset).copied().ok_or_else(|| DecodeError::InvalidFormat("EOF is_event".into()))? != 0;
        *offset += 1;
        let is_prop = bytes.get(*offset).copied().ok_or_else(|| DecodeError::InvalidFormat("EOF is_prop".into()))? != 0;
        *offset += 1;
        let value = read_attr_value(decoder, bytes, offset)?;
        attrs.push(RenderAttr { name, value, is_event, is_prop, event_id: None });
    }
    Ok(attrs)
}

fn read_attr_value(decoder: &mut WireIrDecoder, bytes: &[u8], offset: &mut usize) -> Result<RenderAttrValue, DecodeError> {
    let tag = bytes.get(*offset).copied().ok_or_else(|| DecodeError::InvalidFormat("EOF attr value".into()))?;
    *offset += 1;
    match tag {
        1 => Ok(RenderAttrValue::Static(read_string(bytes, offset)?)),
        2 => Ok(RenderAttrValue::Expr(decoder.intern_expr(read_string(bytes, offset)?))),
        3 => Ok(RenderAttrValue::Template(read_text_segments(decoder, bytes, offset)?)),
        other => Err(DecodeError::InvalidFormat(format!("unknown attr value tag {other}"))),
    }
}

fn read_text_segments(decoder: &mut WireIrDecoder, bytes: &[u8], offset: &mut usize) -> Result<Vec<RenderTextSegment>, DecodeError> {
    let count = read_u32(bytes, offset)? as usize;
    let mut segments = Vec::with_capacity(count);
    for _ in 0..count {
        let tag = bytes.get(*offset).copied().ok_or_else(|| DecodeError::InvalidFormat("EOF text part".into()))?;
        *offset += 1;
        match tag {
            1 => segments.push(RenderTextSegment::Static(read_string(bytes, offset)?)),
            2 => segments.push(RenderTextSegment::Expr(decoder.intern_expr(read_string(bytes, offset)?))),
            other => return Err(DecodeError::InvalidFormat(format!("unknown text part {other}"))),
        }
    }
    Ok(segments)
}

/// 将 AWSL 组件列表编码为 Asgard UI 二进制包（wire v2，含 ABI 段）。
pub fn encode_mobile_ui_package(components: &[LoweredComponent]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(UI_BIN_MAGIC);
    out.push(UI_BIN_VERSION_V2);
    out.extend_from_slice(&(components.len() as u32).to_le_bytes());
    for component in components {
        write_string(&mut out, &component.route_name);
        write_string(&mut out, &component.name);
        write_abi(&mut out, &component.component_abi);
        write_bindings(&mut out, &component.script_bindings);
        write_ir(&mut out, &component.render_ir);
    }
    out
}

fn write_abi(out: &mut Vec<u8>, abi: &oak_awsl::ComponentAbi) {
    out.extend_from_slice(&(abi.properties.len() as u32).to_le_bytes());
    for property in &abi.properties {
        write_string(out, &property.name);
        out.push(type_hint_tag(property.type_hint.as_deref()));
        let mut flags = 0u8;
        if property.required {
            flags |= 1;
        }
        if property.default_expr.is_some() {
            flags |= 2;
        }
        out.push(flags);
        if let Some(default) = &property.default_expr {
            write_string(out, default);
        }
    }
    out.extend_from_slice(&(abi.events.len() as u32).to_le_bytes());
    for event in &abi.events {
        write_string(out, &event.name);
        out.extend_from_slice(&(event.params.len() as u32).to_le_bytes());
        for param in &event.params {
            write_string(out, &param.name);
            out.push(type_hint_tag(param.type_hint.as_deref()));
        }
    }
}

fn type_hint_tag(type_hint: Option<&str>) -> u8 {
    match type_hint {
        Some(hint) if hint.contains("bool") => 3,
        Some(hint) if hint.contains("i32") || hint.contains("int") => 1,
        Some(hint) if hint.contains("utf8") || hint.contains("string") => 2,
        _ => 0,
    }
}

/// 把 UI 段嵌入 Android 容器（DEX/ELF）尾部。
pub fn embed_asgard_ui_section(container: &mut Vec<u8>, ir: &[u8]) {
    crate::codegen::section_framing::embed_ui_section(container, ir);
}

/// 从容器中查找并返回 UI 段。
pub fn find_asgard_ui_section(container: &[u8]) -> Option<&[u8]> {
    crate::codegen::section_framing::find_ui_section(container)
}

/// 从容器中查找并返回 native 段。
pub fn find_asgard_native_section(container: &[u8]) -> Option<&[u8]> {
    crate::codegen::section_framing::find_native_section(container)
}

/// 构造内嵌 UI 段的测试用 DEX 容器。
#[cfg(test)]
pub fn dex_with_embedded_ui(components: &[LoweredComponent]) -> Vec<u8> {
    let mut dex_builder = acorn_dex::dex::DexImageBuilder::new();
    dex_builder.add_class("asgard/Placeholder", &[0xCA, 0xFE, 0xBA, 0xBE]);
    let mut dex = dex_builder.build().expect("DEX 构建失败");
    embed_asgard_ui_section(&mut dex, &encode_mobile_ui_package(components));
    dex
}

/// 构造内嵌 UI 段的测试用宿主可执行容器。
#[cfg(test)]
pub fn host_executable_with_embedded_ui(platform: &str, components: &[LoweredComponent]) -> Vec<u8> {
    let builder = acorn_mach_o::mach_o::MachOImageBuilder::new();
    let _ = platform;
    let mut out = builder.build_executable().expect("Mach-O 构建失败");
    embed_asgard_ui_section(&mut out, &encode_mobile_ui_package(components));
    out
}

fn write_bindings(out: &mut Vec<u8>, bindings: &[ScriptBinding]) {
    out.extend_from_slice(&(bindings.len() as u32).to_le_bytes());
    for binding in bindings {
        write_string(out, &binding.name);
        write_string(out, binding.init_expr.trim());
        out.push(u8::from(binding.reactive));
        out.push(signal_type_tag(binding.value_type));
    }
}

fn signal_type_tag(ty: SignalValueType) -> u8 {
    match ty {
        SignalValueType::I32 => 1,
        SignalValueType::Utf8 => 2,
        SignalValueType::Bool => 3,
    }
}

fn write_ir(out: &mut Vec<u8>, module: &RenderModule) {
    write_region(out, module, &module.roots);
}

fn write_region(out: &mut Vec<u8>, module: &RenderModule, node_ids: &[RenderNodeId]) {
    let flat: Vec<RenderNodeId> = node_ids
        .iter()
        .flat_map(|&node_id| match module.node(node_id) {
            RenderNode::Fragment(fragment) => module.region(fragment.children).nodes.clone(),
            _ => vec![node_id],
        })
        .collect();
    out.extend_from_slice(&(flat.len() as u32).to_le_bytes());
    for node_id in flat {
        write_node(out, module, node_id);
    }
}

fn write_node(out: &mut Vec<u8>, module: &RenderModule, node_id: RenderNodeId) {
    match module.node(node_id) {
        RenderNode::Element(element) => {
            out.push(1);
            write_string(out, &element.tag);
            out.push(kind_tag(element.kind));
            write_attrs(out, module, &element.attrs);
            write_region(out, module, module.region(element.children).nodes.as_slice());
        }
        RenderNode::Component(component) => {
            out.push(1);
            write_string(out, &component.tag);
            out.push(kind_tag(TemplateNodeKind::Component));
            write_attrs(out, module, &component.attrs);
            write_region(out, module, module.region(component.children).nodes.as_slice());
        }
        RenderNode::Text(text) => {
            out.push(2);
            write_text_segments(out, module, &text.segments);
        }
        RenderNode::If(render_if) => {
            out.push(3);
            write_string(out, module.expr_source(render_if.condition).trim());
            write_region(out, module, module.region(render_if.then_region).nodes.as_slice());
            write_region(out, module, module.region(render_if.else_region).nodes.as_slice());
        }
        RenderNode::Loop(render_loop) => {
            out.push(4);
            write_string(out, module.expr_source(render_loop.items).trim());
            write_string(out, &render_loop.item_var);
            write_region(out, module, module.region(render_loop.body_region).nodes.as_slice());
        }
        RenderNode::Fragment { .. } => {}
    }
}

fn kind_tag(kind: TemplateNodeKind) -> u8 {
    match kind {
        TemplateNodeKind::Component => 1,
        TemplateNodeKind::Intrinsic => 2,
        TemplateNodeKind::HostView => 3,
    }
}

fn write_attrs(out: &mut Vec<u8>, module: &RenderModule, attrs: &[RenderAttr]) {
    out.extend_from_slice(&(attrs.len() as u32).to_le_bytes());
    for attr in attrs {
        write_string(out, &attr.name);
        out.push(u8::from(attr.is_event));
        out.push(u8::from(attr.is_prop));
        match &attr.value {
            RenderAttrValue::Static(s) => {
                out.push(1);
                write_string(out, s);
            }
            RenderAttrValue::Expr(expr_id) => {
                out.push(2);
                write_string(out, module.expr_source(*expr_id).trim());
            }
            RenderAttrValue::Template(segments) => {
                out.push(3);
                write_text_segments(out, module, segments);
            }
        }
    }
}

fn write_text_segments(out: &mut Vec<u8>, module: &RenderModule, segments: &[RenderTextSegment]) {
    out.extend_from_slice(&(segments.len() as u32).to_le_bytes());
    for segment in segments {
        match segment {
            RenderTextSegment::Static(text) => {
                out.push(1);
                write_string(out, text);
            }
            RenderTextSegment::Expr(expr_id) => {
                out.push(2);
                write_string(out, module.expr_source(*expr_id).trim());
            }
        }
    }
}

fn write_string(out: &mut Vec<u8>, value: &str) {
    let bytes = value.as_bytes();
    out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(bytes);
}

#[cfg(test)]
#[doc(hidden)]
pub fn host_logic_placeholder(platform: &str) -> Vec<u8> {
    let mut out = Vec::from(b"ASGDHOST\x01");
    write_string(&mut out, platform);
    out
}

#[cfg(test)]
#[doc(hidden)]
pub fn dex_placeholder() -> Vec<u8> {
    let mut dex = vec![0u8; 112];
    dex[..8].copy_from_slice(b"dex\n035\0");
    dex
}
#[cfg(test)]
#[path = "../../tests/path_wired/codegen_mobile_ui_binary.rs"]
mod codegen_mobile_ui_binary_tests;
