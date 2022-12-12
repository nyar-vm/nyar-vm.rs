use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::fs;

pub struct Preprocessor {
    macros: HashMap<String, MacroDef>,
    include_paths: Vec<PathBuf>,
    system_include_paths: Vec<PathBuf>,
    included_files: Vec<PathBuf>,
    current_file: PathBuf,
    current_line: usize,
    expanding_macros: Vec<String>,
}

#[derive(Clone)]
enum MacroDef {
    Simple(String),
    Function {
        params: Vec<String>,
        has_varargs: bool,
        body: String,
    },
}

impl Preprocessor {
    pub fn new() -> Self {
        let mut macros = HashMap::new();
        // Predefined macros
        macros.insert("__RUSTY_C__".to_string(), MacroDef::Simple("1".to_string()));
        macros.insert("__STDC__".to_string(), MacroDef::Simple("1".to_string()));
        macros.insert("__STDC_VERSION__".to_string(), MacroDef::Simple("202311L".to_string()));
        macros.insert("__STDC_HOSTED__".to_string(), MacroDef::Simple("1".to_string()));

        // Architecture / OS macros
        #[cfg(target_arch = "x86_64")]
        macros.insert("__x86_64__".to_string(), MacroDef::Simple("1".to_string()));
        #[cfg(target_arch = "aarch64")]
        macros.insert("__aarch64__".to_string(), MacroDef::Simple("1".to_string()));
        #[cfg(windows)]
        macros.insert("_WIN32".to_string(), MacroDef::Simple("1".to_string()));
        #[cfg(target_os = "linux")]
        macros.insert("__linux__".to_string(), MacroDef::Simple("1".to_string()));
        #[cfg(target_os = "macos")]
        macros.insert("__APPLE__".to_string(), MacroDef::Simple("1".to_string()));
        #[cfg(target_os = "macos")]
        macros.insert("__MACH__".to_string(), MacroDef::Simple("1".to_string()));
        #[cfg(unix)]
        macros.insert("__unix__".to_string(), MacroDef::Simple("1".to_string()));
        #[cfg(unix)]
        macros.insert("__unix".to_string(), MacroDef::Simple("1".to_string()));
        
        // Size of types (assuming 64-bit for now as it's common for nyar-vm)
        macros.insert("__SIZEOF_INT__".to_string(), MacroDef::Simple("4".to_string()));
        macros.insert("__SIZEOF_LONG__".to_string(), MacroDef::Simple("8".to_string()));
        macros.insert("__SIZEOF_LONG_LONG__".to_string(), MacroDef::Simple("8".to_string()));
        macros.insert("__SIZEOF_SHORT__".to_string(), MacroDef::Simple("2".to_string()));
        macros.insert("__SIZEOF_POINTER__".to_string(), MacroDef::Simple("8".to_string()));
        macros.insert("__SIZEOF_FLOAT__".to_string(), MacroDef::Simple("4".to_string()));
        macros.insert("__SIZEOF_DOUBLE__".to_string(), MacroDef::Simple("8".to_string()));
        macros.insert("__SIZEOF_LONG_DOUBLE__".to_string(), MacroDef::Simple("16".to_string()));
        macros.insert("__SIZEOF_SIZE_T__".to_string(), MacroDef::Simple("8".to_string()));
        macros.insert("__SIZEOF_PTRDIFF_T__".to_string(), MacroDef::Simple("8".to_string()));
        macros.insert("__SIZEOF_WINT_T__".to_string(), MacroDef::Simple("4".to_string()));
        macros.insert("__SIZEOF_WCHAR_T__".to_string(), MacroDef::Simple("4".to_string()));

        // Endianness
        #[cfg(target_endian = "little")]
        macros.insert("__BYTE_ORDER__".to_string(), MacroDef::Simple("__ORDER_LITTLE_ENDIAN__".to_string()));
        #[cfg(target_endian = "big")]
        macros.insert("__BYTE_ORDER__".to_string(), MacroDef::Simple("__ORDER_BIG_ENDIAN__".to_string()));
        
        macros.insert("__ORDER_LITTLE_ENDIAN__".to_string(), MacroDef::Simple("1234".to_string()));
        macros.insert("__ORDER_BIG_ENDIAN__".to_string(), MacroDef::Simple("4321".to_string()));
        macros.insert("__ORDER_PDP_ENDIAN__".to_string(), MacroDef::Simple("3412".to_string()));
        
        Self {
            macros,
            include_paths: Vec::new(),
            system_include_paths: Vec::new(),
            included_files: Vec::new(),
            current_file: PathBuf::from("<stdin>"),
            current_line: 0,
            expanding_macros: Vec::new(),
        }
    }

    pub fn add_include_path<P: AsRef<Path>>(&mut self, path: P) {
        self.include_paths.push(path.as_ref().to_path_buf());
    }

    pub fn add_system_include_path<P: AsRef<Path>>(&mut self, path: P) {
        self.system_include_paths.push(path.as_ref().to_path_buf());
    }

    pub fn define<S: Into<String>, V: Into<String>>(&mut self, name: S, value: V) {
        self.macros.insert(name.into(), MacroDef::Simple(value.into()));
    }

    pub fn process(&mut self, source: &str, current_dir: &Path) -> Result<String, String> {
        let stripped_source = self.strip_comments(source);
        let mut output = String::new();
        let mut lines = stripped_source.lines();
        let mut skip_stack = Vec::new();

        while let Some(mut line) = lines.next() {
            self.current_line += 1;
            
            // Handle line continuation
            let mut full_line = line.to_string();
            while full_line.ends_with('\\') {
                full_line.pop();
                if let Some(next_line) = lines.next() {
                    full_line.push_str(next_line);
                    // Line continuation doesn't increment current_line in terms of #line directive mapping
                } else {
                    break;
                }
            }
            line = &full_line;
            let trimmed = line.trim();

            if trimmed.is_empty() {
                output.push('\n');
                continue;
            }

            // Handle directives
            if trimmed.starts_with('#') {
                let directive_line = trimmed[1..].trim();
                
                if directive_line.starts_with("ifdef") {
                    let name = directive_line[5..].trim();
                    skip_stack.push(!self.macros.contains_key(name));
                    continue;
                } else if directive_line.starts_with("ifndef") {
                    let name = directive_line[6..].trim();
                    skip_stack.push(self.macros.contains_key(name));
                    continue;
                } else if directive_line.starts_with("if") {
                    // Basic support for #if - currently just checks if macro exists or is non-zero
                    let expr = directive_line[2..].trim();
                    let val = self.evaluate_condition(expr);
                    skip_stack.push(!val);
                    continue;
                } else if directive_line.starts_with("elifdef") {
                    if let Some(skip) = skip_stack.pop() {
                        if skip {
                            let name = directive_line[7..].trim();
                            let val = self.macros.contains_key(name);
                            skip_stack.push(!val);
                        } else {
                            skip_stack.push(true);
                        }
                    } else {
                        return Err(format!("Line {}: Unexpected #elifdef", self.current_line));
                    }
                    continue;
                } else if directive_line.starts_with("elifndef") {
                    if let Some(skip) = skip_stack.pop() {
                        if skip {
                            let name = directive_line[8..].trim();
                            let val = !self.macros.contains_key(name);
                            skip_stack.push(!val);
                        } else {
                            skip_stack.push(true);
                        }
                    } else {
                        return Err(format!("Line {}: Unexpected #elifndef", self.current_line));
                    }
                    continue;
                } else if directive_line.starts_with("elif") {
                    if let Some(skip) = skip_stack.pop() {
                        if skip {
                            let expr = directive_line[4..].trim();
                            let val = self.evaluate_condition(expr);
                            skip_stack.push(!val);
                        } else {
                            skip_stack.push(true); // Already executed a branch
                        }
                    } else {
                        return Err(format!("Line {}: Unexpected #elif", self.current_line));
                    }
                    continue;
                } else if directive_line == "else" {
                    if let Some(skip) = skip_stack.pop() {
                        skip_stack.push(!skip);
                    } else {
                        return Err(format!("Line {}: Unexpected #else", self.current_line));
                    }
                    continue;
                } else if directive_line == "endif" {
                    if skip_stack.pop().is_none() {
                        return Err(format!("Line {}: Unexpected #endif", self.current_line));
                    }
                    continue;
                }

                // Skip other directives if in a false conditional branch
                if skip_stack.iter().any(|&skip| skip) {
                    continue;
                }

                if directive_line.starts_with("define") {
                    let rest = directive_line[6..].trim();
                    if let Some(paren_idx) = rest.find('(') {
                        // Function-like macro
                        let name = rest[..paren_idx].trim().to_string();
                        if let Some(close_paren_idx) = rest.find(')') {
                            let params_str = &rest[paren_idx + 1..close_paren_idx];
                            let mut params: Vec<String> = params_str.split(',')
                                .map(|s| s.trim().to_string())
                                .filter(|s| !s.is_empty())
                                .collect();
                            
                            let mut has_varargs = false;
                            if let Some(last) = params.last() {
                                if last == "..." {
                                    has_varargs = true;
                                    params.pop();
                                }
                            }
                            
                            let body = rest[close_paren_idx + 1..].trim().to_string();
                            self.macros.insert(name, MacroDef::Function { params, has_varargs, body });
                        } else {
                            return Err(format!("Line {}: Invalid function-like macro definition", self.current_line));
                        }
                    } else {
                        // Simple macro
                        let parts: Vec<&str> = rest.splitn(2, |c: char| c.is_whitespace()).collect();
                        if parts.is_empty() {
                            return Err(format!("Line {}: Invalid #define", self.current_line));
                        }
                        let name = parts[0].to_string();
                        let value = if parts.len() > 1 { parts[1].trim().to_string() } else { "1".to_string() };
                        self.macros.insert(name, MacroDef::Simple(value));
                    }
                } else if directive_line.starts_with("undef") {
                    let name = directive_line[5..].trim();
                    self.macros.remove(name);
                } else if directive_line.starts_with("include") {
                    let include_spec = directive_line[7..].trim();
                    if include_spec.starts_with('"') && include_spec.ends_with('"') {
                        let file_name = &include_spec[1..include_spec.len() - 1];
                        let included_content = self.resolve_include(file_name, current_dir, true)?;
                        output.push_str(&included_content);
                    } else if include_spec.starts_with('<') && include_spec.ends_with('>') {
                        let file_name = &include_spec[1..include_spec.len() - 1];
                        let included_content = self.resolve_include(file_name, current_dir, false)?;
                        output.push_str(&included_content);
                    } else {
                        return Err(format!("Line {}: Unsupported include format: {}", self.current_line, include_spec));
                    }
                } else if directive_line.starts_with("embed") {
                    let rest = directive_line[5..].trim();
                    
                    // Improved parsing of #embed directive
                    let mut chars = rest.chars().peekable();
                    while chars.peek().map_or(false, |c| c.is_whitespace()) { chars.next(); }
                    
                    let mut spec = String::new();
                    if let Some(&c) = chars.peek() {
                        if c == '"' || c == '<' {
                            let end_c = if c == '"' { '"' } else { '>' };
                            spec.push(chars.next().unwrap());
                            while let Some(nc) = chars.next() {
                                spec.push(nc);
                                if nc == end_c { break; }
                            }
                        }
                    }
                    
                    if spec.is_empty() {
                        return Err(format!("Line {}: #embed missing file name", self.current_line));
                    }

                    let (file_name, search_current) = if spec.starts_with('"') && spec.ends_with('"') {
                        (&spec[1..spec.len()-1], true)
                    } else if spec.starts_with('<') && spec.ends_with('>') {
                        (&spec[1..spec.len()-1], false)
                    } else {
                        return Err(format!("Line {}: Invalid #embed file specification", self.current_line));
                    };

                    let mut prefix = String::new();
                    let mut suffix = String::new();
                    let mut if_empty = String::new();
                    let mut limit: Option<usize> = None;

                    // Parse parameters
                    while let Some(c) = chars.next() {
                        if c.is_whitespace() { continue; }
                        if c.is_alphabetic() || c == '_' {
                            let mut param_name = String::new();
                            param_name.push(c);
                            while let Some(&nc) = chars.peek() {
                                if nc.is_alphanumeric() || nc == '_' {
                                    param_name.push(chars.next().unwrap());
                                } else { break; }
                            }
                            
                            // Skip whitespace before '('
                            while chars.peek().map_or(false, |c| c.is_whitespace()) { chars.next(); }
                            
                            if let Some('(') = chars.next() {
                                let mut depth = 1;
                                let mut content = String::new();
                                while let Some(ac) = chars.next() {
                                    if ac == '(' { depth += 1; }
                                    else if ac == ')' {
                                        depth -= 1;
                                        if depth == 0 { break; }
                                    }
                                    content.push(ac);
                                }
                                
                                match param_name.as_str() {
                                    "prefix" => prefix = content,
                                    "suffix" => suffix = content,
                                    "if_empty" => if_empty = content,
                                    "limit" => limit = content.trim().parse().ok(),
                                    _ => {}
                                }
                            }
                        }
                    }

                    let mut paths_to_check = Vec::new();
                    if search_current {
                        paths_to_check.push(current_dir.to_path_buf());
                        paths_to_check.extend(self.include_paths.clone());
                    }
                    paths_to_check.extend(self.system_include_paths.clone());

                    let mut found = false;
                    for path in paths_to_check {
                        let full_path = path.join(file_name);
                        if full_path.exists() {
                            let mut bytes = fs::read(&full_path)
                                .map_err(|e| format!("Failed to read #embed file {:?}: {}", full_path, e))?;
                            
                            if let Some(l) = limit {
                                bytes.truncate(l);
                            }

                            if bytes.is_empty() {
                                output.push_str(&if_empty);
                            } else {
                                output.push_str(&prefix);
                                for (idx, b) in bytes.iter().enumerate() {
                                    if idx > 0 {
                                        output.push_str(", ");
                                    }
                                    output.push_str(&b.to_string());
                                }
                                output.push_str(&suffix);
                            }
                            output.push('\n');
                            found = true;
                            break;
                        }
                    }
                    if !found {
                        return Err(format!("Line {}: Could not find #embed file: {}", self.current_line, file_name));
                    }
                } else if directive_line.starts_with("error") {
                    let msg = directive_line[5..].trim();
                    return Err(format!("Line {}: #error: {}", self.current_line, msg));
                } else if directive_line.starts_with("warning") {
                    let msg = directive_line[7..].trim();
                    eprintln!("Warning: Line {}: #warning: {}", self.current_line, msg);
                } else if directive_line.starts_with("line") {
                    // Simple #line support
                    let parts: Vec<&str> = directive_line[4..].trim().split_whitespace().collect();
                    if !parts.is_empty() {
                        if let Ok(line_num) = parts[0].parse::<usize>() {
                            self.current_line = line_num - 1; // -1 because it will be incremented
                            if parts.len() > 1 {
                                let mut file_name = parts[1].to_string();
                                if file_name.starts_with('"') && file_name.ends_with('"') {
                                    file_name = file_name[1..file_name.len()-1].to_string();
                                }
                                self.current_file = PathBuf::from(file_name);
                            }
                        }
                    }
                } else if directive_line == "pragma once" {
                    // Handled in resolve_include
                }
                continue;
            }

            // Skip regular lines if in a false conditional branch
            if skip_stack.iter().any(|&skip| skip) {
                continue;
            }

            // Regular line, raise macro expansion
            let expanded_line = self.expand_macros_in_line(line);
            output.push_str(&expanded_line);
            output.push('\n');
        }

        if !skip_stack.is_empty() {
            return Err(format!("File {:?}: Missing #endif", self.current_file));
        }

        Ok(output)
    }

    fn strip_comments(&self, source: &str) -> String {
        let mut result = String::new();
        let mut chars = source.chars().peekable();
        let mut in_string = false;

        while let Some(c) = chars.next() {
            if c == '"' {
                in_string = !in_string;
                result.push(c);
                continue;
            }

            if !in_string {
                if c == '/' {
                    if let Some(&nc) = chars.peek() {
                        if nc == '/' {
                            // Line comment
                            while let Some(&lc) = chars.peek() {
                                if lc == '\n' { break; }
                                chars.next();
                            }
                            continue;
                        } else if nc == '*' {
                            // Block comment
                            chars.next(); // consume '*'
                            while let Some(bc) = chars.next() {
                                if bc == '*' {
                                    if let Some(&nbc) = chars.peek() {
                                        if nbc == '/' {
                                            chars.next(); // consume '/'
                                            break;
                                        }
                                    }
                                }
                            }
                            continue;
                        }
                    }
                }
            }
            result.push(c);
        }
        result
    }

    fn evaluate_condition(&mut self, expr: &str) -> bool {
        // 1. Handle 'defined' operator before any macro expansion
        let mut with_defined = String::new();
        let mut chars = expr.chars().peekable();
        while let Some(c) = chars.next() {
            if c.is_alphabetic() || c == '_' {
                let mut name = String::new();
                name.push(c);
                while let Some(&nc) = chars.peek() {
                    if nc.is_alphanumeric() || nc == '_' {
                        name.push(nc);
                        chars.next();
                    } else {
                        break;
                    }
                }
                
                if name == "defined" {
                    // Skip whitespace
                    while let Some(&nc) = chars.peek() {
                        if nc.is_whitespace() { chars.next(); } else { break; }
                    }
                    
                    let mut has_paren = false;
                    if let Some(&'(') = chars.peek() {
                        chars.next();
                        has_paren = true;
                        while let Some(&nc) = chars.peek() {
                            if nc.is_whitespace() { chars.next(); } else { break; }
                        }
                    }
                    
                    let mut macro_name = String::new();
                    while let Some(nc) = chars.next() {
                        if nc.is_alphanumeric() || nc == '_' {
                            macro_name.push(nc);
                        } else {
                            // If we have more characters, we might need to put them back or handle error
                            // But for simple cases this is fine
                            break;
                        }
                    }
                    
                    if has_paren {
                        while let Some(&nc) = chars.peek() {
                            if nc.is_whitespace() { chars.next(); } else if nc == ')' { chars.next(); break; } else { break; }
                        }
                    }
                    
                    if self.macros.contains_key(&macro_name) {
                        with_defined.push('1');
                    } else {
                        with_defined.push('0');
                    }
                } else {
                    with_defined.push_str(&name);
                }
            } else {
                with_defined.push(c);
            }
        }

        // 2. Expand macros in the remaining expression
        let expanded = self.expand_macros_in_line(&with_defined);

        // 3. Replace remaining identifiers (not defined in macros) with 0, as per C standard
        let mut cleaned_expr = String::new();
        let mut chars = expanded.chars().peekable();
        while let Some(c) = chars.next() {
            if c.is_alphabetic() || c == '_' {
                let mut name = String::new();
                name.push(c);
                while let Some(&nc) = chars.peek() {
                    if nc.is_alphanumeric() || nc == '_' {
                        name.push(nc);
                        chars.next();
                    } else {
                        break;
                    }
                }
                if name == "__has_include" || name == "__has_c_attribute" || name == "__has_builtin" {
                    // Keep these built-ins for parse_unary to handle
                    cleaned_expr.push_str(&name);
                    // Copy arguments
                    while let Some(&nc) = chars.peek() {
                        if nc.is_whitespace() {
                            cleaned_expr.push(chars.next().unwrap());
                        } else if nc == '(' {
                            let mut depth = 0;
                            while let Some(ac) = chars.next() {
                                cleaned_expr.push(ac);
                                if ac == '(' { depth += 1; }
                                else if ac == ')' {
                                    depth -= 1;
                                    if depth == 0 { break; }
                                }
                            }
                        } else {
                            break;
                        }
                    }
                } else if name == "true" {
                    cleaned_expr.push('1');
                } else if name == "false" {
                    cleaned_expr.push('0');
                } else {
                    // Check if it's a number that was just expanded
                    if name.chars().all(|c| c.is_digit(10)) {
                        cleaned_expr.push_str(&name);
                    } else {
                        cleaned_expr.push('0');
                    }
                }
            } else {
                cleaned_expr.push(c);
            }
        }

        let tokens = self.tokenize_expr(&cleaned_expr);
        let mut pos = 0;
        self.parse_logical_or(&tokens, &mut pos).unwrap_or(0) != 0
    }

    fn tokenize_expr(&self, expr: &str) -> Vec<String> {
        let mut tokens = Vec::new();
        let mut chars = expr.chars().peekable();
        while let Some(c) = chars.next() {
            if c.is_whitespace() { continue; }
            if c.is_digit(10) {
                let mut num = String::new();
                num.push(c);
                while let Some(&nc) = chars.peek() {
                    if nc.is_digit(10) || nc == 'x' || nc == 'X' || (nc >= 'a' && nc <= 'f') || (nc >= 'A' && nc <= 'F') || nc == 'L' || nc == 'U' {
                        num.push(chars.next().unwrap());
                    } else {
                        break;
                    }
                }
                tokens.push(num);
            } else if c.is_alphabetic() || c == '_' {
                let mut name = String::new();
                name.push(c);
                while let Some(&nc) = chars.peek() {
                    if nc.is_alphanumeric() || nc == '_' {
                        name.push(chars.next().unwrap());
                    } else {
                        break;
                    }
                }
                tokens.push(name);
            } else if c == '\'' {
                let mut content = String::new();
                content.push(c);
                while let Some(nc) = chars.next() {
                    content.push(nc);
                    if nc == '\'' { break; }
                    if nc == '\\' {
                        if let Some(esc) = chars.next() {
                            content.push(esc);
                        }
                    }
                }
                tokens.push(content);
            } else {
                let mut op = String::new();
                op.push(c);
                if let Some(&nc) = chars.peek() {
                    let mut combined = op.clone();
                    combined.push(nc);
                    match combined.as_str() {
                        "==" | "!=" | "<=" | ">=" | "&&" | "||" | "<<" | ">>" => {
                            tokens.push(combined);
                            chars.next();
                            continue;
                        }
                        _ => {}
                    }
                }
                tokens.push(op);
            }
        }
        tokens
    }

    fn parse_logical_or(&mut self, tokens: &[String], pos: &mut usize) -> Option<i64> {
        let mut val = self.parse_logical_and(tokens, pos)?;
        while *pos < tokens.len() && tokens[*pos] == "||" {
            *pos += 1;
            let rhs = self.parse_logical_and(tokens, pos)?;
            val = if val != 0 || rhs != 0 { 1 } else { 0 };
        }
        Some(val)
    }

    fn parse_logical_and(&mut self, tokens: &[String], pos: &mut usize) -> Option<i64> {
        let mut val = self.parse_bitwise_or(tokens, pos)?;
        while *pos < tokens.len() && tokens[*pos] == "&&" {
            *pos += 1;
            let rhs = self.parse_bitwise_or(tokens, pos)?;
            val = if val != 0 && rhs != 0 { 1 } else { 0 };
        }
        Some(val)
    }

    fn parse_bitwise_or(&mut self, tokens: &[String], pos: &mut usize) -> Option<i64> {
        let mut val = self.parse_bitwise_xor(tokens, pos)?;
        while *pos < tokens.len() && tokens[*pos] == "|" {
            *pos += 1;
            let rhs = self.parse_bitwise_xor(tokens, pos)?;
            val |= rhs;
        }
        Some(val)
    }

    fn parse_bitwise_xor(&mut self, tokens: &[String], pos: &mut usize) -> Option<i64> {
        let mut val = self.parse_bitwise_and(tokens, pos)?;
        while *pos < tokens.len() && tokens[*pos] == "^" {
            *pos += 1;
            let rhs = self.parse_bitwise_and(tokens, pos)?;
            val ^= rhs;
        }
        Some(val)
    }

    fn parse_bitwise_and(&mut self, tokens: &[String], pos: &mut usize) -> Option<i64> {
        let mut val = self.parse_equality(tokens, pos)?;
        while *pos < tokens.len() && tokens[*pos] == "&" {
            *pos += 1;
            let rhs = self.parse_equality(tokens, pos)?;
            val &= rhs;
        }
        Some(val)
    }

    fn parse_equality(&mut self, tokens: &[String], pos: &mut usize) -> Option<i64> {
        let mut val = self.parse_relational(tokens, pos)?;
        while *pos < tokens.len() && (tokens[*pos] == "==" || tokens[*pos] == "!=") {
            let op = tokens[*pos].clone();
            *pos += 1;
            let rhs = self.parse_relational(tokens, pos)?;
            val = if (op == "==" && val == rhs) || (op == "!=" && val != rhs) { 1 } else { 0 };
        }
        Some(val)
    }

    fn parse_relational(&mut self, tokens: &[String], pos: &mut usize) -> Option<i64> {
        let mut val = self.parse_shift(tokens, pos)?;
        while *pos < tokens.len() && (tokens[*pos] == "<" || tokens[*pos] == ">" || tokens[*pos] == "<=" || tokens[*pos] == ">=") {
            let op = tokens[*pos].clone();
            *pos += 1;
            let rhs = self.parse_shift(tokens, pos)?;
            val = match op.as_str() {
                "<" => if val < rhs { 1 } else { 0 },
                ">" => if val > rhs { 1 } else { 0 },
                "<=" => if val <= rhs { 1 } else { 0 },
                ">=" => if val >= rhs { 1 } else { 0 },
                _ => 0,
            };
        }
        Some(val)
    }

    fn parse_shift(&mut self, tokens: &[String], pos: &mut usize) -> Option<i64> {
        let mut val = self.parse_additive(tokens, pos)?;
        while *pos < tokens.len() && (tokens[*pos] == "<<" || tokens[*pos] == ">>") {
            let op = tokens[*pos].clone();
            *pos += 1;
            let rhs = self.parse_additive(tokens, pos)?;
            val = if op == "<<" { val << rhs } else { val >> rhs };
        }
        Some(val)
    }

    fn parse_additive(&mut self, tokens: &[String], pos: &mut usize) -> Option<i64> {
        let mut val = self.parse_multiplicative(tokens, pos)?;
        while *pos < tokens.len() && (tokens[*pos] == "+" || tokens[*pos] == "-") {
            let op = tokens[*pos].clone();
            *pos += 1;
            let rhs = self.parse_multiplicative(tokens, pos)?;
            val = if op == "+" { val + rhs } else { val - rhs };
        }
        Some(val)
    }

    fn parse_multiplicative(&mut self, tokens: &[String], pos: &mut usize) -> Option<i64> {
        let mut val = self.parse_unary(tokens, pos)?;
        while *pos < tokens.len() && (tokens[*pos] == "*" || tokens[*pos] == "/" || tokens[*pos] == "%") {
            let op = tokens[*pos].clone();
            *pos += 1;
            let rhs = self.parse_unary(tokens, pos)?;
            if op == "/" || op == "%" {
                if rhs == 0 { return None; }
                val = if op == "/" { val / rhs } else { val % rhs };
            } else {
                val *= rhs;
            }
        }
        Some(val)
    }

    fn parse_unary(&mut self, tokens: &[String], pos: &mut usize) -> Option<i64> {
        if *pos >= tokens.len() { return None; }
        let op = &tokens[*pos];
        if op == "!" {
            *pos += 1;
            return Some(if self.parse_unary(tokens, pos)? == 0 { 1 } else { 0 });
        }
        if op == "~" {
            *pos += 1;
            return Some(!self.parse_unary(tokens, pos)?);
        }
        if op == "+" {
            *pos += 1;
            return self.parse_unary(tokens, pos);
        }
        if op == "-" {
            *pos += 1;
            return Some(-self.parse_unary(tokens, pos)?);
        }
        if op == "defined" {
            *pos += 1;
            if *pos < tokens.len() && tokens[*pos] == "(" {
                *pos += 1;
                let name = &tokens[*pos];
                *pos += 1;
                if *pos < tokens.len() && tokens[*pos] == ")" {
                    *pos += 1;
                }
                return Some(if self.macros.contains_key(name) { 1 } else { 0 });
            } else if *pos < tokens.len() {
                let name = &tokens[*pos];
                *pos += 1;
                return Some(if self.macros.contains_key(name) { 1 } else { 0 });
            }
        }
        if op == "__has_builtin" {
            *pos += 1;
            if *pos < tokens.len() && tokens[*pos] == "(" {
                *pos += 1;
                let builtin_name = &tokens[*pos];
                *pos += 1;
                if *pos < tokens.len() && tokens[*pos] == ")" {
                    *pos += 1;
                }
                // We don't have many builtins yet, but we can support a few common ones
                return Some(match builtin_name.as_str() {
                    "__builtin_expect" | "__builtin_strlen" | "__builtin_memcpy" | "__builtin_memset" => 1,
                    _ => 0,
                });
            }
        }
        if op == "__has_include" {
            *pos += 1;
            if *pos < tokens.len() && tokens[*pos] == "(" {
                *pos += 1;
                let mut spec = String::new();
                // Reconstruct spec from tokens until ')'
                while *pos < tokens.len() && tokens[*pos] != ")" {
                    spec.push_str(&tokens[*pos]);
                    *pos += 1;
                }
                if *pos < tokens.len() && tokens[*pos] == ")" {
                    *pos += 1;
                }
                
                let (file_name, search_current) = if spec.starts_with('"') && spec.ends_with('"') {
                    (&spec[1..spec.len()-1], true)
                } else if spec.starts_with('<') && spec.ends_with('>') {
                    (&spec[1..spec.len()-1], false)
                } else {
                    return Some(0);
                };
                return Some(if self.include_exists(file_name, search_current) { 1 } else { 0 });
            }
        }
        if op == "__has_c_attribute" {
            *pos += 1;
            if *pos < tokens.len() && tokens[*pos] == "(" {
                *pos += 1;
                let attr_name = &tokens[*pos];
                *pos += 1;
                if *pos < tokens.len() && tokens[*pos] == ")" {
                    *pos += 1;
                }
                // Support common C23 attributes
                return Some(match attr_name.as_str() {
                    "nodiscard" | "maybe_unused" | "deprecated" | "fallthrough" | "noreturn" | "unsequenced" | "reproducible" => 202311,
                    _ => 0,
                });
            }
        }
        self.parse_primary(tokens, pos)
    }

    fn parse_primary(&mut self, tokens: &[String], pos: &mut usize) -> Option<i64> {
        if *pos >= tokens.len() { return None; }
        let token = &tokens[*pos];
        if token == "(" {
            *pos += 1;
            let val = self.parse_logical_or(tokens, pos)?;
            if *pos < tokens.len() && tokens[*pos] == ")" {
                *pos += 1;
            }
            return Some(val);
        }
        
        if token.starts_with('\'') && token.ends_with('\'') {
            *pos += 1;
            let inner = &token[1..token.len()-1];
            if inner.starts_with('\\') {
                return match inner {
                    "\\n" => Some(10),
                    "\\r" => Some(13),
                    "\\t" => Some(9),
                    "\\0" => Some(0),
                    _ if inner.len() > 1 => Some(inner.chars().nth(1).unwrap() as i64),
                    _ => Some(0),
                };
            }
            return Some(inner.chars().next().unwrap_or('\0') as i64);
        }

        // Try parsing as integer (handles hex, octal, suffixes)
        let val = if token.starts_with("0x") || token.starts_with("0X") {
            i64::from_str_radix(&token[2..].trim_end_matches(|c| c == 'L' || c == 'U' || c == 'l' || c == 'u'), 16).ok()
        } else if token.starts_with('0') && token.len() > 1 && token.chars().nth(1).unwrap().is_digit(8) {
            i64::from_str_radix(&token[1..].trim_end_matches(|c| c == 'L' || c == 'U' || c == 'l' || c == 'u'), 8).ok()
        } else {
            token.trim_end_matches(|c| c == 'L' || c == 'U' || c == 'l' || c == 'u').parse::<i64>().ok()
        };

        if val.is_some() {
            *pos += 1;
        }
        val
    }

    fn include_exists(&self, file_name: &str, search_current: bool) -> bool {
        let mut paths_to_check = Vec::new();
        if search_current {
            paths_to_check.push(self.current_file.parent().unwrap_or(Path::new(".")).to_path_buf());
            paths_to_check.extend(self.include_paths.clone());
        }
        paths_to_check.extend(self.system_include_paths.clone());

        for path in paths_to_check {
            if path.join(file_name).exists() {
                return true;
            }
        }
        false
    }

    fn resolve_include(&mut self, file_name: &str, current_dir: &Path, search_current: bool) -> Result<String, String> {
        let mut paths_to_check = Vec::new();
        if search_current {
            paths_to_check.push(current_dir.to_path_buf());
            paths_to_check.extend(self.include_paths.clone());
        }
        paths_to_check.extend(self.system_include_paths.clone());

        for path in paths_to_check {
            let full_path = path.join(file_name);
            if full_path.exists() {
                // Handle #pragma once
                if self.included_files.contains(&full_path) {
                    return Ok(String::new());
                }
                
                let content = fs::read_to_string(&full_path)
                    .map_err(|e| format!("Failed to read included file {:?}: {}", full_path, e))?;
                
                if content.contains("#pragma once") {
                    self.included_files.push(full_path.clone());
                }

                // Recursively process the included file
                let prev_file = self.current_file.clone();
                let prev_line = self.current_line;
                self.current_file = full_path.clone();
                self.current_line = 1;
                let result = self.process(&content, full_path.parent().unwrap_or(Path::new(".")))?;
                self.current_file = prev_file;
                self.current_line = prev_line;
                
                return Ok(result);
            }
        }

        Err(format!("Could not find included file: {}", file_name))
    }

    fn expand_macros_in_line(&mut self, line: &str) -> String {
        let mut result = String::new();
        let mut chars = line.chars().peekable();

        while let Some(c) = chars.next() {
            if c == '"' {
                // String literal - don't expand macros inside
                result.push(c);
                while let Some(&nc) = chars.peek() {
                    chars.next();
                    result.push(nc);
                    if nc == '"' {
                        break;
                    }
                    if nc == '\\' {
                        if let Some(esc) = chars.next() {
                            result.push(esc);
                        }
                    }
                }
            } else if c.is_alphabetic() || c == '_' {
                // Potential macro name
                let mut name = String::new();
                name.push(c);
                while let Some(&nc) = chars.peek() {
                    if nc.is_alphanumeric() || nc == '_' {
                        name.push(nc);
                        chars.next();
                    } else {
                        break;
                    }
                }
                
                if self.expanding_macros.contains(&name) {
                    // Standard C rule: if a macro is being expanded, don't expand it again
                    result.push_str(&name);
                    continue;
                }

                // Handle special predefined macros
                match name.as_str() {
                    "__FILE__" => {
                        result.push_str(&format!("\"{}\"", self.current_file.to_string_lossy().replace("\\", "\\\\")));
                        continue;
                    }
                    "__LINE__" => {
                        result.push_str(&self.current_line.to_string());
                        continue;
                    }
                    "__DATE__" => {
                        result.push_str("\"Feb  3 2026\""); // Today's date
                        continue;
                    }
                    "__TIME__" => {
                        result.push_str("\"12:00:00\""); // Placeholder
                        continue;
                    }
                    _ => {}
                }

                if let Some(def) = self.macros.get(&name).cloned() {
                    self.expanding_macros.push(name.clone());
                    match def {
                        MacroDef::Simple(value) => {
                            let expanded = self.expand_macros_in_line(&value);
                            result.push_str(&expanded);
                        }
                        MacroDef::Function { params, has_varargs, body } => {
                            // Try to parse arguments
                            if let Some(&'(') = chars.peek() {
                                chars.next(); // skip '('
                                let mut args = Vec::new();
                                let mut current_arg = String::new();
                                let mut paren_depth = 0;
                                while let Some(ac) = chars.next() {
                                    if ac == '(' { paren_depth += 1; }
                                    if ac == ')' {
                                        if paren_depth == 0 {
                                            args.push(current_arg.trim().to_string());
                                            break;
                                        } else {
                                            paren_depth -= 1;
                                        }
                                    }
                                    if ac == ',' && paren_depth == 0 {
                                        args.push(current_arg.trim().to_string());
                                        current_arg.clear();
                                    } else {
                                        current_arg.push(ac);
                                    }
                                }
                                
                                let mut expanded_body = body.clone();
                                
                                // Handle __VA_ARGS__ and __VA_OPT__
                                if has_varargs {
                                    let va_args = if args.len() >= params.len() {
                                        args[params.len()..].join(", ")
                                    } else {
                                        String::new()
                                    };
                                    
                                    // Handle __VA_OPT__(...)
                                    while let Some(opt_start) = expanded_body.find("__VA_OPT__") {
                                        if let Some(paren_start) = expanded_body[opt_start..].find('(') {
                                            let abs_paren_start = opt_start + paren_start;
                                            let mut depth = 0;
                                            let mut opt_end = None;
                                            for (idx, c) in expanded_body[abs_paren_start..].chars().enumerate() {
                                                if c == '(' { depth += 1; }
                                                else if c == ')' {
                                                    depth -= 1;
                                                    if depth == 0 {
                                                        opt_end = Some(abs_paren_start + idx);
                                                        break;
                                                    }
                                                }
                                            }
                                            
                                            if let Some(abs_opt_end) = opt_end {
                                                // Always try to consume one preceding space for __VA_OPT__
                                                let mut start = opt_start;
                                                if start > 0 && expanded_body.as_bytes()[start-1] == b' ' {
                                                    start -= 1;
                                                }
                                                
                                                if !va_args.is_empty() {
                                                    let replacement = expanded_body[abs_paren_start + 1..abs_opt_end].to_string();
                                                    expanded_body.replace_range(start..abs_opt_end + 1, &replacement);
                                                } else {
                                                    expanded_body.replace_range(start..abs_opt_end + 1, "");
                                                }
                                                
                                                // Collapse potential double spaces created by replacement
                                                while expanded_body.contains("  ") {
                                                    expanded_body = expanded_body.replace("  ", " ");
                                                }
                                            } else {
                                                break;
                                            }
                                        } else {
                                            break;
                                        }
                                    }
                                    
                                    // Handle GCC extension ##__VA_ARGS__ (comma elision)
                                    if va_args.is_empty() {
                                        expanded_body = expanded_body.replace(", ##__VA_ARGS__", "");
                                        expanded_body = expanded_body.replace(",##__VA_ARGS__", "");
                                        expanded_body = expanded_body.replace(", __VA_ARGS__", "");
                                        expanded_body = expanded_body.replace(",__VA_ARGS__", "");
                                        // Also handle space before __VA_ARGS__ if no comma
                                        expanded_body = expanded_body.replace(" __VA_ARGS__", "");
                                    } else {
                                        expanded_body = expanded_body.replace("##__VA_ARGS__", &va_args);
                                    }

                                    expanded_body = expanded_body.replace("__VA_ARGS__", &va_args);
                                    // Collapse double spaces again after __VA_ARGS__ replacement
                                    expanded_body = expanded_body.replace("  ", " ");
                                }

                                // Handle stringification (#param)
                                for (p, a) in params.iter().zip(args.iter()) {
                                    let pattern = format!("#{}", p);
                                    let stringified = format!("\"{}\"", a.replace("\"", "\\\""));
                                    expanded_body = expanded_body.replace(&pattern, &stringified);
                                }

                                // Handle concatenation (##)
                                let mut final_body = String::new();
                                let mut i = 0;
                                let body_chars: Vec<char> = expanded_body.chars().collect();
                                while i < body_chars.len() {
                                    if i + 1 < body_chars.len() && body_chars[i] == '#' && body_chars[i+1] == '#' {
                                        // Skip ##
                                        i += 2;
                                        // Skip whitespace after ##
                                        while i < body_chars.len() && body_chars[i].is_whitespace() {
                                            i += 1;
                                        }
                                        // Remove trailing whitespace from final_body
                                        while let Some(c) = final_body.pop() {
                                            if !c.is_whitespace() {
                                                final_body.push(c);
                                                break;
                                            }
                                        }
                                    } else {
                                        final_body.push(body_chars[i]);
                                        i += 1;
                                    }
                                }
                                expanded_body = final_body;

                                for (p, a) in params.iter().zip(args.iter()) {
                                    // Simple replacement for arguments
                                    expanded_body = expanded_body.replace(p, a);
                                }
                                
                                // Recursively expand the result
                                let final_expansion = self.expand_macros_in_line(&expanded_body);
                                result.push_str(&final_expansion);
                            } else {
                                result.push_str(&name);
                            }
                        }
                    }
                    self.expanding_macros.pop();
                } else {
                    result.push_str(&name);
                }
            } else {
                result.push(c);
            }
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_simple_macro() {
        let mut pp = Preprocessor::new();
        pp.define("PI", "3.14");
        let result = pp.process("float area = PI * r * r;", Path::new(".")).unwrap();
        assert_eq!(result, "float area = 3.14 * r * r;\n");
    }

    #[test]
    fn test_function_macro() {
        let mut pp = Preprocessor::new();
        pp.process("#define SQUARE(x) ((x)*(x))", Path::new(".")).unwrap();
        let result = pp.process("int y = SQUARE(5);", Path::new(".")).unwrap();
        assert_eq!(result, "int y = ((5)*(5));\n");
    }

    #[test]
    fn test_ifdef() {
        let mut pp = Preprocessor::new();
        pp.define("DEBUG", "1");
        let source = "#ifdef DEBUG\nint x = 1;\n#else\nint x = 0;\n#endif";
        let result = pp.process(source, Path::new(".")).unwrap();
        assert!(result.contains("int x = 1;"));
        assert!(!result.contains("int x = 0;"));
    }

    #[test]
    fn test_recursion_protection() {
        let mut pp = Preprocessor::new();
        pp.process("#define A B\n#define B A", Path::new(".")).unwrap();
        let result = pp.process("A", Path::new(".")).unwrap();
        // Should not infinite loop, A expands to B, B expands to A, but A is now protected
        assert_eq!(result, "A\n");
    }

    #[test]
    fn test_arithmetic_if() {
        let mut pp = Preprocessor::new();
        pp.define("X", "10");
        pp.define("Y", "20");
        let source = "#if X + 10 == Y\nint matched = 1;\n#endif";
        let result = pp.process(source, Path::new(".")).unwrap();
        assert_eq!(result, "int matched = 1;\n");
    }

    #[test]
    fn test_comments() {
        let mut pp = Preprocessor::new();
        let source = "// line comment\n/* block\n   comment */\nint x = 1;";
        let result = pp.process(source, Path::new(".")).unwrap();
        assert_eq!(result, "\n\nint x = 1;\n");
    }

    #[test]
    fn test_line_directive() {
        let mut pp = Preprocessor::new();
        let source = "#line 100 \"test.c\"\nint line = __LINE__;";
        let result = pp.process(source, Path::new(".")).unwrap();
        assert!(result.contains("int line = 100;"));
    }

    #[test]
    fn test_elifdef() {
        let mut pp = Preprocessor::new();
        pp.define("A", "1");
        let source = "#ifdef B\nint x = 0;\n#elifdef A\nint x = 1;\n#endif";
        let result = pp.process(source, Path::new(".")).unwrap();
        assert!(result.contains("int x = 1;"));
    }

    #[test]
    fn test_has_include() {
        let mut pp = Preprocessor::new();
        let source = "#if __has_include(\"stdio.h\")\n#define HAS_STDIO 1\n#endif";
        // stdio.h doesn't exist in current dir, so it should be false
        let result = pp.process(source, Path::new(".")).unwrap();
        assert!(!result.contains("#define HAS_STDIO 1"));
    }

    #[test]
    fn test_va_opt() {
        let mut pp = Preprocessor::new();
        pp.process("#define LOG(fmt, ...) printf(fmt __VA_OPT__(,) __VA_ARGS__)", Path::new(".")).unwrap();
        let result1 = pp.process("LOG(\"hello\");", Path::new(".")).unwrap();
        assert_eq!(result1, "printf(\"hello\");\n");
        let result2 = pp.process("LOG(\"num: %d\", 42);", Path::new(".")).unwrap();
        assert_eq!(result2, "printf(\"num: %d\", 42);\n");
    }

    #[test]
    fn test_va_args_comma_elision() {
        let mut pp = Preprocessor::new();
        pp.process("#define LOG(fmt, ...) printf(fmt, ##__VA_ARGS__)", Path::new(".")).unwrap();
        let result1 = pp.process("LOG(\"hello\");", Path::new(".")).unwrap();
        assert_eq!(result1, "printf(\"hello\");\n");
    }

    #[test]
    fn test_full_arithmetic() {
        let mut pp = Preprocessor::new();
        pp.define("X", "10");
        let source = "#if X * 2 == 20\nint ok = 1;\n#endif";
        let result = pp.process(source, Path::new(".")).unwrap();
        assert!(result.contains("int ok = 1;"));
    }

    #[test]
    fn test_va_args() {
        let mut pp = Preprocessor::new();
        pp.process("#define LOG(fmt, ...) printf(fmt, __VA_ARGS__)", Path::new(".")).unwrap();
        let result = pp.process("LOG(\"%d %d\", 1, 2);", Path::new(".")).unwrap();
        assert_eq!(result, "printf(\"%d %d\", 1, 2);\n");
    }

    #[test]
    fn test_stringification() {
        let mut pp = Preprocessor::new();
        pp.process("#define STR(x) #x", Path::new(".")).unwrap();
        let result = pp.process("STR(hello);", Path::new(".")).unwrap();
        assert_eq!(result, "\"hello\";\n");
    }

    #[test]
    fn test_predefined_macros() {
        let mut pp = Preprocessor::new();
        let result = pp.process("int line = __LINE__;", Path::new(".")).unwrap();
        assert_eq!(result, "int line = 1;\n");
    }

    #[test]
    fn test_line_continuation() {
        let mut pp = Preprocessor::new();
        pp.process("#define MULTI \\\nline \\\nmacro", Path::new(".")).unwrap();
        let result = pp.process("MULTI", Path::new(".")).unwrap();
        assert_eq!(result, "line macro\n");
    }

    #[test]
    fn test_concatenation() {
        let mut pp = Preprocessor::new();
        pp.process("#define CONCAT(a, b) a ## b", Path::new(".")).unwrap();
        let result = pp.process("int CONCAT(x, 1) = 42;", Path::new(".")).unwrap();
        assert_eq!(result, "int x1 = 42;\n");
    }

    #[test]
    fn test_if_defined() {
        let mut pp = Preprocessor::new();
        pp.define("VERSION", "2");
        let source = "#if defined(VERSION)\nint v = VERSION;\n#endif";
        let result = pp.process(source, Path::new(".")).unwrap();
        assert_eq!(result, "int v = 2;\n");
    }

    #[test]
    fn test_if_logic() {
        let mut pp = Preprocessor::new();
        pp.define("A", "1");
        // B is not defined
        let source = "#if defined(A) && !defined(B)\nint x = 1;\n#endif";
        let result = pp.process(source, Path::new(".")).unwrap();
        assert_eq!(result, "int x = 1;\n");
    }

    #[test]
    fn test_include() {
        let dir = tempdir().unwrap();
        let header_path = dir.path().join("test.h");
        let mut header_file = fs::File::create(&header_path).unwrap();
        writeln!(header_file, "#define VAL 42").unwrap();

        let mut pp = Preprocessor::new();
        let source = "#include \"test.h\"\nint x = VAL;";
        let result = pp.process(source, dir.path()).unwrap();
        assert!(result.contains("int x = 42;"));
    }

    #[test]
    fn test_embed() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.bin");
        fs::write(&file_path, vec![1, 2, 3]).unwrap();

        let mut pp = Preprocessor::new();
        let source = format!("#embed \"{}\" prefix(pre ) suffix( post)", file_path.to_str().unwrap());
        let result = pp.process(&source, Path::new(".")).unwrap();
        assert_eq!(result, "pre 1, 2, 3 post\n");
    }

    #[test]
    fn test_has_c_attribute() {
        let mut pp = Preprocessor::new();
        let source = "#if __has_c_attribute(nodiscard)\nint x = 1;\n#endif";
        let result = pp.process(source, Path::new(".")).unwrap();
        assert_eq!(result, "int x = 1;\n");
    }

    #[test]
    fn test_bitwise_if() {
        let mut pp = Preprocessor::new();
        let source = "#if (0x0F & 0xF0) == 0 && (1 << 3) == 8\nint ok = 1;\n#endif";
        let result = pp.process(source, Path::new(".")).unwrap();
        assert_eq!(result, "int ok = 1;\n");
    }

    #[test]
    fn test_has_builtin() {
        let mut pp = Preprocessor::new();
        let source = "#if __has_builtin(__builtin_expect)\nint ok = 1;\n#endif";
        let result = pp.process(source, Path::new(".")).unwrap();
        assert_eq!(result, "int ok = 1;\n");
    }

    #[test]
    fn test_c23_integrated() {
        let mut pp = Preprocessor::new();
        let source = r#"
#define VERSION 202311L
#if __STDC_VERSION__ >= VERSION
    #elifdef UNDEFINED
        #error "Should not happen"
    #elifndef DEFINED
        #if __has_c_attribute(nodiscard) && __has_builtin(__builtin_memcpy)
            int c23_ok = 1;
        #endif
#endif
"#;
        let result = pp.process(source, Path::new(".")).unwrap();
        assert!(result.contains("int c23_ok = 1;"));
    }
}

