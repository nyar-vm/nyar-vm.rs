use rusty_typescript::RustyTypescriptFrontend;
use nyar_types::NyarFrontend;

#[test]
fn test_namespace_compilation() {
    let source = r#"
        namespace Math {
            export function add(a: number, b: number) {
                return a + b;
            }
        }
        console.log(Math.add(1, 2));
    "#;
    let frontend = RustyTypescriptFrontend::new();
    let result = frontend.compile_to_nyar(source);
    assert!(result.is_ok(), "Failed to compile namespace: {:?}", result.err());
}

#[test]
fn test_dynamic_import_compilation() {
    let source = r#"
        async function load() {
            const math = await import("./math");
            return math.add(1, 2);
        }
    "#;
    let frontend = RustyTypescriptFrontend::new();
    let result = frontend.compile_to_nyar(source);
    assert!(result.is_ok(), "Failed to compile dynamic import: {:?}", result.err());
}
