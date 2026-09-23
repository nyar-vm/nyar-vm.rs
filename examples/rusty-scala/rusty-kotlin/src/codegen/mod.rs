//! Kotlin 到 Nyar 字节码的翻译器

use chomsky_cost::DefaultCostModel;
use chomsky_extract::IKunExtractor;
use chomsky_uir::{ConstraintAnalysis, EGraph, IKun, IKunTree, Id, IntentBuilder};
use nyar_types::{NyarError, Loc};
use oak_kotlin::ast::*;
use oak_kotlin::kind::KotlinSyntaxKind;
use oak_kotlin::language::KotlinLanguage;

pub struct NyarTranslator;

impl NyarTranslator {
    pub fn new() -> Self {
        Self
    }

    pub fn translate_to_tree(&self, root: &KotlinRoot) -> Result<IKunTree, NyarError> {
        let mut egraph = EGraph::<IKun, ConstraintAnalysis>::new();
        let mut builder = IntentBuilder::new(&mut egraph);
        let root_id = self.translate_to_id(root, &mut builder)?;
        let extractor = IKunExtractor::new(&egraph, DefaultCostModel);
        Ok(extractor.extract(root_id))
    }

    pub fn translate_to_id(
        &self,
        root: &KotlinRoot,
        builder: &mut IntentBuilder<ConstraintAnalysis>,
    ) -> Result<Id, NyarError> {
        let mut members = Vec::new();

        for decl in &root.declarations {
            members.push(self.translate_declaration(decl, builder)?);
        }

        let root_id = builder.seq(members, Loc::default());
        Ok(root_id)
    }

    fn translate_declaration(
        &self,
        decl: &Declaration,
        builder: &mut IntentBuilder<ConstraintAnalysis>,
    ) -> Result<Id, NyarError> {
        let loc = Loc::default();
        match decl {
            Declaration::Class { name, members, .. } => {
                let mut class_members = Vec::new();
                for member in members {
                    class_members.push(self.translate_declaration(member, builder)?);
                }
                let name_id = builder.string(name, loc);
                let members_seq = builder.seq(class_members, loc);
                Ok(builder.extension("class", vec![name_id, members_seq], loc))
            }
            Declaration::Function {
                name, params, body, ..
            } => {
                let mut param_ids = Vec::new();
                for param in params {
                    let p_name = builder.string(&param.name, loc);
                    let p_type = builder.string(param.type_name.as_deref().unwrap_or("Any"), loc);
                    param_ids.push(builder.extension("parameter", vec![p_name, p_type], loc));
                }
                let params_seq = builder.seq(param_ids, loc);

                let mut stmt_ids = Vec::new();
                for stmt in body {
                    stmt_ids.push(self.translate_statement(stmt, builder)?);
                }
                let body_seq = builder.seq(stmt_ids, loc);

                let name_id = builder.string(name, loc);
                let ret_type = builder.string("void", loc); // TODO: proper return type
                Ok(builder.extension("method", vec![name_id, ret_type, params_seq, body_seq], loc))
            }
            Declaration::Variable { name, is_val, .. } => {
                let name_id = builder.string(name, loc);
                let is_val_id = builder.bool(*is_val, loc);
                Ok(builder.extension("variable", vec![name_id, is_val_id], loc))
            }
        }
    }

    fn translate_statement(
        &self,
        stmt: &Statement,
        builder: &mut IntentBuilder<ConstraintAnalysis>,
    ) -> Result<Id, NyarError> {
        let loc = Loc::default();
        match stmt {
            Statement::Return(expr) => {
                let expr_id = if let Some(e) = expr {
                    builder.string(e, loc) // TODO: parse expression
                } else {
                    builder.seq(vec![], loc)
                };
                Ok(builder.extension("return", vec![expr_id], loc))
            }
            Statement::Expression(expr) => {
                Ok(builder.string(expr, loc)) // TODO: proper expression translation
            }
            Statement::Variable { name, is_val } => {
                let name_id = builder.string(name, loc);
                let is_val_id = builder.bool(*is_val, loc);
                Ok(builder.extension("variable", vec![name_id, is_val_id], loc))
            }
            Statement::Assignment { target, value } => {
                let target_id = builder.string(target, loc);
                let value_id = builder.string(value, loc);
                Ok(builder.extension("assign", vec![target_id, value_id], loc))
            }
        }
    }
}
