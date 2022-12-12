use nyar_types::{NyarContext, NyarError, NyarFrontend, Id, Vfs, Loc};
use oak_cobol::{CobolLanguage, CobolBuilder, ast::CobolRoot};
use oak_core::{Builder, SourceText};

pub struct RustyCobolFrontend {
    language: CobolLanguage,
}

impl RustyCobolFrontend {
    pub fn new() -> Self {
        Self {
            language: CobolLanguage::default(),
        }
    }
}

impl Default for RustyCobolFrontend {
    fn default() -> Self {
        Self::new()
    }
}

impl NyarFrontend<(), CobolRoot> for RustyCobolFrontend {
    type Language = CobolLanguage;

    fn parse(&self, source: &str) -> Result<CobolRoot, NyarError> {
        let builder = CobolBuilder::new(&self.language);
        let source_text = SourceText::new(source);
        let mut session = oak_core::parser::ParseSession::<CobolLanguage>::default();

        let output = builder.build(&source_text, &[], &mut session);
        output
            .result
            .map_err(|e| NyarError::Parse(format!("{:?}", e)))
    }

    fn lower_unified<V: Vfs>(&self, _ast: &CobolRoot, _ctx: &mut NyarContext<'_, V>) -> Id {
        // let mut lowerer = CobolLowerer::new(ctx);
        // lowerer.lower_root(ast)
        0
    }
}

struct CobolLowerer<'a, 'b, V: Vfs, A: chomsky_uir::Analysis<chomsky_uir::IKun>> {
    ctx: &'a mut NyarContext<'b, V, A>,
}

impl<'a, 'b, V: Vfs, A: chomsky_uir::Analysis<chomsky_uir::IKun>> CobolLowerer<'a, 'b, V, A> {
    fn new(ctx: &'a mut NyarContext<'b, V, A>) -> Self {
        Self { ctx }
    }

    fn lower_root(&mut self, root: &CobolRoot) -> Id {
        let mut items = Vec::new();
        
        if let Some(proc_div) = &root.program.procedure_division {
            self.lower_procedure_division(proc_div, &mut items);
        }
        
        let mut builder = self.ctx.builder();
        builder.module("rusty-cobol-program", items, Loc::default())
    }

    fn lower_procedure_division(&mut self, proc_div: &oak_cobol::ast::ProcedureDivision, items: &mut Vec<Id>) {
        let mut stmts = Vec::new();
        
        for stmt in &proc_div.statements {
            let mut builder = self.ctx.builder();
            match stmt {
                oak_cobol::ast::Statement::Display(display) => {
                    let mut args = Vec::new();
                    for arg in &display.items {
                        args.push(builder.string(arg, Loc::default()));
                    }
                    stmts.push(builder.extension("display", args, Loc::default()));
                }
                oak_cobol::ast::Statement::Stop(stop) => {
                    if stop.run {
                        stmts.push(builder.extension("stop", Vec::new(), Loc::default()));
                    }
                }
                oak_cobol::ast::Statement::Move(move_stmt) => {
                    let mut args = Vec::new();
                    args.push(builder.string(&move_stmt.source, Loc::default()));
                    for target in &move_stmt.targets {
                        args.push(builder.string(target, Loc::default()));
                    }
                    stmts.push(builder.extension("move", args, Loc::default()));
                }
                _ => {}
            }
        }
        
        let mut builder = self.ctx.builder();
        let body = builder.block(stmts, Loc::default());
        let main_func = builder.lambda(Vec::new(), body, Loc::default());
        let export = builder.export("main", main_func, Loc::default());
        items.push(export);
    }
}
