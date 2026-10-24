//! Migrated from `src/package/desktop/mod.rs` (`tests`).

use super::*;

use crate::awsl::{LoweringOptions, lower_component};
use oak_awsl::AwslParser;

#[test]
fn package_desktop_no_gtk_artifacts() {
    let source = r#"<widget Demo><text>hi</text></widget>"#;
    let root = AwslParser::parse_root(source).expect("parse");
    let component = lower_component(&root, "demo", "demo.awsl", &LoweringOptions::default());
    for platform in ["windows", "linux", "macos"] {
        let out = package_desktop_project(&[component.clone()], "app", platform, &[]);
        for (name, _) in &out.files {
            assert!(!name.to_ascii_lowercase().contains("gtk"), "platform={platform} file={name}");
            assert!(!name.to_ascii_lowercase().contains("sdl"), "platform={platform} file={name}");
        }
        let runtime = out.files.iter().find(|(n, _)| n.contains("Runtime")).expect("runtime asset");
        if let PackageArtifact::Text(src) = &runtime.1 {
            assert!(!src.contains("asgard_gtk"));
            assert!(!src.contains("gtk/gtk.h"));
            assert!(!src.contains("GtkWidget"));
            assert!(!src.contains("AsgardGtk"));
        }
    }
}
