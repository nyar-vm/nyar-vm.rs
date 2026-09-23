//! Bash host-script semantic bridge.

use std_data::text::bash::{BashScript, BashStmt};

/// Bridge over a parsed Bash script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BashSemanticBridge {
    /// Parsed script.
    pub script: BashScript,
    /// Exported function / command names.
    pub exported_commands: Vec<String>,
}

impl BashSemanticBridge {
    /// Build from a script, collecting top-level function names.
    pub fn from_script(script: BashScript) -> Self {
        let exported_commands = script
            .statements
            .iter()
            .filter_map(|stmt| match stmt {
                BashStmt::FunctionDef { name, .. } => Some(name.clone()),
                _ => None,
            })
            .collect();
        Self { script, exported_commands }
    }

    /// Language-neutral exported symbol names.
    pub fn exported_symbols(&self) -> &[String] {
        &self.exported_commands
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std_data::text::bash::BashScript;

    #[test]
    fn collects_top_level_functions() {
        let script = BashScript::parse("greet() { echo hi; }\necho done").expect("parse");
        let bridge = BashSemanticBridge::from_script(script);
        assert_eq!(bridge.exported_symbols(), &["greet".to_string()]);
    }
}
