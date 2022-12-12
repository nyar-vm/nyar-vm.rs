use crate::{state::ServerState, types::Position};
use oak_lsp::types::*;
use oak_valkyrie::ast::*;

/// 签名帮助处理器
pub struct SignatureHelpHandler;

impl SignatureHelpHandler {
    pub async fn handle(state: &ServerState, uri: &str, position: Position) -> Option<SignatureHelp> {
        let doc = state.get_document(uri)?;
        let offset = doc.position_to_offset(position) as u32;
        let ast = doc.ast.as_ref()?;

        // 找到当前位置的函数调用
        let mut call_node = None;
        Self::find_call_at(&ast.items, offset as usize, &mut call_node);

        if let Some((callee, args, span)) = call_node {
            let caller_name = match callee {
                Expr::Ident(ident) => ident.name.clone(),
                Expr::Path(path) => path.parts.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join("::"),
                _ => "function".to_string(),
            };

            // 尝试从索引中获取真实的参数名称
            let caller_pos = doc.offset_to_position(span.start);
            let mut param_names = Vec::new();
            if let Some(info) = state.query_symbol_at_position(uri, caller_pos).await {
                if let Some(ty) = info.type_info {
                    if let Some(start) = ty.find('(') {
                        if let Some(end) = ty.rfind(')') {
                            let params_str = &ty[start + 1..end];
                            for p in params_str.split(',') {
                                let name = p.split(':').next().unwrap_or("").trim();
                                if !name.is_empty() {
                                    param_names.push(name.to_string());
                                }
                            }
                        }
                    }
                }
            }

            let mut parameters = Vec::new();
            for (i, _) in args.iter().enumerate() {
                let name = if i < param_names.len() { param_names[i].clone() } else { format!("arg{}", i) };
                parameters.push(ParameterInformation { label: name, documentation: None });
            }

            let active_parameter = args
                .iter()
                .position(|arg| {
                    let arg_span = match arg {
                        Expr::Ident(i) => i.span.clone(),
                        Expr::Path(p) => p.span.clone(),
                        Expr::StringLiteral(s) => s.span.clone(),
                        Expr::Bool { span, .. } => span.clone(),
                        Expr::Unary { span, .. } => span.clone(),
                        Expr::Binary { span, .. } => span.clone(),
                        Expr::Call { span, .. } => span.clone(),
                        Expr::Field { span, .. } => span.clone(),
                        Expr::Index { span, .. } => span.clone(),
                        Expr::Offset { span, .. } => span.clone(),
                        Expr::Paren { span, .. } => span.clone(),
                        Expr::Block(b) => b.span.clone(),
                        Expr::Lambda(l) => l.span.clone(),
                        Expr::Object { span, .. } => span.clone(),
                        Expr::AnonymousClass { span, .. } => span.clone(),
                        Expr::If { span, .. } => span.clone(),
                        Expr::Match { span, .. } => span.clone(),
                        Expr::Loop { span, .. } => span.clone(),
                        Expr::Return { span, .. } => span.clone(),
                        Expr::Break { span, .. } => span.clone(),
                        Expr::Continue { span, .. } => span.clone(),
                        Expr::Yield { span, .. } => span.clone(),
                        Expr::Raise { span, .. } => span.clone(),
                        Expr::Resume { span, .. } => span.clone(),
                        Expr::Catch { span, .. } => span.clone(),
                        Expr::With { span, .. } => span.clone(),
                        Expr::SuperCall { span, .. } => span.clone(),
                    };
                    arg_span.contains(&(offset as usize))
                })
                .map(|p| p as u32);

            Some(SignatureHelp {
                signatures: vec![SignatureInformation {
                    label: format!(
                        "{}({})",
                        caller_name,
                        parameters.iter().map(|p| p.label.as_str()).collect::<Vec<_>>().join(", ")
                    ),
                    documentation: Some(format!("Signature help for {}", caller_name)),
                    parameters: Some(parameters),
                    active_parameter,
                }],
                active_signature: Some(0),
                active_parameter: active_parameter.or(Some(0)),
            })
        }
        else {
            None
        }
    }

    fn find_call_at<'a>(
        items: &'a [Item],
        offset: usize,
        found: &mut Option<(&'a Expr, &'a [Expr], core::range::Range<usize>)>,
    ) {
        for item in items {
            match item {
                Item::TypeFunction(func) => {
                    Self::find_call_in_block(&func.body, offset, found);
                }
                Item::Micro(micro) => {
                    Self::find_call_in_block(&micro.body, offset, found);
                }
                Item::Class(cls) => {
                    Self::find_call_at(&cls.items, offset, found);
                }
                Item::Namespace(ns) => {
                    Self::find_call_at(&ns.items, offset, found);
                }
                Item::Widget(w) => {
                    Self::find_call_at(&w.items, offset, found);
                }
                Item::Statement(stmt) => {
                    Self::find_call_in_stmt(stmt, offset, found);
                }
                _ => {}
            }
            if found.is_some() {
                return;
            }
        }
    }

    fn find_call_in_stmt<'a>(
        stmt: &'a Statement,
        offset: usize,
        found: &mut Option<(&'a Expr, &'a [Expr], core::range::Range<usize>)>,
    ) {
        match stmt {
            Statement::Let { expr, .. } => {
                Self::find_call_in_expr(expr, offset, found);
            }
            Statement::ExprStmt { expr, .. } => {
                Self::find_call_in_expr(expr, offset, found);
            }
        }
    }

    fn find_call_in_block<'a>(
        block: &'a Block,
        offset: usize,
        found: &mut Option<(&'a Expr, &'a [Expr], core::range::Range<usize>)>,
    ) {
        for stmt in &block.statements {
            Self::find_call_in_stmt(stmt, offset, found);
            if found.is_some() {
                return;
            }
        }
    }

    fn find_call_in_expr<'a>(
        expr: &'a Expr,
        offset: usize,
        found: &mut Option<(&'a Expr, &'a [Expr], core::range::Range<usize>)>,
    ) {
        match expr {
            Expr::Call { callee, args, span } => {
                if span.contains(&offset) {
                    *found = Some((callee.as_ref(), args.as_slice(), span.clone()));
                }
                for arg in args {
                    Self::find_call_in_expr(arg, offset, found);
                    if found.is_some() {
                        return;
                    }
                }
                Self::find_call_in_expr(callee, offset, found);
            }
            Expr::Unary { expr, .. } => {
                Self::find_call_in_expr(expr, offset, found);
            }
            Expr::Binary { left, right, .. } => {
                Self::find_call_in_expr(left, offset, found);
                if found.is_none() {
                    Self::find_call_in_expr(right, offset, found);
                }
            }
            Expr::Field { receiver, .. } => {
                Self::find_call_in_expr(receiver, offset, found);
            }
            Expr::Index { receiver, index, .. } => {
                Self::find_call_in_expr(receiver, offset, found);
                if found.is_none() {
                    Self::find_call_in_expr(index, offset, found);
                }
            }
            Expr::Paren { expr, .. } => {
                Self::find_call_in_expr(expr, offset, found);
            }
            Expr::Block(b) => {
                Self::find_call_in_block(b, offset, found);
            }
            Expr::Lambda(l) => {
                Self::find_call_in_block(&l.body, offset, found);
            }
            Expr::If { condition, then_branch, else_branch, .. } => {
                Self::find_call_in_expr(condition, offset, found);
                if found.is_none() {
                    Self::find_call_in_block(then_branch, offset, found);
                }
                if found.is_none() {
                    if let Some(eb) = else_branch {
                        Self::find_call_in_block(eb, offset, found);
                    }
                }
            }
            Expr::Match { scrutinee, arms, .. } => {
                Self::find_call_in_expr(scrutinee, offset, found);
                if found.is_none() {
                    for arm in arms {
                        Self::find_call_in_expr(&arm.body, offset, found);
                        if found.is_some() {
                            return;
                        }
                    }
                }
            }
            Expr::Loop { condition, body, .. } => {
                if let Some(cond) = condition {
                    Self::find_call_in_expr(cond, offset, found);
                }
                if found.is_none() {
                    Self::find_call_in_block(body, offset, found);
                }
            }
            Expr::Return { expr, .. } => {
                if let Some(e) = expr {
                    Self::find_call_in_expr(e, offset, found);
                }
            }
            Expr::Break { expr, .. } => {
                if let Some(e) = expr {
                    Self::find_call_in_expr(e, offset, found);
                }
            }
            Expr::Yield { expr, .. } => {
                if let Some(e) = expr {
                    Self::find_call_in_expr(e, offset, found);
                }
            }
            Expr::Raise { expr, .. } => {
                Self::find_call_in_expr(expr, offset, found);
            }
            Expr::Catch { expr, arms, .. } => {
                Self::find_call_in_expr(expr, offset, found);
                if found.is_none() {
                    for arm in arms {
                        Self::find_call_in_expr(&arm.body, offset, found);
                        if found.is_some() {
                            return;
                        }
                    }
                }
            }
            Expr::Object { callee, fields, .. } => {
                Self::find_call_in_expr(callee, offset, found);
                if found.is_none() {
                    for (_, value) in fields {
                        if let Some(v) = value {
                            Self::find_call_in_expr(v, offset, found);
                            if found.is_some() {
                                return;
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
}
