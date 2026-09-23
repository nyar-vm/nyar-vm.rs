//! Static guards for the Canonical Semantic MIR boundary.
//!
//! These files are the only code allowed to decide whether submitted MIR is
//! semantically complete. Backend implementations remain intentionally out of
//! this scan: they are physical mappings and currently diagnostic-only.

use std::{fs, path::PathBuf};

fn read(relative: &str) -> String {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = root.join(relative);
    fs::read_to_string(&path).unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
}

fn semantic_validator_source() -> String {
    let source = read("../nyar-language/src/valkyrie/mir/validation.rs");
    let end = source.find("pub fn validate_module").expect("semantic validator delimiter must exist");
    source[..end].to_string()
}

fn production_source(source: String) -> String {
    source.split("#[cfg(test)]").next().expect("source prefix must exist").to_string()
}

#[test]
fn formal_semantic_path_forbids_semantic_reconstruction_patterns() {
    let sources = [
        ("SMIR-G001", "name-based semantic dispatch", production_source(read("src/lowering/features/semantic_mir_contract.rs"))),
        ("SMIR-G001", "name-based semantic dispatch", semantic_validator_source()),
        ("SMIR-G002", "descriptor semantic fallback", read("../nyar-language/src/valkyrie/frontend_contract/executable.rs")),
        ("SMIR-G005", "host carrier to source encoding inference", read("../nyar-language/src/valkyrie/frontend_contract/executable.rs")),
    ];
    let forbidden = [
        ("SMIR-G001", ".symbol.contains("),
        ("SMIR-G001", ".symbol.starts_with("),
        ("SMIR-G001", ".symbol.ends_with("),
        ("SMIR-G001", "\"von\""),
        ("SMIR-G001", "\"Option\""),
        ("SMIR-G001", "\"Result\""),
        ("SMIR-G002", "descriptor"),
        ("SMIR-G003", "infer_output"),
        ("SMIR-G004", "operand_stack"),
        ("SMIR-G004", "stack_shape"),
        ("SMIR-G005", "java.lang.String"),
        ("SMIR-G005", "System.String"),
        ("SMIR-G005", "JS string"),
        ("SMIR-G005", "WASM_GC_ANYREF"),
        ("SMIR-G006", "unwrap_or(NyarType::Integer32"),
        ("SMIR-G006", "unwrap_or(ValkyrieType::Integer32"),
        ("SMIR-G006", "default_i32"),
        ("SMIR-G006", "default_void"),
        ("SMIR-G006", "default_object"),
        ("SMIR-G006", "default_anyref"),
    ];
    for (rule, description, source) in sources {
        for (forbidden_rule, pattern) in forbidden {
            if rule == forbidden_rule || matches!(forbidden_rule, "SMIR-G003" | "SMIR-G004" | "SMIR-G006") {
                assert!(
                    !source.contains(pattern),
                    "{rule}: {description} must not contain forbidden semantic reconstruction pattern `{pattern}`"
                );
            }
        }
    }
}

#[test]
fn managed_backend_entries_gate_before_preparation() {
    for (path, gate, first_preparation) in [
        ("src/lowering/backends/clr/mod.rs", "validate_submission(submission)", "let mut submission = submission.clone()"),
        ("src/lowering/backends/jvm/mod.rs", "validate_submission(submission)", "validate_jvm_call_contracts(submission)"),
        (
            "src/lowering/backends/wasm/mod.rs",
            "validate_submission(submission)",
            "validate_text_encoding_projection(submission, host_boundary)",
        ),
    ] {
        let source = read(path);
        let gate_at = source.find(gate).unwrap_or_else(|| panic!("{path} has no shared Semantic MIR gate"));
        let preparation_at = source.find(first_preparation).unwrap_or_else(|| panic!("{path} has no preparation marker"));
        assert!(gate_at < preparation_at, "{path} must validate Semantic MIR before backend preparation");
    }
}
