use asgard::{
    awsl::{LoweringOptions, lower_component},
    codegen::build_awsl_wasm_source,
    ssg::project::valkyrie_v_roots,
};
use oak_awsl::AwslParser;

fn main() {
    let roots = valkyrie_v_roots().expect("valkyrie.v 源码库根目录须由 `VALKYRIE_V_ROOT` 显式提供");
    let path = roots[0].join("projects/asgard._/projects/asgard.plotter/source/components/interactive-col-plot.awsl");
    let source = std::fs::read_to_string(&path).unwrap();
    let root = AwslParser::parse_root(&source).unwrap();
    let lowered = lower_component(&root, "interactive-col-plot", "interactive-col-plot.awsl", &LoweringOptions::default());
    let v_source = build_awsl_wasm_source(&[lowered]);
    let pos = 8365usize;
    let start = pos.saturating_sub(120);
    let end = (pos + 120).min(v_source.len());
    println!("{}", &v_source[start..end]);
    if let Some(i) = v_source.find('\\') {
        println!("first backslash at {}", i);
        println!("{}", &v_source[i.saturating_sub(40)..(i + 80).min(v_source.len())]);
    }
}
