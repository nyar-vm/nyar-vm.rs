#[cfg(test)]
mod tests {
    use crate::valkyrie::frontend::reject_unexpanded_target_templates;

    #[test]
    fn rejects_unexpanded_target_templates() {
        let error = reject_unexpanded_target_templates("micro main() -> i32 { <% match arch %> return 1 }")
            .expect_err("unexpanded template must fail before Oak parse");
        assert!(error.to_string().contains("未展开的目标模板"));
    }

    #[test]
    fn accepts_plain_source() {
        reject_unexpanded_target_templates("micro main() -> i32 { return 1 }").expect("plain source must pass staging gate");
    }
}
