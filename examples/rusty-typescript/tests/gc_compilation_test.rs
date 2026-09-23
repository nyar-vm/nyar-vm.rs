use chomsky_emit::GaiaEmitter;
use chomsky_extract::Backend;
use mini_typescript::MiniTypescriptFrontend;
use nyar_types::NyarFrontend;

#[test]
fn test_typescript_class_to_wasm_gc_asm() {
    let frontend = MiniTypescriptFrontend::new();
    let source = r#"
        class Point {
            x: number;
            y: number;
            constructor(x: number, y: number) {
                this.x = x;
                this.y = y;
            }
            sum(): number {
                return this.x + this.y;
            }
        }
        let p = new Point(10, 20);
        let s = p.x;
    "#;

    // 1. Parse and lower to IKunTree
    let ast = frontend.parse(source).expect("Failed to parse");
    let tree = frontend.lower(&ast).expect("Failed to lower");

    // 2. Use GaiaEmitter to generate assembly
    let emitter = GaiaEmitter::new("wasm-gc");
    let artifact = emitter
        .generate(&tree)
        .expect("Failed to generate Gaia assembly");

    let asm = match artifact {
        chomsky_extract::BackendArtifact::Source(s) => s,
        _ => panic!("Expected source artifact"),
    };

    println!("Generated Gaia Assembly:\n{}", asm);

    // 3. Verify GC-specific instructions are present
    assert!(
        asm.contains(".type struct Point"),
        "Should contain struct definition"
    );
    assert!(asm.contains(".field x"), "Should contain field x");
    assert!(asm.contains(".field y"), "Should contain field y");
    assert!(
        asm.contains("struct.new Point"),
        "Should contain struct.new Point"
    );
    assert!(asm.contains("struct.get x"), "Should contain struct.get x");
}
