//! `asgard.config.v` 必须使用 `define_config(asgard) { ... }`（Valkyrie script）。

use miette::Result;

/// 将 `define_config(asgard)` 块规范化为内部配置对象文本（仅供解析管线使用）。
pub fn normalize_config_source(source: &str) -> Result<String> {
    let trimmed = source.trim();
    if trimmed.is_empty() {
        return Err(miette::miette!("asgard.config.v 为空"));
    }
    if trimmed.starts_with('{') {
        return Err(miette::miette!("asgard.config.v 必须使用 define_config(asgard) {{ ... }} 写法，不支持 {{ key: value }} 对象字面量简写"));
    }
    if !trimmed.starts_with("define_config(asgard)") && !trimmed.starts_with("define_config (asgard)") {
        return Err(miette::miette!("asgard.config.v 必须以 define_config(asgard) {{ ... }} 开头"));
    }
    let start = trimmed.find('{').ok_or_else(|| miette::miette!("define_config 缺少 '{{'"))?;
    let object_text = extract_balanced_block(&trimmed[start..])?;
    reject_von_colon_field_syntax(&object_text)?;
    let colonized = assignment_object_to_colon_object(&object_text)?;
    let nested = insert_nested_block_colons(&colonized);
    Ok(strip_trailing_commas(&insert_line_commas(&nested)))
}

/// 拒绝 `define_config` 块内 `key: value` 式 VON 字段写法（应使用 `key = value`）。
fn reject_von_colon_field_syntax(block: &str) -> Result<()> {
    let mut in_string = None::<char>;
    let mut escaped = false;
    for line in block.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("//") {
            continue;
        }
        let mut index = 0usize;
        let chars: Vec<char> = line.chars().collect();
        while index < chars.len() {
            let ch = chars[index];
            if let Some(quote) = in_string {
                if escaped {
                    escaped = false;
                }
                else if ch == '\\' {
                    escaped = true;
                }
                else if ch == quote {
                    in_string = None;
                }
                index += 1;
                continue;
            }
            if ch == '"' || ch == '\'' {
                in_string = Some(ch);
                index += 1;
                continue;
            }
            if ch.is_ascii_alphabetic() || ch == '_' {
                let start = index;
                while index < chars.len() && (chars[index].is_ascii_alphanumeric() || chars[index] == '_') {
                    index += 1;
                }
                let mut look = index;
                while look < chars.len() && chars[look].is_whitespace() {
                    look += 1;
                }
                if look < chars.len() && chars[look] == ':' {
                    let ident: String = chars[start..index].iter().collect();
                    if ident != "http" && ident != "https" {
                        return Err(miette::miette!(
                            "asgard.config.v 请使用 Valkyrie script 赋值（{ident} = ...），不要使用 VON 冒号语法（{ident}: ...）"
                        ));
                    }
                }
                continue;
            }
            index += 1;
        }
    }
    Ok(())
}

fn strip_trailing_commas(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut in_string = None::<char>;
    let mut escaped = false;
    let chars: Vec<char> = source.chars().collect();
    let mut index = 0usize;
    while index < chars.len() {
        let ch = chars[index];
        if let Some(quote) = in_string {
            out.push(ch);
            if escaped {
                escaped = false;
            }
            else if ch == '\\' {
                escaped = true;
            }
            else if ch == quote {
                in_string = None;
            }
            index += 1;
            continue;
        }
        if ch == '"' || ch == '\'' {
            in_string = Some(ch);
            out.push(ch);
            index += 1;
            continue;
        }
        if ch == ',' {
            let mut look = index + 1;
            while look < chars.len() && chars[look].is_whitespace() {
                look += 1;
            }
            if look < chars.len() && matches!(chars[look], '}' | ']') {
                index += 1;
                continue;
            }
        }
        out.push(ch);
        index += 1;
    }
    out
}

fn insert_line_commas(source: &str) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let mut out = Vec::with_capacity(lines.len());
    for (index, line) in lines.iter().enumerate() {
        let mut current = (*line).to_string();
        let trimmed = line.trim();
        if trimmed.is_empty() {
            out.push(current);
            continue;
        }
        if trimmed.ends_with(',') {
            out.push(current);
            continue;
        }
        let Some(next) = lines.get(index + 1)
        else {
            out.push(current);
            continue;
        };
        let next_trim = next.trim();
        if next_trim.is_empty() {
            out.push(current);
            continue;
        }
        let next_starts_field = next_trim.chars().next().is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_');
        let should_comma = if trimmed == "}" || trimmed == "]" {
            next_starts_field
        }
        else if trimmed.ends_with('{') || trimmed.ends_with('[') {
            false
        }
        else if trimmed.contains(':') {
            next_starts_field || next_trim.starts_with('}')
        }
        else {
            false
        };
        if should_comma {
            current.push(',');
        }
        out.push(current);
    }
    out.join("\n")
}

fn insert_nested_block_colons(source: &str) -> String {
    source
        .lines()
        .map(|line| {
            let trimmed = line.trim();
            if !trimmed.ends_with('{') || trimmed.contains(':') {
                return line.to_string();
            }
            let head = trimmed.trim_end_matches('{').trim();
            if head.is_empty() || !head.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_') {
                return line.to_string();
            }
            let indent = line.chars().take_while(|ch| ch.is_whitespace()).collect::<String>();
            format!("{indent}{head}: {{")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn extract_balanced_block(source: &str) -> Result<String> {
    let mut depth = 0i32;
    let mut in_string = None::<char>;
    let mut escaped = false;
    for (index, ch) in source.char_indices() {
        if let Some(quote) = in_string {
            if escaped {
                escaped = false;
                continue;
            }
            if ch == '\\' {
                escaped = true;
                continue;
            }
            if ch == quote {
                in_string = None;
            }
            continue;
        }
        if ch == '"' || ch == '\'' {
            in_string = Some(ch);
            continue;
        }
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Ok(source[..=index].to_string());
                }
            }
            _ => {}
        }
    }
    Err(miette::miette!("define_config 块未闭合"))
}

fn assignment_object_to_colon_object(source: &str) -> Result<String> {
    let mut out = String::with_capacity(source.len());
    let mut in_string = None::<char>;
    let mut escaped = false;
    let chars: Vec<char> = source.chars().collect();
    let mut index = 0usize;
    while index < chars.len() {
        let ch = chars[index];
        if let Some(quote) = in_string {
            out.push(ch);
            if escaped {
                escaped = false;
            }
            else if ch == '\\' {
                escaped = true;
            }
            else if ch == quote {
                in_string = None;
            }
            index += 1;
            continue;
        }
        if ch == '"' || ch == '\'' {
            in_string = Some(ch);
            out.push(ch);
            index += 1;
            continue;
        }
        if ch == '=' {
            out.push(':');
            index += 1;
            continue;
        }
        if ch == ';' {
            out.push(',');
            index += 1;
            continue;
        }
        out.push(ch);
        index += 1;
    }
    Ok(out)
}
#[cfg(test)]
#[path = "../tests/path_wired/config_script.rs"]
mod config_script_tests;
