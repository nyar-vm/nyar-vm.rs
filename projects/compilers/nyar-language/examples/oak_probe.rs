use oak_core::{ParseSession, SourceText};
use oak_valkyrie::{ValkyrieLanguage, ValkyrieParser};

fn main() {
    let src = "[main] micro entry() -> i32 { return 23 }";
    let language = ValkyrieLanguage::default();
    let parser = ValkyrieParser::new(&language);
    let text = SourceText::new(src);
    let mut session = ParseSession::<ValkyrieLanguage>::default();
    let tree = parser.parse(&text, &mut session);
    println!("parse ok: {}", tree.is_ok());
    if let Err(e) = tree {
        println!("parse err: {e}");
    }
}
