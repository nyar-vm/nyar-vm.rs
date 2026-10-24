use rusty_valkyrie::{types::Identifier, *};
use nyar_types::IntrinsicId;

use rusty_valkyrie::mir::ssa::resolve_intrinsic_id;

#[test]
fn resolve_intrinsic_id_maps_builtin_paths_and_private_seeds() {
    let push = NamePath::new(vec![Identifier::new("builtin"), Identifier::new("array"), Identifier::new("push")]);
    assert_eq!(resolve_intrinsic_id(&push), Some(IntrinsicId::ArrayPush));
    let len = NamePath::new(vec![Identifier::new("__array_len")]);
    assert_eq!(resolve_intrinsic_id(&len), Some(IntrinsicId::ArrayLen));
    let get = NamePath::new(vec![Identifier::new("__array_get")]);
    assert_eq!(resolve_intrinsic_id(&get), Some(IntrinsicId::ArrayGet));
    let set = NamePath::new(vec![Identifier::new("__array_set")]);
    assert_eq!(resolve_intrinsic_id(&set), Some(IntrinsicId::ArraySet));
    let qualified_deref = NamePath::new(vec![Identifier::new("marker"), Identifier::new("__ref_deref")]);
    assert_eq!(resolve_intrinsic_id(&qualified_deref), Some(IntrinsicId::RefDeref));
    // 禁止按类型名末段猜
    let hashmap_get = NamePath::new(vec![Identifier::new("HashMap"), Identifier::new("get")]);
    assert_eq!(resolve_intrinsic_id(&hashmap_get), None);
    let array_get_method = NamePath::new(vec![Identifier::new("Array"), Identifier::new("get")]);
    assert_eq!(resolve_intrinsic_id(&array_get_method), None);
}
