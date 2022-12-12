use chomsky_types::Loc;
use chomsky_uir::egraph::Id;
use chomsky_uir::intent::IKun;
use nyar_types::NyarError;
use oak_r::ast::{Expr, RRoot, Statement};
use oak_r::kind::RSyntaxKind;

pub struct TranslatorContext<'a, A: chomsky_uir::Analysis<IKun>> {
    pub builder: chomsky_uir::builder::IntentBuilder<'a, A>,
}

impl<'a, A: chomsky_uir::Analysis<IKun>> TranslatorContext<'a, A> {
    pub fn new_with_builder(builder: chomsky_uir::builder::IntentBuilder<'a, A>) -> Self {
        Self { builder }
    }
}

#[derive(Default)]
pub struct NyarTranslator {}

impl NyarTranslator {
    pub fn new() -> Self {
        Self {}
    }

    fn to_loc(&self, span: &core::range::Range<usize>) -> Loc {
        Loc::new(0, span.start as u32, span.end as u32)
    }

    pub fn translate_root<A: chomsky_uir::Analysis<IKun>>(
        &self,
        root: &RRoot,
        ctx: &mut TranslatorContext<'_, A>,
    ) -> Result<Id, NyarError> {
        let mut items = Vec::new();
        for stmt in &root.statements {
            items.push(self.translate_statement(stmt, ctx)?);
        }
        Ok(ctx.builder.module("main", items, Loc::default()))
    }

    fn translate_statement<A: chomsky_uir::Analysis<IKun>>(
        &self,
        stmt: &Statement,
        ctx: &mut TranslatorContext<'_, A>,
    ) -> Result<Id, NyarError> {
        match stmt {
            Statement::Assignment { name, expr, span } => {
                let val_id = self.translate_expr(expr, ctx)?;
                Ok(ctx.builder.assign(&name.name, val_id, self.to_loc(span)))
            }
            Statement::ExprStmt { expr, .. } => self.translate_expr(expr, ctx),
            Statement::FunctionDef {
                name,
                params,
                body,
                span,
            } => {
                let mut body_ids = Vec::new();
                for s in body {
                    body_ids.push(self.translate_statement(s, ctx)?);
                }
                let body_id = ctx.builder.block(body_ids, self.to_loc(span));
                let param_names: Vec<String> = params.iter().map(|p| p.name.clone()).collect();
                let lambda_id = ctx.builder.lambda(param_names, body_id, self.to_loc(span));
                Ok(ctx.builder.export(&name.name, lambda_id, self.to_loc(span)))
            }
        }
    }

    fn translate_expr<A: chomsky_uir::Analysis<IKun>>(
        &self,
        expr: &Expr,
        ctx: &mut TranslatorContext<'_, A>,
    ) -> Result<Id, NyarError> {
        match expr {
            Expr::Ident(id) => Ok(ctx.builder.symbol(&id.name, self.to_loc(&id.span))),
            Expr::Literal { value, span } => {
                if let Ok(i) = value.parse::<i64>() {
                    Ok(ctx.builder.constant(i, self.to_loc(span)))
                } else if let Ok(f) = value.parse::<f64>() {
                    Ok(ctx.builder.float(f, self.to_loc(span)))
                } else {
                    Ok(ctx.builder.string(value, self.to_loc(span)))
                }
            }
            Expr::Bool { value, span } => Ok(ctx.builder.bool(*value, self.to_loc(span))),
            Expr::Null { span } => Ok(ctx.builder.symbol("null", self.to_loc(span))),
            Expr::Call {
                callee,
                args,
                span,
            } => {
                let callee_id = self.translate_expr(callee, ctx)?;
                let mut arg_ids = Vec::new();
                for arg in args {
                    arg_ids.push(self.translate_expr(arg, ctx)?);
                }
                // Handle built-in functions
                if let Expr::Ident(id) = callee.as_ref() {
                    match id.name.as_str() {
                        "print" => {
                            return Ok(ctx.builder.cross_lang_call(
                                "nyar",
                                "std",
                                "print",
                                arg_ids,
                                self.to_loc(span),
                            ));
                        }
                        _ => {}
                    }
                }
                Ok(ctx.builder.call(callee_id, arg_ids, self.to_loc(span)))
            }
            Expr::Binary {
                left,
                op,
                right,
                span,
            } => {
                let l_id = self.translate_expr(left, ctx)?;
                let r_id = self.translate_expr(right, ctx)?;
                let op_name = match op {
                    RSyntaxKind::Plus => "add",
                    RSyntaxKind::Minus => "sub",
                    RSyntaxKind::Star => "mul",
                    RSyntaxKind::Slash => "div",
                    RSyntaxKind::EqualEqual => "eq",
                    RSyntaxKind::NotEqual => "ne",
                    RSyntaxKind::Less => "lt",
                    RSyntaxKind::LessEqual => "le",
                    RSyntaxKind::Greater => "gt",
                    RSyntaxKind::GreaterEqual => "ge",
                    _ => "unknown",
                };
                Ok(ctx.builder.binary_op(op_name, l_id, r_id, self.to_loc(span)))
            }
            Expr::Unary { op, expr, span } => {
                let expr_id = self.translate_expr(expr, ctx)?;
                let op_name = match op {
                    RSyntaxKind::Minus => "neg",
                    RSyntaxKind::Plus => "pos",
                    RSyntaxKind::Not => "not",
                    _ => "unknown",
                };
                Ok(ctx
                    .builder
                    .extension(op_name, vec![expr_id], self.to_loc(span)))
            }
        }
    }
}
