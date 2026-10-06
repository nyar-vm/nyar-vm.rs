#[cfg(test)]
mod tests {
    use crate::valkyrie::frontend::{parse_source_with_language, reject_unsupported_template_source};
    use oak_valkyrie::ValkyrieLanguage;

    #[test]
    fn rejects_template_source_when_support_t_grammar_disabled() {
        let error = reject_unsupported_template_source("micro main() -> i32 { <% if true %> return 1 <% end if %> }")
            .expect_err("template source must fail when support_t_grammar is false");
        assert!(error.to_string().contains("support_t_grammar"), "{error}");
    }

    #[test]
    fn parse_with_support_t_grammar_true_accepts_structured_match() {
        let language = ValkyrieLanguage { support_t_grammar: true, ..ValkyrieLanguage::default() };
        parse_source_with_language(
            r#"<% match arch %>
<% case "wasm32" %>
micro main() -> i32 { return 1 }
<% end %>"#,
            &language,
        )
        .expect("structured TGrammar must parse through Oak");
    }

    #[test]
    fn parse_with_support_t_grammar_false_rejects_templates() {
        let language = ValkyrieLanguage { support_t_grammar: false, ..ValkyrieLanguage::default() };
        parse_source_with_language("micro main() -> i32 { return 1 }", &language).expect("plain source should parse");
        let template_error = parse_source_with_language("micro main() -> i32 { <% match arch %> return 1 }", &language)
            .expect_err("template source must fail");
        assert!(template_error.to_string().contains("support_t_grammar"), "{template_error}");
    }
}
