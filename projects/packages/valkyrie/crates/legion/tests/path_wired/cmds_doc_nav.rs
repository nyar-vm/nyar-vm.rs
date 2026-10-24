//! Migrated from `src/cmds/doc/nav.rs` (`tests`).

use super::*;

#[test]
fn nav_sections_for_awsl_groups_children() {
    let nodes = vec![
        NavNode {
            title: "guides".into(),
            relative_path: "guides".into(),
            href: String::new(),
            children: vec![NavNode {
                title: "Intro".into(),
                relative_path: "guides/intro.md".into(),
                href: "intro.html".into(),
                children: vec![],
            }],
        },
        NavNode { title: "Root page".into(), relative_path: "root.md".into(), href: "root.html".into(), children: vec![] },
    ];
    let sections = nav_sections_for_awsl(&nodes, "", "guides/intro.md");
    assert_eq!(sections.len(), 2);
    assert_eq!(sections[0]["title"], "guides");
    assert_eq!(sections[0]["pages"].as_array().unwrap().len(), 1);
    assert_eq!(sections[0]["pages"][0]["link_class"], "vp-nav-link active");
    assert_eq!(sections[1]["title"], "");
}

#[test]
fn sanitize_dir_name_replaces_spaces() {
    assert_eq!(sanitize_dir_name("用户指南"), "用户指南");
    assert_eq!(sanitize_dir_name("a b"), "a_b");
}
