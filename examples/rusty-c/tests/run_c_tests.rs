use rusty_c::frontend::RustyCFrontend;
use rusty_c::runtime::RustyCRuntime;
use nyar_types::{NyarContext, NyarFrontend, EGraph};
use oak_vfs::MemoryVfs;
use std::path::Path;

#[test]
fn test_simple_return() {
    let source = "int main() { return 42; }";
    let frontend = RustyCFrontend::new();
    let ast = frontend.parse(source).expect("Failed to parse");
    
    let mut egraph = EGraph::new();
    let vfs = MemoryVfs::new();
    let mut ctx = NyarContext::new(&mut egraph, &vfs, 0);
    
    let root_id = frontend.lower_unified(&ast, &mut ctx);
    
    let mut runtime = RustyCRuntime::new();
    runtime.execute((egraph, root_id)).expect("Failed to execute");
}

#[test]
fn test_hello_world() {
    let source = r#"
        int printf(const char* format, ...);
        int main() {
            printf("Hello, World!\n");
            return 0;
        }
    "#;
    let frontend = RustyCFrontend::new();
    let ast = frontend.parse(source).expect("Failed to parse");
    
    let mut egraph = EGraph::new();
    let vfs = MemoryVfs::new();
    let mut ctx = NyarContext::new(&mut egraph, &vfs, 0);
    
    let root_id = frontend.lower_unified(&ast, &mut ctx);
    
    let mut runtime = RustyCRuntime::new();
    runtime.execute((egraph, root_id)).expect("Failed to execute");
}

#[test]
fn test_control_flow() {
    let source = r#"
        int main() {
            int a = 10;
            int b = 20;
            if (a < b) {
                return a + b;
            } else {
                return a - b;
            }
        }
    "#;
    let frontend = RustyCFrontend::new();
    let ast = frontend.parse(source).expect("Failed to parse");
    
    let mut egraph = EGraph::new();
    let vfs = MemoryVfs::new();
    let mut ctx = NyarContext::new(&mut egraph, &vfs, 0);
    
    let root_id = frontend.lower_unified(&ast, &mut ctx);
    
    let mut runtime = RustyCRuntime::new();
    runtime.execute((egraph, root_id)).expect("Failed to execute");
}
