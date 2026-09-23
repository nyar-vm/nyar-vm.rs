use std::{fs, path::PathBuf};

use nyar::{assert_or_regenerate_text_sidecar, collect_fixture_cases_with_extensions, regenerate_enabled};
use nyar_analyzer::highlight::{HighlightRequest, HighlighterRegistry};
use nyar_language::valkyrie::highlight::ValkyrieHighlighterProvider;

use super::dump::dump_highlight_snapshot;

/// `legion` 持有的 Valkyrie 文本快照 fixture 根目录。
pub fn legion_text_fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../legion/tests/fixtures/text/valkyrie")
}

fn language_id_for_fixture(fixture_path: &PathBuf) -> &'static str {
    if fixture_path.extension().and_then(|value| value.to_str()).is_some_and(|value| value.eq_ignore_ascii_case("vx")) {
        "vx"
    }
    else {
        "valkyrie"
    }
}

#[test]
fn text_fixture_highlight_regression() {
    let root = legion_text_fixtures_root();
    if !root.exists() {
        panic!("fixture root not found: {}", root.display());
    }

    let regenerate = regenerate_enabled();
    let cases = collect_fixture_cases_with_extensions(&root, &["v", "vx"]);
    assert!(!cases.is_empty(), "no .v/.vx fixtures found under {}", root.display());

    let mut registry = HighlighterRegistry::new();
    registry.register_provider(&ValkyrieHighlighterProvider);

    for fixture_path in cases {
        let source =
            fs::read_to_string(&fixture_path).unwrap_or_else(|error| panic!("failed to read fixture '{}': {}", fixture_path.display(), error));

        let language_id = language_id_for_fixture(&fixture_path);
        let request = HighlightRequest::source_only(&source);
        let spans = registry
            .highlight_merged(language_id, &request)
            .unwrap_or_else(|| panic!("highlight_merged returned None for {}", fixture_path.display()));

        let observed = dump_highlight_snapshot(&source, &spans);
        assert_or_regenerate_text_sidecar(&fixture_path, "highlight", &observed, regenerate);
    }
}
