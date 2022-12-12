
use oak_go::{GoLanguage, GoLexer};
use oak_core::{Lexer, ParseSession};
use oak_vfs::{DiskVfs, Vfs};

fn main() {
    let vfs = DiskVfs::new();
    let source_text = vfs.get_source("tests/basic.go").unwrap();
    let language = GoLanguage::default();
    let lexer = GoLexer::new(&language);
    let mut session = ParseSession::<GoLanguage>::default();
    let output = lexer.lex(&source_text, &[], &mut session);
    
    if let Ok(tokens) = output.result.as_ref() {
        for token in tokens.iter() {
            let text = source_text.get_text_in(token.span.clone());
            println!("{:?}: {:?} ({:?})", token.kind, text, token.span);
        }
    } else {
        println!("Lexer errors: {:?}", output.diagnostics);
    }
}
