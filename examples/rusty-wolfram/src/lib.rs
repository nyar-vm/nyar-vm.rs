use nyar_types::{Id, Loc, NyarContext, NyarError, NyarFrontend, Vfs};
use oak_core::tree::TypedNode;
use oak_core::{Builder, Source, SourceText};
use oak_wolfram::{ast::WolframRoot, WolframBuilder, WolframLanguage};

pub struct RustyWolframFrontend {
    language: WolframLanguage,
}

impl RustyWolframFrontend {
    pub fn new() -> Self {
        Self {
            language: WolframLanguage::default(),
        }
    }
}

impl Default for RustyWolframFrontend {
    fn default() -> Self {
        Self::new()
    }
}

impl NyarFrontend for RustyWolframFrontend {
    type Language = WolframLanguage;

    fn parse(&self, source: &str) -> Result<WolframRoot, NyarError> {
        let builder = WolframBuilder::new(&self.language);
        let source_text = SourceText::new(source);
        let mut session = oak_core::parser::ParseSession::<WolframLanguage>::default();

        let output = builder.build(&source_text, &[], &mut session);
        output
            .result
            .map_err(|e| NyarError::Parse(format!("{:?}", e)))
    }

    fn lower_unified<V: Vfs, A: chomsky_uir::Analysis<IKun>>(&self, ast: &WolframRoot, ctx: &mut NyarContext<V, A>) -> Id
    where
        A::Data: chomsky_uir::egraph::HasDebugInfo,
    {
        let uri = ctx.vfs.get_uri(ctx.source_id.into()).unwrap_or_else(|| oak_core::Arc::from("anonymous"));
        let source = ctx.vfs.get_source(&uri).expect("Source not found");
        let mut stmts = Vec::new();
        let mut offset = 0;
        for tree in ast.green().children() {
            if let Some(expr_id) = self.lower_node(tree, ctx, &source, offset) {
                stmts.push(expr_id);
            }
            offset += tree.len() as usize;
        }
        ctx.builder().module("rusty-wolfram-program", stmts, Loc::default())
    }
}

impl RustyWolframFrontend {
    fn lower_node<V: Vfs, A: chomsky_uir::Analysis<IKun>>(
        &self,
        tree: &oak_core::tree::GreenTree<WolframLanguage>,
        ctx: &mut NyarContext<V, A>,
        source: &V::Source,
        offset: usize,
    ) -> Option<Id>
    where
        A::Data: chomsky_uir::egraph::HasDebugInfo,
    {
        use oak_wolfram::ast::*;
        let node = tree.as_node()?;
        let red = oak_core::tree::RedNode::new(node, offset);

        if let Some(symbol) = WolframSymbol::cast(red.clone()) {
            let name = source.get_text_in(red.span()).to_string();
            // Handle slots specifically if needed, or just treat them as symbols
            return Some(ctx.builder().symbol(&name, Loc::default()));
        }

        if let Some(literal) = WolframLiteral::cast(red.clone()) {
            let text = source.get_text_in(red.span());
            if let Ok(val) = text.parse::<i64>() {
                return Some(ctx.builder().int(val, Loc::default()));
            }
            if let Ok(val) = text.parse::<f64>() {
                return Some(ctx.builder().float(val, Loc::default()));
            }
            // Strip quotes from string literals
            let content = if text.starts_with('"') && text.ends_with('"') {
                &text[1..text.len() - 1]
            } else {
                &text
            };
            return Some(ctx.builder().string(content, Loc::default()));
        }

        if let Some(expr) = WolframExpression::cast(red.clone()) {
            let mut current_offset = offset;
            for child in expr.green().children() {
                if let Some(id) = self.lower_node(child, ctx, source, current_offset) {
                    return Some(id);
                }
                current_offset += child.len() as usize;
            }
        }

        if let Some(call) = WolframCall::cast(red.clone()) {
            let mut children = call.green().children().iter();
            let mut current_offset = offset;
            if let Some(head_tree) = children.next() {
                let head_id = self.lower_node(head_tree, ctx, source, current_offset)?;
                current_offset += head_tree.len() as usize;
                let mut args = Vec::new();
                for next_tree in children {
                    if let Some(node) = next_tree.as_node() {
                        if let Some(wolfram_args) = WolframArguments::cast(oak_core::tree::RedNode::new(node, current_offset)) {
                            let mut arg_offset = current_offset;
                            for arg_tree in wolfram_args.green().children() {
                                if let Some(arg_id) = self.lower_node(arg_tree, ctx, source, arg_offset) {
                                    args.push(arg_id);
                                }
                                arg_offset += arg_tree.len() as usize;
                            }
                        }
                    }
                    current_offset += next_tree.len() as usize;
                }
                return Some(ctx.builder().call(head_id, args, Loc::default()));
            }
        }

        if let Some(binary) = WolframBinaryExpr::cast(red.clone()) {
            let mut children = binary.green().children().iter();
            let mut current_offset = offset;
            if let (Some(left_tree), Some(op_tree), Some(right_tree)) = (children.next(), children.next(), children.next()) {
                let left_id = self.lower_node(left_tree, ctx, source, current_offset)?;
                let left_len = left_tree.len() as usize;
                let op_offset = current_offset + left_len;
                let op_len = op_tree.len() as usize;
                let right_offset = op_offset + op_len;

                let right_id = self.lower_node(right_tree, ctx, source, right_offset)?;

                let op_text = match op_tree {
                    oak_core::tree::GreenTree::Node(n) => source.get_text_from(oak_core::tree::RedNode::new(n, op_offset).span()),
                    oak_core::tree::GreenTree::Leaf(l) => {
                        let span = oak_core::Range {
                            start: op_offset,
                            end: op_offset + l.length as usize,
                        };
                        source.get_text_from(span)
                    }
                };

                return match op_text.as_ref() {
                    "+" => Some(ctx.builder().add_op(left_id, right_id, Loc::default())),
                    "-" => Some(ctx.builder().sub_op(left_id, right_id, Loc::default())),
                    "*" => Some(ctx.builder().mul_op(left_id, right_id, Loc::default())),
                    "/" => Some(ctx.builder().div_op(left_id, right_id, Loc::default())),
                    "^" => Some(ctx.builder().binary_op("pow", left_id, right_id, Loc::default())),
                    "&&" => Some(ctx.builder().binary_op("and", left_id, right_id, Loc::default())),
                    "||" => Some(ctx.builder().binary_op("or", left_id, right_id, Loc::default())),
                    "==" => Some(ctx.builder().eq_op(left_id, right_id, Loc::default())),
                    "!=" => Some(ctx.builder().ne_op(left_id, right_id, Loc::default())),
                    "<" => Some(ctx.builder().lt_op(left_id, right_id, Loc::default())),
                    ">" => Some(ctx.builder().gt_op(left_id, right_id, Loc::default())),
                    "<=" => Some(ctx.builder().le_op(left_id, right_id, Loc::default())),
                    ">=" => Some(ctx.builder().ge_op(left_id, right_id, Loc::default())),
                    "->" | "\[Rule]" => Some(ctx.builder().extension("rule", vec![left_id, right_id], Loc::default())),
                    ":>" | "\[RuleDelayed]" => Some(ctx.builder().extension("rule_delayed", vec![left_id, right_id], Loc::default())),
                    "=" => Some(ctx.builder().assign_to_id(left_id, right_id, Loc::default())),
                    ":=" => Some(ctx.builder().extension("set_delayed", vec![left_id, right_id], Loc::default())),
                    "@" => Some(ctx.builder().call(left_id, vec![right_id], Loc::default())),
                    "/@" => Some(ctx.builder().map(left_id, right_id, Loc::default())),
                    "@@" => Some(ctx.builder().extension("apply", vec![left_id, right_id], Loc::default())),
                    "@@@" => Some(ctx.builder().extension("apply_level", vec![left_id, right_id], Loc::default())),
                    "//@" => Some(ctx.builder().extension("map_all", vec![left_id, right_id], Loc::default())),
                    "//" => Some(ctx.builder().call(right_id, vec![left_id], Loc::default())),
                    _ => Some(ctx.builder().binary_op(&op_text, left_id, right_id, Loc::default())),
                };
            }
        }

        if let Some(prefix) = WolframPrefixExpr::cast(red.clone()) {
            let mut children = prefix.green().children().iter();
            let mut current_offset = offset;
            if let (Some(op_tree), Some(arg_tree)) = (children.next(), children.next()) {
                let op_len = op_tree.len() as usize;
                let arg_id = self.lower_node(arg_tree, ctx, source, current_offset + op_len)?;
                let op_text = match op_tree {
                    oak_core::tree::GreenTree::Node(n) => source.get_text_from(oak_core::tree::RedNode::new(n, current_offset).span()),
                    oak_core::tree::GreenTree::Leaf(l) => {
                        let span = oak_core::Range {
                            start: current_offset,
                            end: current_offset + l.length as usize,
                        };
                        source.get_text_from(span)
                    }
                };
                return match op_text.as_ref() {
                    "-" => Some(ctx.builder().neg_op(arg_id, Loc::default())),
                    "!" => Some(ctx.builder().not_op(arg_id, Loc::default())),
                    _ => None,
                };
            }
        }

        if let Some(postfix) = WolframPostfixExpr::cast(red.clone()) {
            let mut children = postfix.green().children().iter();
            let mut current_offset = offset;
            if let (Some(arg_tree), Some(op_tree)) = (children.next(), children.next()) {
                let arg_id = self.lower_node(arg_tree, ctx, source, current_offset)?;
                let arg_len = arg_tree.len() as usize;
                let op_text = match op_tree {
                    oak_core::tree::GreenTree::Node(n) => source.get_text_from(oak_core::tree::RedNode::new(n, current_offset + arg_len).span()),
                    oak_core::tree::GreenTree::Leaf(l) => {
                        let span = oak_core::Range {
                            start: current_offset + arg_len,
                            end: current_offset + arg_len + l.length as usize,
                        };
                        source.get_text_from(span)
                    }
                };
                return match op_text.as_ref() {
                    "!" => Some(ctx.builder().extension("factorial", vec![arg_id], Loc::default())),
                    "&" => {
                        // body & -> pure function
                        // We need to extract parameters (#, ##, #1, #2...) from body
                        // For now, let's assume a simple case
                        Some(ctx.builder().lambda(vec!["#".to_string()], arg_id, Loc::default()))
                    }
                    _ => None,
                };
            }
        }

        if let Some(list) = WolframList::cast(red.clone()) {
            let mut items = Vec::new();
            let mut current_offset = offset;
            for item_tree in list.green().children() {
                if let Some(item_id) = self.lower_node(item_tree, ctx, source, current_offset) {
                    items.push(item_id);
                }
                current_offset += item_tree.len() as usize;
            }
            return Some(ctx.builder().extension("list", items, Loc::default()));
        }

        None
    }
}
