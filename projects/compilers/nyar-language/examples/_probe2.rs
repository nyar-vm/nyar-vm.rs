use nyar_language::ValkyrieCompiler;
fn main() {
    let source = r#"namespace std.collection;
class ArrayList<T> { _items: [T], _capacity: usize }
class Array<T> { _address: usize, _length: usize }
[intrinsic("array.get")] private micro __array_get<T>(self: Array<T>, ordinal: usize): T { }
imply Array<T> {
    micro get(self, ordinal: usize): Option<T> { return Some(__array_get(self, ordinal)) }
}
imply ArrayList<T> {
    [host_contract]
    micro get(self, ordinal: usize): Option<T> {
        if ordinal == 0 || ordinal > self.length() { return None }
        return self._items.get(ordinal)
    }
    [host_contract]
    micro length(self): usize { return self._items.length }
}
"#;
    let mir = ValkyrieCompiler::default().compile_source_to_mir(source).expect("mir");
    for f in &mir.functions {
        if !f.symbol.contains("ArrayList.get") { continue; }
        for ins in f.blocks.iter().flat_map(|b| &b.instructions) {
            println!("{:?}", ins.kind);
        }
    }
}
