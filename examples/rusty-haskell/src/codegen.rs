//! Haskell 到 Nyar 字节码的翻译器

use chomsky_types::Loc;
use chomsky_uir::{ConstraintAnalysis, EGraph, IKun, IKunTree, IntentBuilder};
use nyar_types::{NyarError};
use oak_haskell::ast::*;

/// Haskell 翻译器上下文
pub struct TranslatorContext<'a, A: chomsky_uir::Analysis<IKun> = ()> {
    pub builder: IntentBuilder<'a, A>,
}

impl<'a, A: chomsky_uir::Analysis<IKun>> TranslatorContext<'a, A> {
    pub fn new(egraph: &'a mut EGraph<IKun, A>) -> Self {
        Self {
            builder: IntentBuilder::new(egraph),
        }
    }

    pub fn new_with_builder(builder: IntentBuilder<'a, A>) -> Self {
        Self { builder }
    }
}

pub struct NyarTranslator;

impl NyarTranslator {
    pub fn new() -> Self {
        Self
    }

    pub fn translate_to_graph(
        &self,
        ast: &HaskellRoot,
        egraph: &mut EGraph<IKun, ConstraintAnalysis>,
    ) -> Result<chomsky_uir::egraph::Id, NyarError> {
        let mut ctx = TranslatorContext::new(egraph);
        self.translate_root(ast, &mut ctx)
    }

    pub fn translate_to_tree(&self, ast: &HaskellRoot) -> Result<IKunTree, NyarError> {
        let mut egraph = EGraph::<IKun, ConstraintAnalysis>::new();
        let root_id = self.translate_to_graph(ast, &mut egraph)?;

        let extractor =
            chomsky_extract::IKunExtractor::new(&egraph, chomsky_cost::DEFAULT_COST_MODEL.clone());

        Ok(extractor.extract(root_id))
    }

    pub fn translate_root<A: chomsky_uir::Analysis<IKun>>(
        &self,
        root: &HaskellRoot,
        ctx: &mut TranslatorContext<'_, A>,
    ) -> Result<chomsky_uir::egraph::Id, NyarError> {
        let mut items = Vec::new();
        for item in &root.items {
            if let Some(id) = self.translate_item(item, ctx)? {
                items.push(id);
            }
        }
        let loc = Loc::default();
        if let Some(name) = &root.module_name {
            Ok(ctx.builder.module(&name.name, items, loc))
        } else {
            Ok(ctx.builder.seq(items, loc))
        }
    }

    pub fn translate_item<A: chomsky_uir::Analysis<IKun>>(
        &self,
        item: &Item,
        ctx: &mut TranslatorContext<'_, A>,
    ) -> Result<Option<chomsky_uir::egraph::Id>, NyarError> {
        match item {
            Item::Function(f) => Ok(Some(self.translate_function(f, ctx)?)),
            Item::Import(_) => Ok(None),
            Item::DataDeclaration(_) => Ok(None),
            Item::TypeAlias(_) => Ok(None),
        }
    }

    pub fn translate_function<A: chomsky_uir::Analysis<IKun>>(
        &self,
        f: &Function,
        ctx: &mut TranslatorContext<'_, A>,
    ) -> Result<chomsky_uir::egraph::Id, NyarError> {
        let loc = Loc::default();
        let mut equations = Vec::new();
        for eq in &f.equations {
            equations.push(self.translate_equation(eq, ctx)?);
        }

        if equations.len() == 1 {
            // If only one equation, we can try to extract its parameters
            let eq = &f.equations[0];
            let body = self.translate_expression(&eq.body, ctx)?;
            let mut params = Vec::new();
            for pat in &eq.patterns {
                match pat {
                    Pattern::Variable(id) => params.push(id.name.clone()),
                    _ => params.push("_".to_string()),
                }
            }
            Ok(ctx.builder.function(&f.name.name, params, vec![body], loc))
        } else {
            // Multiple equations - simplified as a sequence of anonymous functions for now
            let seq = ctx.builder.seq(equations, loc);
            Ok(ctx.builder.assign(&f.name.name, seq, loc))
        }
    }

    pub fn translate_equation<A: chomsky_uir::Analysis<IKun>>(
        &self,
        eq: &Equation,
        ctx: &mut TranslatorContext<'_, A>,
    ) -> Result<chomsky_uir::egraph::Id, NyarError> {
        let loc = Loc::default();
        let body = self.translate_expression(&eq.body, ctx)?;
        let mut params = Vec::new();
        for pat in &eq.patterns {
            match pat {
                Pattern::Variable(id) => params.push(id.name.clone()),
                _ => params.push("_".to_string()),
            }
        }
        Ok(ctx.builder.lambda(params, body, loc))
    }

    pub fn translate_expression<A: chomsky_uir::Analysis<IKun>>(
        &self,
        expr: &Expression,
        ctx: &mut TranslatorContext<'_, A>,
    ) -> Result<chomsky_uir::egraph::Id, NyarError> {
        let loc = Loc::default();
        match expr {
            Expression::Variable(id) => Ok(ctx.builder.symbol(&id.name, loc)),
            Expression::Literal(lit) => match lit {
                Literal::Integer(i) => Ok(ctx.builder.constant(*i, loc)),
                Literal::Float(f) => Ok(ctx.builder.float(*f, loc)),
                Literal::String(s) => Ok(ctx.builder.string(s, loc)),
                Literal::Char(c) => Ok(ctx.builder.constant(*c as i64, loc)),
            },
            Expression::Application(f, a) => {
                let func = self.translate_expression(f, ctx)?;
                let arg = self.translate_expression(a, ctx)?;
                Ok(ctx.builder.call(func, vec![arg], loc))
            }
            Expression::Lambda(pats, body) => {
                let body_id = self.translate_expression(body, ctx)?;
                let mut params = Vec::new();
                for pat in pats {
                    match pat {
                        Pattern::Variable(id) => params.push(id.name.clone()),
                        _ => params.push("_".to_string()),
                    }
                }
                Ok(ctx.builder.lambda(params, body_id, loc))
            }
            Expression::Let(items, body) => {
                let mut item_ids = Vec::new();
                for item in items {
                    if let Some(id) = self.translate_item(item, ctx)? {
                        item_ids.push(id);
                    }
                }
                let body_id = self.translate_expression(body, ctx)?;
                item_ids.push(body_id);
                Ok(ctx.builder.seq(item_ids, loc))
            }
            Expression::Case(expr, arms) => {
                let _cond = self.translate_expression(expr, ctx)?;
                let mut arm_ids = Vec::new();
                for arm in arms {
                    let body = self.translate_expression(&arm.body, ctx)?;
                    arm_ids.push(body);
                }
                Ok(ctx.builder.seq(arm_ids, loc))
            }
        }
    }
}
