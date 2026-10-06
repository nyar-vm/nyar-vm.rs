#[cfg(test)]
mod tests {
    use crate::valkyrie::frontend::{parse_source_with_language, reject_unexpanded_target_templates, reject_unsupported_template_source};
    use oak_valkyrie::ValkyrieLanguage;

    #[test]
    fn rejects_unexpanded_target_templates() {
        let error = reject_unexpanded_target_templates("micro main() -> i32 { <% match arch %> return 1 }")
            .expect_err("unexpanded template must fail before Oak parse");
        assert!(error.to_string().contains("TGrammar"), "{error}");
    }

    #[test]
    fn rejects_template_source_when_support_t_grammar_disabled() {
        let error = reject_unsupported_template_source("micro main() -> i32 { <% if true %> return 1 <% end if %> }")
            .expect_err("template source must fail when support_t_grammar is false");
        assert!(error.to_string().contains("support_t_grammar"), "{error}");
    }

    #[test]
    fn parse_with_support_t_grammar_false_rejects_templates() {
        let language = ValkyrieLanguage { support_t_grammar: false, ..ValkyrieLanguage::default() };
        parse_source_with_language("micro main() -> i32 { return 1 }", &language).expect("plain source should parse");
        let template_error = parse_source_with_language("micro main() -> i32 { <% match arch %> return 1 }", &language)
            .expect_err("template source must fail");
        assert!(template_error.to_string().contains("support_t_grammar"), "{template_error}");
    }

    #[test]
    fn accepts_plain_source() {
        reject_unexpanded_target_templates("micro main() -> i32 { return 1 }").expect("plain source must pass staging gate");
    }
}
