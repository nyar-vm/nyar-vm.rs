use nyar_language::typing::mro::*;

#[test]
fn test_single_class() {
    let result = C3Linearization::compute("A", vec![]);
    assert_eq!(result.unwrap(), vec!["A"]);
}

#[test]
fn test_single_inheritance() {
    let mro_a = vec!["A".to_string()];
    let result = C3Linearization::compute("B", vec![mro_a]);
    assert_eq!(result.unwrap(), vec!["B", "A"]);
}

#[test]
fn test_diamond_inheritance() {
    let mro_b = vec!["B".to_string(), "A".to_string()];
    let mro_c = vec!["C".to_string(), "A".to_string()];
    let result = C3Linearization::compute("D", vec![mro_b, mro_c]);
    assert_eq!(result.unwrap(), vec!["D", "B", "C", "A"]);
}

#[test]
fn test_complex_diamond() {
    let mro_a = vec!["A".to_string(), "O".to_string()];
    let mro_b = vec!["B".to_string(), "O".to_string()];
    let mro_c = vec!["C".to_string(), "O".to_string()];
    let mro_d = vec!["D".to_string(), "B".to_string(), "O".to_string()];
    let mro_e = vec!["E".to_string(), "C".to_string(), "O".to_string()];

    let mro_f = C3Linearization::compute("F", vec![mro_d.clone(), mro_e.clone()]).unwrap();
    assert_eq!(mro_f, vec!["F", "D", "B", "E", "C", "O"]);

    let mro_g = C3Linearization::compute("G", vec![mro_a, mro_b, mro_c]).unwrap();
    assert_eq!(mro_g, vec!["G", "A", "B", "C", "O"]);
}

#[test]
fn test_python_example() {
    let mro_a = vec!["A".to_string(), "O".to_string()];
    let mro_b = vec!["B".to_string(), "O".to_string()];
    let mro_c = vec!["C".to_string(), "O".to_string()];
    let mro_k1 = C3Linearization::compute("K1", vec![mro_a.clone(), mro_b.clone(), mro_c.clone()]).unwrap();
    assert_eq!(mro_k1, vec!["K1", "A", "B", "C", "O"]);

    let mro_k2 = C3Linearization::compute("K2", vec![mro_b.clone(), mro_c.clone()]).unwrap();
    assert_eq!(mro_k2, vec!["K2", "B", "C", "O"]);

    let mro_k3 = C3Linearization::compute("K3", vec![mro_a, mro_c]).unwrap();
    assert_eq!(mro_k3, vec!["K3", "A", "C", "O"]);

    let mro_z = C3Linearization::compute("Z", vec![mro_k1, mro_k2, mro_k3]).unwrap();
    assert_eq!(mro_z, vec!["Z", "K1", "K2", "K3", "A", "B", "C", "O"]);
}

#[test]
fn test_inconsistent_hierarchy() {
    let mro_z1 = vec!["Z1".to_string(), "X".to_string(), "Y".to_string(), "A".to_string()];
    let mro_z2 = vec!["Z2".to_string(), "Y".to_string(), "X".to_string(), "A".to_string()];

    let result = C3Linearization::compute("W", vec![mro_z1, mro_z2]);
    assert!(matches!(result, Err(MroError::InconsistentHierarchy { .. })));
}

#[test]
fn test_circular_inheritance() {
    let mro_a = vec!["B".to_string(), "A".to_string()];
    let mro_b = vec!["A".to_string(), "B".to_string()];

    let result = C3Linearization::compute("C", vec![mro_a, mro_b]);
    assert!(matches!(result, Err(MroError::InconsistentHierarchy { .. })));
}

#[test]
fn test_error_display() {
    let err = MroError::InconsistentHierarchy { class: "TestClass".to_string(), details: "Cannot merge".to_string() };
    assert!(err.to_string().contains("TestClass"));

    let err = MroError::CircularInheritance { chain: vec!["A".to_string(), "B".to_string(), "A".to_string()] };
    assert!(err.to_string().contains("A -> B -> A"));
}

#[test]
fn test_renamed_inheritance_method_resolver() {
    let parents = vec![
        ParentInfo { name: "Teacher".to_string(), alias: Some("primary".to_string()) },
        ParentInfo { name: "Student".to_string(), alias: Some("secondary".to_string()) },
    ];
    let mro = vec!["TA".to_string(), "Teacher".to_string(), "Student".to_string()];
    let resolver = MethodResolver::new(parents, mro);

    assert_eq!(resolver.resolve_qualified("primary"), Some("Teacher"));
    assert_eq!(resolver.resolve_qualified("secondary"), Some("Student"));
    assert_eq!(resolver.resolve_qualified("nonexistent"), None);
    assert!(resolver.is_valid_alias("primary"));
    assert!(!resolver.is_valid_alias("Teacher"));
}

#[test]
fn test_method_resolver_get_effective_parent() {
    let parents = vec![
        ParentInfo { name: "Parent1".to_string(), alias: Some("primary".to_string()) },
        ParentInfo { name: "Parent2".to_string(), alias: Some("secondary".to_string()) },
    ];
    let mro = vec!["Child".to_string(), "Parent1".to_string(), "Parent2".to_string()];
    let resolver = MethodResolver::new(parents, mro);

    assert_eq!(resolver.get_effective_parent("primary"), Some("Parent1"));
    assert_eq!(resolver.get_effective_parent("secondary"), Some("Parent2"));
    assert_eq!(resolver.get_effective_parent("Parent1"), Some("Parent1"));
    assert_eq!(resolver.get_effective_parent("unknown"), None);
}

#[test]
fn test_method_resolver_without_alias() {
    let parents = vec![ParentInfo { name: "Parent1".to_string(), alias: None }, ParentInfo { name: "Parent2".to_string(), alias: None }];
    let mro = vec!["Child".to_string(), "Parent1".to_string(), "Parent2".to_string()];
    let resolver = MethodResolver::new(parents, mro);

    assert_eq!(resolver.resolve_qualified("primary"), None);
    assert_eq!(resolver.get_effective_parent("Parent1"), Some("Parent1"));
    assert_eq!(resolver.get_effective_parent("Parent2"), Some("Parent2"));
}
