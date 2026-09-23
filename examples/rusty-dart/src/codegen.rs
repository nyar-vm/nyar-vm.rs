//! Dart 代码生成实现

use nyar_types::{NyarContext, Loc};
use oak_vfs::Vfs;
use chomsky_uir::{Id, IKun};
use oak_dart::DartRoot;
use oak_dart::ast::Item;

pub struct UirConverter<'a, 'b, V: Vfs, A: chomsky_uir::Analysis<IKun> = ()> {
    ctx: &'a mut NyarContext<'b, V, A>,
}

impl<'a, 'b, V: Vfs, A: chomsky_uir::Analysis<IKun>> UirConverter<'a, 'b, V, A> {
    pub fn new(ctx: &'a mut NyarContext<'b, V, A>) -> Self {
        Self { ctx }
    }

    pub fn convert_root(&mut self, root: &DartRoot) -> Id {
        let mut items = Vec::new();
        for item in &root.items {
            if let Some(node) = self.convert_item(item) {
                items.push(node);
            }
        }
        self.ctx.builder().module("main", items, Loc::default())
    }

    fn convert_item(&mut self, item: &Item) -> Option<Id> {
        let loc = Loc::default();
        match item {
            Item::Class(class) => {
                let name = &class.name.name;
                // Basic class representation
                let name_id = self.ctx.builder().symbol(name, loc.clone());
                Some(self.ctx.builder().extension("class", vec![name_id], loc))
            }
            Item::Function(func) => {
                let name = &func.name.name;
                // Basic function representation
                let body = self.ctx.builder().seq(vec![], loc.clone());
                Some(self.ctx.builder().export(name, body, loc))
            }
            Item::Variable(var) => {
                let name = &var.name.name;
                let mangled = self.ctx.scopes.declare_variable(name);
                let value = self.ctx.builder().constant(0, loc.clone());
                Some(self.ctx.builder().assign(&mangled, value, loc))
            }
        }
    }
}
