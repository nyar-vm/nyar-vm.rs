use chomsky_uir::{EGraph, IKun, Id};
use chomsky_cost::DefaultCostModel;
use chomsky_extract::IKunExtractor;
use nyar_types::NyarError;
use nyar_vm::vm::NyarVM;
use nyar_vm::vm::traits::RuntimeProvider;

#[derive(Debug)]
pub struct RustyTypeScriptRuntime {
    vm: NyarVM,
}

impl RustyTypeScriptRuntime {
    pub fn new() -> Self {
        Self {
            vm: NyarVM::new(),
        }
    }

    pub fn execute(&mut self, intent_graph: (EGraph<IKun, ()>, Id)) -> Result<(), NyarError> {
        let (egraph, root_id) = intent_graph;

        // 1. Extract the best tree using the default cost model
        let cost_model = DefaultCostModel::default();
        let extractor = IKunExtractor::new(&egraph, cost_model);
        let tree = extractor.extract(root_id);

        // 2. Translate IKunTree to Nyar Module
        use nyar_vm::NyarBackend;
        let mut backend = NyarBackend::new();
        let module = backend.compile(&tree)?;

        // 3. Execute using Nyar VM
        println!("Executing Nyar Module: {:?}", module.exports);

        let module_idx = self.vm.load_module(module);

        // Find main or first export
        let entry_info = if let Some(module_ref) = self.vm.env.modules.get(&module_idx) {
            module_ref
                .exports
                .iter()
                .find(|e| e.symbol == "main".into())
                .or(module_ref.exports.first())
                .map(|e| (e.chunk_idx as usize, e.symbol.clone()))
        } else {
            None
        };

        if let Some((chunk_idx, _symbol)) = entry_info {
            match self.vm.execute(module_idx, chunk_idx) {
                Ok(val) => {
                    println!("Execution result: {}", val);
                    Ok(())
                }
                Err(e) => {
                    println!("Nyar VM execution failed: {:?}", e);
                    Err(e)
                }
            }
        } else {
            Err(NyarError::RuntimeError("No entry point found in module".to_string()))
        }
    }
}

impl RuntimeProvider for RustyTypeScriptRuntime {
    fn resolve(&self, _name: &str) -> Option<nyar_vm::vm::traits::ExternFunc> {
        None
    }

    fn get_intrinsic(&self, _id: u32) -> Option<nyar_vm::vm::traits::ExternFunc> {
        None
    }
}
