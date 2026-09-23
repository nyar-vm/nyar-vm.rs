use std::collections::BTreeMap;

use nyar_language::typing::mro::{MemberSource, MethodConflictAnalyzer, MethodResolver, ParentInfo, PropertyConflictAnalyzer, PropertySource};

#[test]
fn resolves_renamed_parent_aliases() {
    let resolver = MethodResolver::new(
        vec![
            ParentInfo { name: "Teacher".to_string(), alias: Some("primary".to_string()) },
            ParentInfo { name: "Student".to_string(), alias: Some("secondary".to_string()) },
        ],
        vec!["TeachingAssistant".to_string(), "Teacher".to_string(), "Student".to_string()],
    );

    assert_eq!(resolver.resolve_qualified("primary"), Some("Teacher"));
    assert_eq!(resolver.resolve_qualified("secondary"), Some("Student"));
    assert_eq!(resolver.get_effective_parent("Teacher"), Some("Teacher"));
    assert!(resolver.is_valid_alias("primary"));
    assert!(!resolver.is_valid_alias("Teacher"));
}

#[test]
fn abstract_interface_conflict_requires_single_override() {
    let analyzer = MethodConflictAnalyzer::new(
        vec![
            ParentInfo { name: "Readable".to_string(), alias: Some("reader".to_string()) },
            ParentInfo { name: "Writable".to_string(), alias: Some("writer".to_string()) },
        ],
        vec!["ReadWrite".to_string(), "Readable".to_string(), "Writable".to_string()],
    );

    let mut inherited = BTreeMap::new();
    inherited.insert("close".to_string(), vec![MemberSource::abstract_member("Readable"), MemberSource::abstract_member("Writable")]);

    let conflicts = analyzer.analyze(inherited, &[]);

    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].method, "close");
    assert_eq!(conflicts[0].direct_parents, vec!["Readable", "Writable"]);
    assert_eq!(conflicts[0].mro_winner.as_deref(), Some("Readable"));
    assert!(conflicts[0].requires_override);
    assert!(!conflicts[0].overridden_in_child);
}

#[test]
fn child_override_resolves_multiple_concrete_parents() {
    let analyzer = MethodConflictAnalyzer::new(
        vec![
            ParentInfo { name: "Parent1".to_string(), alias: Some("p1".to_string()) },
            ParentInfo { name: "Parent2".to_string(), alias: Some("p2".to_string()) },
        ],
        vec!["Child".to_string(), "Parent1".to_string(), "Parent2".to_string()],
    );

    let mut inherited = BTreeMap::new();
    inherited.insert("render".to_string(), vec![MemberSource::concrete("Parent1"), MemberSource::concrete("Parent2")]);

    let conflicts = analyzer.analyze(inherited, &["render".to_string()]);

    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].method, "render");
    assert_eq!(conflicts[0].mro_winner.as_deref(), Some("Parent1"));
    assert!(conflicts[0].overridden_in_child);
    assert!(!conflicts[0].requires_override);
}

#[test]
fn c3_keeps_first_concrete_parent_when_child_does_not_override() {
    let analyzer = MethodConflictAnalyzer::new(
        vec![ParentInfo { name: "Parent1".to_string(), alias: None }, ParentInfo { name: "Parent2".to_string(), alias: None }],
        vec!["Child".to_string(), "Parent1".to_string(), "Parent2".to_string()],
    );

    let mut inherited = BTreeMap::new();
    inherited.insert("render".to_string(), vec![MemberSource::concrete("Parent1"), MemberSource::concrete("Parent2")]);

    let conflicts = analyzer.analyze(inherited, &[]);

    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].mro_winner.as_deref(), Some("Parent1"));
    assert!(!conflicts[0].overridden_in_child);
    assert!(!conflicts[0].requires_override);
}

#[test]
fn abstract_property_conflict_requires_override() {
    let analyzer = PropertyConflictAnalyzer::new(
        vec![
            ParentInfo { name: "Shape".to_string(), alias: Some("shape".to_string()) },
            ParentInfo { name: "Drawable".to_string(), alias: Some("drawable".to_string()) },
        ],
        vec!["Widget".to_string(), "Shape".to_string(), "Drawable".to_string()],
    );

    let mut inherited = BTreeMap::new();
    inherited.insert(
        "area".to_string(),
        vec![PropertySource::abstract_property("Shape", true, false), PropertySource::abstract_property("Drawable", true, false)],
    );

    let conflicts = analyzer.analyze(inherited, &[]);

    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].property, "area");
    assert_eq!(conflicts[0].direct_parents, vec!["Shape", "Drawable"]);
    assert!(conflicts[0].requires_override);
    assert!(conflicts[0].requires_getter);
    assert!(!conflicts[0].requires_setter);
}

#[test]
fn property_override_merges_getter_and_setter_requirements() {
    let analyzer = PropertyConflictAnalyzer::new(
        vec![ParentInfo { name: "Named".to_string(), alias: None }, ParentInfo { name: "MutableNamed".to_string(), alias: None }],
        vec!["Entity".to_string(), "Named".to_string(), "MutableNamed".to_string()],
    );

    let mut inherited = BTreeMap::new();
    inherited.insert(
        "name".to_string(),
        vec![PropertySource::abstract_property("Named", true, false), PropertySource::abstract_property("MutableNamed", true, true)],
    );

    let conflicts = analyzer.analyze(inherited, &["name".to_string()]);

    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].property, "name");
    assert!(conflicts[0].overridden_in_child);
    assert!(!conflicts[0].requires_override);
    assert!(conflicts[0].requires_getter);
    assert!(conflicts[0].requires_setter);
}
