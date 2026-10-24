use chomsky_extract::Backend;
use nyar_types::{NyarFrontend, QualifiedName};
use odin_vm::{
    bytecode::compiler::NyarBackend,
    vm::{value::Value, NyarVM},
};
use rusty_python::RustyPythonFrontend;

#[test]
fn test_python_oop_execution() {
    let source = r#"class Person:
    def __init__(self, name):
        self.name = name
    def say(self):
        return self.name

p = Person("Alice")
res = p.say()
"#;
    let frontend = RustyPythonFrontend::new();
    let ast = frontend.parse(source).expect("Failed to parse source");

    let vfs = oak_vfs::MemoryVfs::new();
    let tree = frontend.lower(&ast, &vfs).expect("Failed to lower AST to tree");

    let mut backend = NyarBackend::new();
    let artifact = backend.generate(&tree).expect("Failed to generate artifact");

    let binary = match artifact {
        chomsky_extract::BackendArtifact::Binary(b) => b,
        _ => panic!("Expected binary artifact"),
    };

    let module = odin_vm::bytecode::format::NyarcModule::parse(&binary).expect("Failed to decode module");

    let mut vm = NyarVM::new();
    let module_idx = vm.load_module(module);

    // The main code is in the last chunk of the module
    let main_chunk_idx = (vm.modules[module_idx].chunks.len() - 1);
    vm.execute(module_idx, main_chunk_idx).expect("Failed to execute");

    // Check results
    let res_name = QualifiedName::from("res");
    let res = vm.builtins.get(&res_name).expect("Global 'res' not found");

    assert_eq!(res.try_as_str(), Some("Alice"));
}
