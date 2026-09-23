use crate::{
    types::{
        Identifier, SourceSpan,
        hir::{HirExpr, HirExprKind, HirFunction, HirModule, HirPattern, HirStatement, HirStatementKind},
    },
    valkyrie::hir::PatternRefutability,
};
use std_data::text::valkyrie::ParseError;

pub fn check_pattern_refutability(module: &HirModule) -> Result<(), ParseError> {
    for function in &module.functions {
        let mut checker = PatternRefutabilityChecker::new();
        checker.check_function(function)?;
    }
    for struct_def in &module.structs {
        for method in &struct_def.methods {
            let mut checker = PatternRefutabilityChecker::new();
            checker.check_function(method)?;
        }
        for property in &struct_def.properties {
            if let Some(getter) = &property.getter {
                let mut checker = PatternRefutabilityChecker::new();
                checker.check_function(getter)?;
            }
            if let Some(setter) = &property.setter {
                let mut checker = PatternRefutabilityChecker::new();
                checker.check_function(setter)?;
            }
        }
    }
    for trait_def in &module.traits {
        for method in &trait_def.methods {
            let mut checker = PatternRefutabilityChecker::new();
            checker.check_function(method)?;
        }
        for method in &trait_def.default_methods {
            let mut checker = PatternRefutabilityChecker::new();
            checker.check_function(method)?;
        }
    }
    for impl_block in &module.impls {
        for method in &impl_block.methods {
            let mut checker = PatternRefutabilityChecker::new();
            checker.check_function(method)?;
        }
    }
    Ok(())
}

struct PatternRefutabilityChecker {
    errors: Vec<ParseError>,
}

impl PatternRefutabilityChecker {
    fn new() -> Self {
        Self { errors: Vec::new() }
    }

    fn check_function(&mut self, function: &HirFunction) -> Result<(), ParseError> {
        self.check_block(&function.body, true)?;
        Ok(())
    }

    fn check_block(&mut self, block: &crate::types::hir::HirBlock, _is_function_body: bool) -> Result<(), ParseError> {
        for statement in &block.statements {
            self.check_statement(statement)?;
        }
        if let Some(expr) = &block.expr {
            self.check_expr(expr)?;
        }
        Ok(())
    }

    fn check_statement(&mut self, statement: &HirStatement) -> Result<(), ParseError> {
        match &statement.kind {
            HirStatementKind::Let { pattern, initializer, .. } => {
                if let Some(init) = initializer {
                    self.check_expr(init)?;
                }
                self.check_let_pattern(pattern)?;
            }
            HirStatementKind::Expr(expr) => {
                self.check_expr(expr)?;
            }
        }
        Ok(())
    }

    fn check_let_pattern(&mut self, pattern: &HirPattern) -> Result<(), ParseError> {
        match pattern.refutability() {
            PatternRefutability::Irrefutable => Ok(()),
            PatternRefutability::Refutable | PatternRefutability::Unknown => {
                self.errors.push(ParseError::invalid(format!(
                    "refutable pattern not allowed in `let` binding; use `if let` or `case` instead (pattern: {})",
                    self.pattern_to_string(pattern)
                )));
                Err(ParseError::invalid("refutable pattern in let binding"))
            }
        }
    }

    fn check_expr(&mut self, expr: &HirExpr) -> Result<(), ParseError> {
        match &expr.kind {
            HirExprKind::Block(block) => self.check_block(block, false)?,
            HirExprKind::If { condition, then_branch, else_branch } => {
                self.check_expr(condition)?;
                self.check_block(then_branch, false)?;
                if let Some(else_branch) = else_branch {
                    self.check_block(else_branch, false)?;
                }
            }
            HirExprKind::IfLet { pattern, scrutinee, then_branch, else_branch, .. } => {
                self.check_expr(scrutinee)?;
                self.check_block(then_branch, false)?;
                if let Some(else_branch) = else_branch {
                    self.check_block(else_branch, false)?;
                }
            }
            HirExprKind::Match { scrutinee, arms } => {
                self.check_expr(scrutinee)?;
                for arm in arms {
                    self.check_expr(&arm.body)?;
                    if let Some(guard) = &arm.guard {
                        self.check_expr(guard)?;
                    }
                }
            }
            HirExprKind::Case { scrutinee, arms } => {
                self.check_expr(scrutinee)?;
                for arm in arms {
                    self.check_expr(&arm.body)?;
                    if let Some(guard) = &arm.guard {
                        self.check_expr(guard)?;
                    }
                }
            }
            HirExprKind::Loop { iterator, condition, body, .. } => {
                if let Some(iter) = iterator {
                    self.check_expr(iter)?;
                }
                if let Some(cond) = condition {
                    self.check_expr(cond)?;
                }
                self.check_block(body, false)?;
            }
            HirExprKind::Call { callee, args, .. } => {
                self.check_expr(callee)?;
                for arg in args {
                    self.check_expr(&arg.value)?;
                }
            }
            HirExprKind::Construct { args, .. } | HirExprKind::ArrayLiteral { items: args } => {
                for arg in args {
                    self.check_expr(arg)?;
                }
            }
            HirExprKind::Lambda { body, .. } => {
                self.check_block(body, false)?;
            }
            HirExprKind::TryScope { body, .. } => {
                self.check_block(body, false)?;
            }
            HirExprKind::Catch { expr, arms } => {
                self.check_expr(expr)?;
                for arm in arms {
                    self.check_expr(&arm.body)?;
                    if let Some(guard) = &arm.guard {
                        self.check_expr(guard)?;
                    }
                }
            }
            HirExprKind::With { base, updates } => {
                self.check_expr(base)?;
                for (_, value) in updates {
                    self.check_expr(value)?;
                }
            }
            HirExprKind::FieldInit { value, .. }
            | HirExprKind::Await(value)
            | HirExprKind::Awake(value)
            | HirExprKind::BlockOn(value)
            | HirExprKind::YieldFrom(value)
            | HirExprKind::TryPropagate(value)
            | HirExprKind::Raise(value)
            | HirExprKind::Resume(value)
            | HirExprKind::ArrayNew { length: value, .. }
            | HirExprKind::FieldAccess { object: value, .. }
            | HirExprKind::StoreField { value, .. }
            | HirExprKind::GenericApply { callee: value, .. }
            | HirExprKind::Assign { value, .. }
            | HirExprKind::Return(Some(value))
            | HirExprKind::Yield(Some(value))
            | HirExprKind::Break { expr: Some(value), .. } => {
                self.check_expr(value)?;
            }
            _ => {}
        }
        Ok(())
    }

    fn pattern_to_string(&self, pattern: &HirPattern) -> String {
        match pattern {
            HirPattern::Wildcard => "_".to_string(),
            HirPattern::Variable(id) => id.name.to_string(),
            HirPattern::Tuple(items) => format!("({})", items.iter().map(|p| self.pattern_to_string(p)).collect::<Vec<_>>().join(", ")),
            HirPattern::Literal(lit) => format!("{:?}", lit),
            HirPattern::Range { start, end, inclusive_end } => {
                format!(
                    "{}..{}{}",
                    start.as_ref().map(|s| format!("{:?}", s)).unwrap_or("".to_string()),
                    end.as_ref().map(|e| format!("{:?}", e)).unwrap_or("".to_string()),
                    if *inclusive_end { "=" } else { "" }
                )
            }
            HirPattern::Extractor(ext) => match ext {
                crate::types::hir::HirExtractorPattern::Constructor { name, fields, .. } => {
                    format!("{} ({})", name, fields.iter().map(|p| self.pattern_to_string(p)).collect::<Vec<_>>().join(", "))
                }
                crate::types::hir::HirExtractorPattern::Array { prefix, suffix, .. } => {
                    format!(
                        "[{}..{}]",
                        prefix.iter().map(|p| self.pattern_to_string(p)).collect::<Vec<_>>().join(", "),
                        suffix.iter().map(|p| self.pattern_to_string(p)).collect::<Vec<_>>().join(", ")
                    )
                }
            },
            HirPattern::Or(items) => items.iter().map(|p| self.pattern_to_string(p)).collect::<Vec<_>>().join(" | "),
            HirPattern::Name(path) => path.to_string(),
            HirPattern::Type(path) => format!("{} :: type", path),
            HirPattern::TypedBind { identifier, ty } => format!("{} : {}", identifier.name, ty),
            HirPattern::Object { name, fields, rest } => {
                let field_strs = fields.iter().map(|(n, p)| format!("{}: {}", n, self.pattern_to_string(p))).collect::<Vec<_>>().join(", ");
                let rest_str = rest.as_ref().map(|r| format!("..{}", r.name)).unwrap_or_default();
                let all = if field_strs.is_empty() {
                    rest_str
                }
                else if rest_str.is_empty() {
                    field_strs
                }
                else {
                    format!("{}, {}", field_strs, rest_str)
                };
                format!("{} {{ {} }}", name.as_ref().map(|n| n.to_string()).unwrap_or("".to_string()), all)
            }
            HirPattern::Else => "else".to_string(),
            HirPattern::Bind { identifier, pattern } => format!("{} <- {}", identifier.name, self.pattern_to_string(pattern)),
            HirPattern::Mut(p) => format!("mut {}", self.pattern_to_string(p)),
            HirPattern::Pin { mutable, pattern } => format!("pin{}{}", if *mutable { " mut" } else { "" }, self.pattern_to_string(pattern)),
        }
    }
}
