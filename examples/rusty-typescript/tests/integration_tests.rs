use mini_typescript::{project::ProjectLoader, MiniTypescriptFrontend};
use nyar_vm::vm::interpreter::NyarVM;
use std::path::Path;

#[test]
fn test_basic_compilation() {
    let mut frontend = MiniTypescriptFrontend::new();
    let source = "let x = 42; function add(a, b) { return a + b; } x = add(x, 10);";

    let result = frontend.compile_to_nyar(source);
    assert!(result.is_ok(), "Compilation failed: {:?}", result.err());

    let module = result.unwrap();
    let mut vm = NyarVM::new();
    vm.load_module(module.into());

    // Execute the main script
    vm.execute(0, 0).expect("Execution failed");
}

#[test]
fn test_multi_file_project() {
    let project_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("multi-file");

    let mut loader = ProjectLoader::new(&project_dir);
    let modules = loader
        .load_project(&project_dir)
        .expect("Failed to load project");

    assert!(modules.len() >= 2, "Should have loaded at least 2 modules");

    let mut vm = NyarVM::new();
    // Load all modules into VM
    for module in modules {
        vm.load_module(module.into());
    }

    // Run the main module (index 0) to initialize globals
    vm.execute(0, 0).expect("Failed to run main module");

    // Call the main function
    let result = vm
        .execute_symbol("main", vec![])
        .expect("Failed to call main function");

    // index.ts: sum + prod = (10+20) + (10*20) = 30 + 200 = 230
    assert_eq!(unsafe { result.as_int() }, 230);
}
