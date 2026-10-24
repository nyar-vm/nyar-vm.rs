//! Source map 生成。

use serde::Serialize;
use std::path::Path;

/// 映射收集器（简化版：记录生成行与源偏移）。
#[derive(Debug, Default)]
pub struct MappingCollector {
    mappings: Vec<(u32, u32)>,
}

impl MappingCollector {
    /// 记录源文件字节偏移。
    pub fn note_span(&mut self, source_offset: usize) {
        let gen_line = self.mappings.len() as u32 + 1;
        self.mappings.push((gen_line, source_offset as u32));
    }

    /// 生成 source map JSON。
    pub fn to_json(&self, source_file: &str, generated_file: &str) -> String {
        let map = SourceMap {
            version: 3,
            file: Some(generated_file.to_string()),
            source_root: None,
            sources: vec![source_file.to_string()],
            names: vec![],
            mappings: encode_mappings(&self.mappings),
        };
        serde_json::to_string(&map).unwrap_or_else(|_| "{}".into())
    }
}

/// 写入 `.map` 旁路文件。
pub fn write_source_map(output_path: &Path, map_json: &str) -> std::io::Result<()> {
    let file_name = format!("{}.map", output_path.file_name().and_then(|name| name.to_str()).unwrap_or("out"));
    let map_path = output_path.with_file_name(file_name);
    std::fs::write(map_path, map_json)
}

#[derive(Serialize)]
struct SourceMap {
    version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_root: Option<String>,
    sources: Vec<String>,
    names: Vec<String>,
    mappings: String,
}

fn encode_mappings(mappings: &[(u32, u32)]) -> String {
    let mut out = String::new();
    let mut prev_gen_col = 0u32;
    let mut prev_src_col = 0u32;
    for (gen_line, src_col) in mappings {
        if !out.is_empty() {
            out.push(';');
        }
        let gen_col = 1u32;
        out.push_str(&encode_vlq(gen_col - prev_gen_col));
        out.push(',');
        out.push_str(&encode_vlq(0));
        out.push(',');
        out.push_str(&encode_vlq(0));
        out.push(',');
        out.push_str(&encode_vlq(src_col - prev_src_col));
        prev_gen_col = gen_col;
        prev_src_col = *src_col;
        let _ = gen_line;
    }
    out
}

fn encode_vlq(value: u32) -> String {
    let mut vlq = Vec::new();
    let mut value = value << 1;
    loop {
        let mut digit = (value & 0x1F) as u8;
        value >>= 5;
        if value > 0 {
            digit |= 0x20;
        }
        vlq.push(digit);
        if value == 0 {
            break;
        }
    }
    vlq.iter().map(|d| (d + 63) as char).collect()
}
