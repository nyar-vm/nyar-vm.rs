use chomsky_uir::intent::CrossLanguageCall;
use chomsky_uir::{IKun, Id};
use nyar_types::{NyarContext, NyarError, NyarFrontend};
use oak_core::{Builder, SourceText};
use oak_tcl::ast::{TclCommand, TclItem, TclRoot, TclWord};
use oak_tcl::{TclBuilder, TclLanguage};
use oak_vfs::Vfs;

pub struct RustyTclFrontend {
    language: TclLanguage,
}

impl RustyTclFrontend {
    pub fn new() -> Self {
        Self {
            language: TclLanguage::default(),
        }
    }

    fn lower_item<V: Vfs>(&self, item: &TclItem, ctx: &mut NyarContext<V>) -> Id {
        match item {
            TclItem::Command(cmd) => self.lower_command(cmd, ctx),
            TclItem::Comment(_) => {
                // Ignore comments for now or lower to something neutral
                ctx.egraph.add(IKun::Seq(vec![]))
            }
        }
    }

    fn lower_command<V: Vfs>(&self, cmd: &TclCommand, ctx: &mut NyarContext<V>) -> Id {
        let words: Vec<Id> = cmd.words.iter().map(|w| self.lower_word(w, ctx)).collect();
        // Skip commands that result in no meaningful words (e.g. only whitespace or empty words)
        if words.is_empty() {
            return ctx.egraph.add(IKun::Seq(vec![]));
        }

        // Check if it's a "set" command for state update
        if let Some(TclWord::Simple(name)) = cmd.words.first() {
            let name = name.trim();
            if name.is_empty() {
                return ctx.egraph.add(IKun::Seq(vec![]));
            }
            
            if name == "set" && cmd.words.len() == 3 {
                // Tcl: set varName value
                // In IKun, StateUpdate expects target to be a symbol.
                // We should pass the symbol directly if possible.
                if let TclWord::Simple(var_name) = &cmd.words[1] {
                    let target = ctx.egraph.add(IKun::Symbol(var_name.trim().to_string()));
                    let value = self.lower_word(&cmd.words[2], ctx);
                    return ctx.egraph.add(IKun::StateUpdate(target, value));
                }
            }
            if name == "puts" {
                // Map puts to nyar:io:println
                let args = if words.len() > 1 { words[1..].to_vec() } else { vec![] };
                return ctx.egraph.add(IKun::CrossLangCall(CrossLanguageCall {
                    language: "nyar".to_string(),
                    module_path: "io".to_string(),
                    function_name: "println".to_string(),
                    arguments: args,
                }));
            }
        }

        let mut words_iter = words.into_iter();
        let callee = words_iter.next().unwrap();
        
        // If the callee is an empty symbol, it's likely a parsing artifact or trailing whitespace command
        let callee_node = ctx.egraph.get_class(callee).nodes.first().cloned();
        if let Some(IKun::Symbol(ref s)) = callee_node {
            if s.trim().is_empty() {
                return ctx.egraph.add(IKun::Seq(vec![]));
            }
        }

        let args = words_iter.collect();
        ctx.egraph.add(IKun::Apply(callee, args))
    }

    fn lower_word<V: Vfs>(&self, word: &TclWord, ctx: &mut NyarContext<V>) -> Id {
        match word {
            TclWord::Simple(s) => {
                // In Tcl, simple words are often strings, but can be command names (symbols)
                // For now, let's treat them as symbols if they don't look like numbers
                if let Ok(i) = s.parse::<i64>() {
                    ctx.egraph.add(IKun::Constant(i))
                } else {
                    ctx.egraph.add(IKun::Symbol(s.clone()))
                }
            }
            TclWord::Variable(v) => ctx.egraph.add(IKun::Symbol(v.clone())),
            TclWord::Script(root) => self.lower_unified(root, ctx),
            TclWord::Braced(s) => ctx.egraph.add(IKun::StringConstant(s.clone())),
        }
    }
}

impl Default for RustyTclFrontend {
    fn default() -> Self {
        Self::new()
    }
}

impl NyarFrontend for RustyTclFrontend {
    type Language = TclLanguage;

    fn parse(&self, source: &str) -> Result<TclRoot, NyarError> {
        let builder = TclBuilder::new(&self.language);
        let source_text = SourceText::new(source);
        let mut session = oak_core::parser::ParseSession::<TclLanguage>::default();

        let output = builder.build(&source_text, &[], &mut session);
        output.result.map_err(|e| NyarError::Parse(format!("{:?}", e)))
    }

    fn lower_unified<V: Vfs>(&self, ast: &TclRoot, ctx: &mut NyarContext<V>) -> Id {
        let items: Vec<Id> = ast.items.iter().map(|item| self.lower_item(item, ctx)).collect();
        // Remove empty/no-op items that might result in empty symbols or unwanted applies
        let items: Vec<Id> = items.into_iter().filter(|&id| {
            let node = ctx.egraph.get_class(id).nodes.first().cloned();
            !matches!(node, Some(IKun::Seq(ref v)) if v.is_empty())
        }).collect();
        ctx.egraph.add(IKun::Module("main".to_string(), items))
    }
}
