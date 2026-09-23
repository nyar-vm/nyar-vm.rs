#![cfg(test)]
use miette::Diagnostic;
use nyar_language::ValkyrieCompiler;

#[test]
fn compile_legion_tools_preprocessed() {
    let source = std::fs::read_to_string(r"E:\Goddess of Victory\valkyrie.v\dist\_pp_fresh.v").expect("pp");
    let compiler = ValkyrieCompiler::default();
    match compiler.compile_source_to_build_output(&source) {
        Ok(_) => println!("OK"),
        Err(e) => {
            println!("ERR: {e}");
            // try to get span from ParseError
            let s = format!("{e:?}");
            println!("DEBUG: {s}");
            if let Some(span) = match &e {
                std_data::text::valkyrie::ParseError::Invalid { span, .. } => span.clone(),
                _ => None,
            } {
                let start = span.start.saturating_sub(200);
                let end = (span.end + 200).min(source.len());
                println!("SPAN {span:?}");
                println!("CTX:\n{}", &source[start..end]);
                // map file via offsets
                for line in std::fs::read_to_string(r"E:\Goddess of Victory\valkyrie.v\dist\_off_fresh.txt").unwrap().lines() {
                    let Some((range, path)) = line.split_once(": ")
                    else {
                        continue;
                    };
                    let Some((a, b)) = range.split_once('-')
                    else {
                        continue;
                    };
                    let a: usize = a.parse().unwrap();
                    let b: usize = b.parse().unwrap();
                    if a <= span.start && span.start < b {
                        println!("FILE {path} local {}", span.start - a);
                        break;
                    }
                }
            }
            panic!("compile failed: {e}");
        }
    }
}
