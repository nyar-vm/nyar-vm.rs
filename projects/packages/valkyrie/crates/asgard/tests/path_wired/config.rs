//! Migrated from `src/config.rs` (`tests`).

use super::*;

#[test]
fn parse_ui_registry_config() {
    let config = VoaConfig::parse(
        r#"define_config(asgard) {
ui {
    themes = [
        { id = "fate", label = "Fate" }
    ]
    modes = [
        { id = "light", label = "Light" }
    ]
}
}"#,
    )
    .expect("parse ui registry");
    assert_eq!(config.ui.themes.len(), 1);
    assert_eq!(config.ui.themes[0].id, "fate");
    assert_eq!(config.ui.modes.len(), 1);
    assert_eq!(config.ui.modes[0].id, "light");
}

#[test]
fn parse_define_config_block() {
    let config = VoaConfig::parse(
        r#"define_config(asgard) {
project_type = "application"
platform = "browser"
build {
    output = "dist"
    mode = "prod"
}
}"#,
    )
    .expect("parse define_config");
    assert_eq!(config.project_type, "application");
    assert_eq!(config.platform, "browser");
    assert_eq!(config.build.output, "dist");
}
