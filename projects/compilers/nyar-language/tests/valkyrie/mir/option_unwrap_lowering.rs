use nyar_language::{
    MirOperand, MirOperation, SourceID, ValkyrieCompiler,
    valkyrie::mir::ssa::MirLowerer,
};

fn has_bare_unwrap_call(function: &nyar_language::MirFunction) -> bool {
    function.blocks.iter().any(|block| {
        block.instructions.iter().any(|instruction| {
            matches!(
                &instruction.kind,
                MirOperation::Call { callee: MirOperand::Symbol(path), .. }
                    if path.parts().len() == 1 && path.parts()[0].as_str() == "unwrap"
            )
        })
    })
}

#[test]
fn get_then_unwrap_lowers_to_sum_payload_get() {
    let hir = ValkyrieCompiler::new(SourceID { version_id: 9910 })
        .compile_source(
            r#"
namespace std.collection;

class ArrayList<T> {
    _items: [T]
    _capacity: usize
}

imply ArrayList<T> {
    micro get(self, ordinal: usize): Option<T> {
        return None
    }

    micro probe(self, ordinal: usize): i32 {
        return self.get(ordinal).unwrap()
    }
}
"#,
        )
        .expect("compile ArrayList probe");

    let mir = MirLowerer::lower_module_semantic(&hir);
    let probe = mir
        .functions
        .iter()
        .find(|function| function.symbol.ends_with("probe"))
        .expect("ArrayList.probe should lower");

    assert!(
        !has_bare_unwrap_call(probe),
        "probe must not emit bare `unwrap` Call: {:?}",
        probe
            .blocks
            .iter()
            .flat_map(|block| block.instructions.iter())
            .map(|ins| format!("{ins:?}"))
            .collect::<Vec<_>>()
    );
    assert!(
        probe.blocks.iter().any(|block| {
            block
                .instructions
                .iter()
                .any(|instruction| matches!(&instruction.kind, MirOperation::SumPayloadGet { .. }))
        }),
        "probe should emit SumPayloadGet for unwrap"
    );
}

#[test]
fn chained_unwrap_lowers_without_bare_call() {
    let hir = ValkyrieCompiler::new(SourceID { version_id: 9911 })
        .compile_source(
            r#"
namespace std.collection;

class ArrayList<T> {
    _items: [T]
    _capacity: usize
}

imply ArrayList<T> {
    micro get(self, ordinal: usize): Option<Option<T>> {
        return None
    }

    micro nested(self, ordinal: usize): i32 {
        return self.get(ordinal).unwrap().unwrap()
    }
}
"#,
        )
        .expect("compile chained unwrap");

    let mir = MirLowerer::lower_module_semantic(&hir);
    let nested = mir
        .functions
        .iter()
        .find(|function| function.symbol.ends_with("nested"))
        .expect("ArrayList.nested should lower");

    assert!(
        !has_bare_unwrap_call(nested),
        "nested must not emit bare `unwrap` Call: {:?}",
        nested
            .blocks
            .iter()
            .flat_map(|block| block.instructions.iter())
            .map(|ins| format!("{ins:?}"))
            .collect::<Vec<_>>()
    );
}

#[test]
fn unwrap_in_comparison_lowers_without_bare_call() {
    let hir = ValkyrieCompiler::new(SourceID { version_id: 9912 })
        .compile_source(
            r#"
namespace std.collection;

class SwissTable<K, V> {
    _states: ArrayList<i32>
    _entries: ArrayList<Option<i32>>
    _length: usize
    _used: usize
}

imply SwissTable<K, V> {
    micro cmp(self, index: usize): bool {
        return self._states.get(index + 1).unwrap() == 1
    }
}
"#,
        )
        .expect("compile SwissTable cmp");

    let mir = MirLowerer::lower_module_semantic(&hir);
    let cmp = mir
        .functions
        .iter()
        .find(|function| function.symbol.ends_with("cmp"))
        .expect("SwissTable.cmp should lower");

    assert!(!has_bare_unwrap_call(cmp), "cmp must not emit bare `unwrap` Call");
}

#[test]
fn swiss_table_find_slot_shape_lowers_without_bare_unwrap() {
    let hir = ValkyrieCompiler::new(SourceID { version_id: 9913 })
        .compile_source(
            r#"
namespace std.collection;

structure SwissTableEntry<K, V> {
    key: K
    value: V
    hash: usize
}

class ArrayList<T> {
    _items: [T]
    _capacity: usize
}

imply ArrayList<T> {
    micro length(self): usize {
        return self._items.length
    }

    micro get(self, ordinal: usize): Option<T> {
        return None
    }
}

class SwissTable<K, V> {
    _states: ArrayList<i32>
    _entries: ArrayList<Option<SwissTableEntry<K, V>>>
    _length: usize
    _used: usize
}

imply SwissTable<K, V> {
    micro find_slot(self, key: K): Option<usize> {
        let slot_count: usize = self._states.length()
        if slot_count == 0 {
            return None
        }

        let key_hash: usize = key.hash()
        let mut index: usize = key_hash % slot_count
        let mut probe: usize = 0
        while probe < slot_count {
            let state: i32 = self._states.get(index + 1).unwrap()
            if state == 0 {
                return None
            }

            if state == 1 {
                let entry: SwissTableEntry<K, V> = self._entries.get(index + 1).unwrap().unwrap()
                if entry.hash == key_hash && entry.key == key {
                    return Some(index)
                }
            }

            index = (index + 1) % slot_count
            probe = probe + 1
        }

        return None
    }
}
"#,
        )
        .expect("compile SwissTable.find_slot shape");

    let mir = MirLowerer::lower_module_semantic(&hir);
    let find_slot = mir
        .functions
        .iter()
        .find(|function| function.symbol.ends_with("find_slot"))
        .expect("SwissTable.find_slot should lower");

    assert!(
        !has_bare_unwrap_call(find_slot),
        "find_slot must not emit bare `unwrap` Call: {:?}",
        find_slot
            .blocks
            .iter()
            .flat_map(|block| block.instructions.iter())
            .map(|ins| format!("{ins:?}"))
            .collect::<Vec<_>>()
    );
}
