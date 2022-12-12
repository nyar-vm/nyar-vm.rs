//! AWSL semantic tokens (`:prop`, `@event`, ABI script markers).

use std_data::text::awsl::collect_template_bindings;

use crate::state::{DocumentState, ServerState};
use oak_lsp::types::SemanticToken;

/// AWSL semantic token type ids (custom legend slots 3–7).
const TOKEN_ABI_PROPERTY: u32 = 5;
const TOKEN_ABI_EVENT: u32 = 6;
const TOKEN_EMIT: u32 = 3;
const TOKEN_EFFECT: u32 = 7;
const TOKEN_MEMO: u32 = 4;

pub struct AwslSemanticTokensHandler;

impl AwslSemanticTokensHandler {
    pub fn collect(doc: &DocumentState, tokens: &mut Vec<SemanticToken>) {
        let Some(root) = &doc.awsl_root else {
            return;
        };

        for binding in collect_template_bindings(root) {
            let (token_type, len) = match binding.kind {
                std_data::text::awsl::TemplateBindingKind::Property => (TOKEN_ABI_PROPERTY, binding.name.len() + 1),
                std_data::text::awsl::TemplateBindingKind::Event => (TOKEN_ABI_EVENT, binding.name.len() + 1),
            };
            push_token(doc, tokens, binding.key_span.start, len, token_type);
        }

        let Some(abi) = &doc.component_abi else {
            return;
        };
        let base = doc.awsl_script_base.unwrap_or(0);
        for property in &abi.properties {
            push_token(doc, tokens, base + property.span.start, property.name.len(), TOKEN_ABI_PROPERTY);
        }
        for event in &abi.events {
            push_token(doc, tokens, base + event.span.start, event.name.len(), TOKEN_ABI_EVENT);
        }
        for memo in &abi.memoized {
            push_token(doc, tokens, base + memo.span.start, memo.name.len(), TOKEN_MEMO);
        }
        for effect in &abi.effects {
            push_token(doc, tokens, base + effect.span.start, "effect".len(), TOKEN_EFFECT);
        }

        if let Some(ast) = &doc.ast {
            Self::collect_script_tokens(doc, &ast.items, tokens);
        }
    }

    fn collect_script_tokens(
        doc: &DocumentState,
        items: &[oak_valkyrie::ast::Item],
        tokens: &mut Vec<SemanticToken>,
    ) {
        use oak_valkyrie::ast::{Expr, Item, Statement};
        let base = doc.awsl_script_base.unwrap_or(0);
        let prefix = doc.awsl_synthetic_prefix.unwrap_or(0);
        for item in items {
            match item {
                Item::Widget(w) => Self::collect_script_tokens(doc, &w.items, tokens),
                Item::Statement(stmt) => match stmt {
                    Statement::ExprStmt { expr, .. } => {
                        Self::collect_emit_tokens(doc, expr, base, prefix, tokens);
                    }
                    Statement::Let { expr, .. } => {
                        Self::collect_emit_tokens(doc, expr, base, prefix, tokens);
                    }
                },
                _ => {}
            }
        }
    }

    fn collect_emit_tokens(
        doc: &DocumentState,
        expr: &oak_valkyrie::ast::Expr,
        base: usize,
        prefix: usize,
        tokens: &mut Vec<SemanticToken>,
    ) {
        use oak_valkyrie::ast::Expr;
        if let Expr::Call { callee, .. } = expr {
            if let Expr::Ident(id) = callee.as_ref() {
                if id.name == "emit" || id.name == "effect" {
                    let file_offset = base + id.span.start.saturating_sub(prefix);
                    let token_type = if id.name == "emit" { TOKEN_EMIT } else { TOKEN_EFFECT };
                    push_token(doc, tokens, file_offset, id.name.len(), token_type);
                }
            }
        }
    }
}

fn push_token(doc: &DocumentState, tokens: &mut Vec<SemanticToken>, offset: usize, length: usize, token_type: u32) {
    let pos = doc.offset_to_position(offset);
    tokens.push(SemanticToken {
        delta_line: pos.line,
        delta_start: pos.character,
        length: length as u32,
        token_type,
        token_modifiers_bitset: 0,
    });
}
