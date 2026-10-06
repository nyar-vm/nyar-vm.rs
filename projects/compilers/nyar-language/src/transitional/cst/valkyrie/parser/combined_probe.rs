//! Bisect preprocessed legion.tools parse failures.
#![cfg(test)]

use super::AstParser;
use crate::transitional::cst::valkyrie::parser::ParseError;

fn span_of(err: &ParseError) -> Option<std::ops::Range<usize>> {
    match err {
        ParseError::Invalid { span, .. } => span.clone(),
        _ => None,
    }
}

#[test]
fn bisect_preprocessed() {
    let source = std::fs::read_to_string(r"E:\Goddess of Victory\valkyrie.v\dist\_preprocessed_now.v").unwrap();
    let offsets: Vec<(usize, usize, String)> = std::fs::read_to_string(r"E:\Goddess of Victory\valkyrie.v\dist\_offsets_now.txt")
        .unwrap()
        .lines()
        .filter_map(|line| {
            let (range, path) = line.split_once(": ")?;
            let (a, b) = range.split_once('-')?;
            Some((a.parse().ok()?, b.parse().ok()?, path.to_string()))
        })
        .collect();
    println!("files {} source_len {}", offsets.len(), source.len());

    // Verify offsets cover source (with trailing newlines between files)
    let last_end = offsets.last().map(|o| o.1).unwrap_or(0);
    println!("last_end {} (+1 newline => {})", last_end, last_end + 1);

    fn try_prefix(source: &str, end: usize) -> (bool, String, Option<std::ops::Range<usize>>) {
        // end is exclusive byte offset into combined preprocessed source
        let prefix = &source[..end.min(source.len())];
        match AstParser::parse_root(prefix) {
            Ok(_) => (false, String::new(), None),
            Err(e) => {
                let msg = e.to_string();
                (true, msg, span_of(&e))
            }
        }
    }

    // Binary search on file boundaries
    let mut lo = 1usize;
    let mut hi = offsets.len();
    let (pf, msg, sp) = try_prefix(&source, source.len());
    println!("full pf={pf} msg={msg} span={sp:?}");
    assert!(pf, "expected failure on full source");
    while lo < hi {
        let mid = (lo + hi) / 2;
        let end = offsets[mid - 1].1 + 1; // include trailing newline after file
        let end = end.min(source.len());
        let (pf, msg, _) = try_prefix(&source, end);
        let name = std::path::Path::new(&offsets[mid - 1].2).file_name().and_then(|s| s.to_str()).unwrap_or("?");
        println!("mid={mid} end={end} pf={pf} msg={} :: {}", &msg[..msg.len().min(80)], name);
        if pf {
            hi = mid;
        }
        else {
            lo = mid + 1;
        }
    }
    println!("FIRST FAIL FILE #{} {}", lo, offsets[lo - 1].2);
    let end = (offsets[lo - 1].1 + 1).min(source.len());
    let (pf, msg, sp) = try_prefix(&source, end);
    println!("fail msg={msg} span={sp:?}");
    if let Some(sp) = sp {
        let local = sp.start.saturating_sub(offsets[lo - 1].0);
        println!("local byte in file {}", local);
        let ctx_start = sp.start.saturating_sub(120);
        let ctx_end = (sp.end + 120).min(source.len());
        println!("CTX:\n{}", &source[ctx_start..ctx_end]);
    }
    // Also show prev file ok
    if lo > 1 {
        let end_prev = (offsets[lo - 2].1 + 1).min(source.len());
        let (pf2, msg2, _) = try_prefix(&source, end_prev);
        println!("prev ok? {} msg={}", !pf2, msg2);
    }
}
