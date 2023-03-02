use nyar_language::{SourceID, ValkyrieCompiler, mir::ssa::MirLowerer, mir_function_to_executable, concretize_type_lossy};

fn main() {
    let hir = ValkyrieCompiler::new(SourceID { version_id: 1 })
        .compile_source(r#"
class Box<T> {
    _items: [T]
    _cap: usize
}
imply Box<T> {
    micro new(cap: usize): Self {
        return Box { _items: [], _cap: cap }
    }
}
"#).expect("compile");
    let mir = MirLowerer::lower_module_semantic(&hir);
    let new_fn = mir.functions.iter().find(|f| f.symbol.ends_with("new")).expect("new");
    println!("symbol={}", new_fn.symbol);
    println!("return={:?}", new_fn.return_type);
    println!("return_lossy={:?}", concretize_type_lossy(&new_fn.return_type));
    for (k,v) in &new_fn.value_types {
        println!("v{:?} => {:?} / {:?}", k, v, concretize_type_lossy(v));
    }
    let exec = mir_function_to_executable(new_fn);
    println!("exec_return={:?}", exec.return_type);
    for (k,v) in &exec.value_types {
        println!("ev{:?} => {:?}", k, v);
    }
    for layout in &mir.aggregate_layouts.layouts {
        if layout.name == "Box" {
            println!("layout Box fields={:?}", layout.fields.iter().map(|f| (&f.name, &f.ty)).collect::<Vec<_>>());
        }
    }
}
