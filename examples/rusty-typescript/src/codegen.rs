//! Nyar 指令生成器
//!
//! 将 UIR (Universal Intermediate Representation) 转换为 Nyar 字节码
//! 这里的实现将作为 Chomsky 的一部分或与其紧密集成

use std::collections::HashMap;

use chomsky_uir::{ConstraintAnalysis, EGraph, IKun, Id};
use nyar_types::NyarError;
use nyar_vm::bytecode::format::{Chunk, Constant, ExportInfo, ImportInfo, NyarModule};
use nyar_vm::bytecode::opcode::{I32Ext, Opcode};

/// Nyar 翻译器，将 UIR 转换为 Nyar 字节码
pub struct NyarTranslator {
    /// 局部变量映射
    locals: HashMap<String, u8>,
    /// 局部变量索引计数器
    local_index: u8,
    /// 常量池
    constants: Vec<Constant>,
    /// 标签计数器
    label_counter: u32,
    /// 当前正在生成的代码
    code: Vec<u8>,
    /// 标签位置映射 (标签名 -> 代码偏移)
    labels: HashMap<String, usize>,
    /// 重定位信息 (待修补的偏移 -> 目标标签名, 跳转指令大小)
    relocations: Vec<(usize, String, bool)>, // offset, label, is_conditional
    /// 已完成的 Chunk
    chunks: Vec<Chunk>,
}

/// 辅助函数：在 EGraph 中递归查找 Lambda 节点
fn find_lambda(
    egraph: &EGraph<IKun, ConstraintAnalysis>,
    class_id: Id,
) -> Option<(Vec<String>, Id)> {
    let class = egraph.get_class(class_id);
    for node in &class.nodes {
        match node {
            IKun::Lambda(params, body) => return Some((params.clone(), *body)),
            IKun::StateUpdate(_, val) => {
                if let Some(res) = find_lambda(egraph, *val) {
                    return Some(res);
                }
            }
            _ => {}
        }
    }
    None
}

impl NyarTranslator {
    /// 创建新的 Nyar 翻译器
    pub fn new() -> Self {
        Self {
            locals: HashMap::new(),
            local_index: 0,
            constants: Vec::new(),
            label_counter: 0,
            code: Vec::new(),
            labels: HashMap::new(),
            relocations: Vec::new(),
            chunks: Vec::new(),
        }
    }

    /// 发射单个字节
    fn emit_u8(&mut self, b: u8) {
        self.code.push(b);
    }

    /// 发射 u16 (小端序)
    fn emit_u16(&mut self, v: u16) {
        self.code.extend_from_slice(&v.to_le_bytes());
    }

    /// 发射 i16 (小端序)
    fn emit_i16(&mut self, v: i16) {
        self.code.extend_from_slice(&v.to_le_bytes());
    }

    /// 发射 i32 (小端序)
    fn emit_i32(&mut self, v: i32) {
        self.code.extend_from_slice(&v.to_le_bytes());
    }

    /// 定义标签
    fn define_label(&mut self, name: &str) {
        self.labels.insert(name.to_string(), self.code.len());
    }

    /// 记录重定位
    fn emit_jump(&mut self, label: &str, is_conditional: bool) {
        let op = if is_conditional {
            Opcode::JumpIfFalse
        } else {
            Opcode::Jump
        };
        self.emit_u8(op as u8);
        let offset = self.code.len();
        self.emit_i16(0); // 占位符
        self.relocations
            .push((offset, label.to_string(), is_conditional));
    }

    /// 修补所有跳转偏移
    fn patch_jumps(&mut self) {
        for (offset, label, _) in std::mem::take(&mut self.relocations) {
            if let Some(&target) = self.labels.get(&label) {
                let jump_offset = (target as isize - (offset + 2) as isize) as i16;
                let bytes = jump_offset.to_le_bytes();
                self.code[offset] = bytes[0];
                self.code[offset + 1] = bytes[1];
            }
        }
    }

    /// 生成一个新的唯一标签
    fn new_label(&mut self, prefix: &str) -> String {
        let label = format!("{}_{}", prefix, self.label_counter);
        self.label_counter += 1;
        label
    }

    /// 重置翻译器状态（用于生成新函数）
    fn reset_for_function(&mut self) {
        self.locals.clear();
        self.local_index = 0;
        self.code.clear();
        self.labels.clear();
        self.relocations.clear();
        self.label_counter = 0;
    }

    /// 生成 NyarModule
    pub fn generate(
        &mut self,
        egraph: &EGraph<IKun, ConstraintAnalysis>,
        root: Id,
    ) -> Result<NyarModule, NyarError> {
        let mut functions_info = Vec::new();
        let mut imports_info = Vec::new();
        let mut exports_info = Vec::new();

        // Retrieve the root node
        let root_class = egraph.get_class(root);
        let root_node = &root_class.nodes[0];

        // Assume root is an Extension("module", [name, items...]) or just Seq
        let items = match root_node {
            IKun::Extension(name, args) if name == "module" => {
                // args[0] is name, args[1..] are items
                &args[1..]
            }
            IKun::Seq(items) => items.as_slice(),
            _ => std::slice::from_ref(&root),
        };

        // 1. Separate functions, imports, and exports from main statements
        let mut main_stmts = Vec::new();
        let mut functions = Vec::new();

        for &item in items {
            let node_class = egraph.get_class(item);
            println!(
                "Translate: processing item {:?}: {:?}",
                item, node_class.nodes
            );
            for entry in egraph.classes.iter() {
                println!("  Class {:?}: {:?}", entry.key(), entry.value().nodes);
            }
            let mut handled_as_special = false;

            for node in &node_class.nodes {
                match node {
                    IKun::Lambda(_params, _body) => {
                        // Directly a lambda (e.g. from FunctionDeclaration)
                        // We mark it as handled so it doesn't go to main_stmts
                        // unless it's an anonymous lambda used as an expression (which shouldn't be at top level anyway)
                        handled_as_special = true;
                        break;
                    }
                    IKun::StateUpdate(target, value) => {
                        let mut is_function = false;

                        if let Some((params, body)) = find_lambda(egraph, *value) {
                            let target_class = egraph.get_class(*target);
                            for t_node in &target_class.nodes {
                                if let IKun::Symbol(name) = t_node {
                                    functions.push((name.clone(), params.clone(), body));
                                    is_function = true;
                                    break;
                                }
                            }
                        }

                        if is_function {
                            handled_as_special = true;
                            break;
                        }
                    }
                    IKun::Extension(name, args) => {
                        match name.as_str() {
                            "import" => {
                                if !args.is_empty() {
                                    let source_class = egraph.get_class(args[0]);
                                    for s_node in &source_class.nodes {
                                        if let IKun::StringConstant(source) = s_node {
                                            if args.len() > 1 {
                                                for &symbol_id in &args[1..] {
                                                    let symbol_class = egraph.get_class(symbol_id);
                                                    for sym_node in &symbol_class.nodes {
                                                        if let IKun::Symbol(symbol_name) = sym_node
                                                        {
                                                            imports_info.push(ImportInfo {
                                                                provider: source.clone(),
                                                                symbol: symbol_name.clone().into(),
                                                            });
                                                            break;
                                                        }
                                                    }
                                                }
                                            } else {
                                                imports_info.push(ImportInfo {
                                                    provider: source.clone(),
                                                    symbol: "*".to_string().into(),
                                                });
                                            }
                                            break;
                                        }
                                    }
                                }
                                handled_as_special = true;
                                break;
                            }
                            "export" => {
                                if !args.is_empty() {
                                    let exported_item_class = egraph.get_class(args[0]);
                                    for exported_item in &exported_item_class.nodes {
                                        match exported_item {
                                            IKun::StateUpdate(target, value) => {
                                                let mut is_function = false;
                                                if let Some((params, body)) =
                                                    find_lambda(egraph, *value)
                                                {
                                                    let target_class = egraph.get_class(*target);
                                                    for t_node in &target_class.nodes {
                                                        if let IKun::Symbol(name) = t_node {
                                                            functions.push((
                                                                name.clone(),
                                                                params.clone(),
                                                                body,
                                                            ));
                                                            exports_info.push(ExportInfo {
                                                                symbol: name.clone().into(),
                                                                chunk_idx: (functions.len()) as u16,
                                                            });
                                                            is_function = true;
                                                            break;
                                                        }
                                                    }
                                                }
                                                if is_function {
                                                    handled_as_special = true;
                                                    break;
                                                }
                                            }
                                            IKun::Lambda(_params, _body) => {
                                                // Handle exported Lambda directly (if it doesn't have a name, we might have an issue,
                                                // but usually exports have names from the declaration)
                                                // For now, let's just mark as handled to avoid warning
                                                handled_as_special = true;
                                                break;
                                            }
                                            _ => {}
                                        }
                                    }
                                }
                                if handled_as_special {
                                    break;
                                }
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }

            if !handled_as_special {
                main_stmts.push(item);
            }
        }

        // 2. Generate Main Chunk
        self.generate_main_chunk(egraph, &main_stmts)?;

        // 3. Generate Function Chunks
        for (name, params, body) in functions {
            functions_info.push((name.clone(), params.len()));
            self.generate_function_chunk(egraph, &name, &params, body)?;
        }

        Ok(NyarModule {
            version: 1,
            flags: 0,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            constants: std::mem::take(&mut self.constants),
            effects: Vec::new(),
            chunks: std::mem::take(&mut self.chunks),
            classes: Vec::new(),
            traits: Vec::new(),
            impls: Vec::new(),
            imports: imports_info,
            exports: exports_info,
        })
    }

    fn generate_main_chunk(
        &mut self,
        egraph: &EGraph<IKun, ConstraintAnalysis>,
        statements: &[Id],
    ) -> Result<(), NyarError> {
        self.reset_for_function();

        for &stmt in statements {
            self.generate_node(egraph, stmt, true)?;
        }

        // 默认返回 0
        self.emit_u8(Opcode::I32Ext as u8);
        self.emit_u8(I32Ext::Const as u8);
        self.emit_i32(0);
        self.emit_u8(Opcode::Return as u8);

        self.patch_jumps();

        self.chunks.push(Chunk {
            locals: self.local_index as u16,
            upvalues: 0,
            max_stack: 64,
            code: std::mem::take(&mut self.code),
            handlers: vec![],
            lines: vec![],
            decoded: std::sync::OnceLock::new(),
            hotness: std::sync::atomic::AtomicU32::new(0),
        });

        Ok(())
    }

    fn generate_function_chunk(
        &mut self,
        egraph: &EGraph<IKun, ConstraintAnalysis>,
        _name: &str,
        parameters: &[String],
        body: Id,
    ) -> Result<(), NyarError> {
        self.reset_for_function();

        // 处理参数
        for (i, param_name) in parameters.iter().enumerate() {
            self.locals.insert(param_name.clone(), i as u8);
            self.local_index += 1;
        }

        // 生成函数体
        // body should be a Seq (Block)
        self.generate_node(egraph, body, true)?;

        // 确保有 Return
        if self.code.last() != Some(&(Opcode::Return as u8)) {
            self.emit_u8(Opcode::I32Ext as u8);
            self.emit_u8(I32Ext::Const as u8);
            self.emit_i32(0);
            self.emit_u8(Opcode::Return as u8);
        }

        self.patch_jumps();

        self.chunks.push(Chunk {
            locals: self.local_index as u16,
            upvalues: 0,
            max_stack: 64,
            code: std::mem::take(&mut self.code),
            handlers: vec![],
            lines: vec![],
            decoded: std::sync::OnceLock::new(),
            hotness: std::sync::atomic::AtomicU32::new(0),
        });

        Ok(())
    }

    fn generate_node(
        &mut self,
        egraph: &EGraph<IKun, ConstraintAnalysis>,
        id: Id,
        is_statement: bool,
    ) -> Result<(), NyarError> {
        let node_class = egraph.get_class(id);

        // Try to find a node we can handle
        let mut handled = false;
        for node in &node_class.nodes {
            match node {
                IKun::Constant(v) => {
                    self.emit_u8(Opcode::I32Ext as u8);
                    self.emit_u8(I32Ext::Const as u8);
                    self.emit_i32(*v as i32);
                    if is_statement {
                        self.emit_u8(Opcode::Pop as u8);
                    }
                    handled = true;
                }
                IKun::StringConstant(s) => {
                    let idx = self.constants.len() as u16;
                    self.constants.push(Constant::String(s.clone()));
                    self.emit_u8(Opcode::Push as u8);
                    self.emit_u16(idx);
                    if is_statement {
                        self.emit_u8(Opcode::Pop as u8);
                    }
                    handled = true;
                }
                IKun::Symbol(name) => {
                    if let Some(&index) = self.locals.get(name.as_str()) {
                        self.emit_u8(Opcode::LoadLocal as u8);
                        self.emit_u8(index);
                    } else {
                        // Try global or builtin
                        let idx = self.constants.len() as u16;
                        self.constants.push(Constant::String(name.clone()));
                        self.emit_u8(Opcode::LoadGlobal as u8);
                        self.emit_u16(idx);
                    }
                    if is_statement {
                        self.emit_u8(Opcode::Pop as u8);
                    }
                    handled = true;
                }
                IKun::StateUpdate(target, value) => {
                    self.generate_node(egraph, *value, false)?;

                    let target_class = egraph.get_class(*target);
                    let mut target_handled = false;
                    for target_node in &target_class.nodes {
                        if let IKun::Symbol(name) = target_node {
                            let index = if let Some(&idx) = self.locals.get(name.as_str()) {
                                idx
                            } else {
                                let idx = self.local_index;
                                self.locals.insert(name.clone(), idx);
                                self.local_index += 1;
                                idx
                            };
                            self.emit_u8(Opcode::StoreLocal as u8);
                            self.emit_u8(index);
                            if !is_statement {
                                self.emit_u8(Opcode::LoadLocal as u8);
                                self.emit_u8(index);
                            }
                            target_handled = true;
                            break;
                        }
                    }
                    if !target_handled && is_statement {
                        self.emit_u8(Opcode::Pop as u8);
                    }
                    handled = true;
                }
                IKun::Seq(items) => {
                    let len = items.len();
                    for (i, &item) in items.iter().enumerate() {
                        let is_last = i == len - 1;
                        self.generate_node(
                            egraph,
                            item,
                            if is_last { is_statement } else { true },
                        )?;
                    }
                    handled = true;
                }
                IKun::Choice(cond, then_branch, else_branch) => {
                    let false_label = self.new_label("if_false");
                    let end_label = self.new_label("if_end");

                    // 1. Evaluate condition
                    self.generate_node(egraph, *cond, false)?;

                    // 2. Jump if false
                    self.emit_jump(&false_label, true);

                    // 3. True block
                    self.generate_node(egraph, *then_branch, is_statement)?;

                    if !is_statement {
                        self.emit_jump(&end_label, false);
                    } else {
                        // If it's a statement, we don't need to jump to end if we already returned or something,
                        // but for simplicity we always jump to end to skip the else branch.
                        self.emit_jump(&end_label, false);
                    }

                    // 4. False block
                    self.define_label(&false_label);
                    self.generate_node(egraph, *else_branch, is_statement)?;

                    // 5. End block
                    self.define_label(&end_label);
                    handled = true;
                }
                IKun::Return(val) => {
                    self.generate_node(egraph, *val, false)?;
                    self.emit_u8(Opcode::Return as u8);
                    handled = true;
                }
                IKun::Extension(name, args) => {
                    match name.as_str() {
                        "while" => {
                            if args.len() < 2 {
                                return Ok(());
                            }
                            let cond_label = self.new_label("while_cond");
                            let end_label = self.new_label("while_end");

                            // 1. Condition block
                            self.define_label(&cond_label);
                            self.generate_node(egraph, args[0], false)?;
                            self.emit_jump(&end_label, true);

                            // 2. Body block
                            self.generate_node(egraph, args[1], true)?;
                            self.emit_jump(&cond_label, false);

                            // 3. End block
                            self.define_label(&end_label);

                            // While loop as expression returns null
                            if !is_statement {
                                // We don't have a Null opcode in I32Ext, so we push 0 for now
                                self.emit_u8(Opcode::I32Ext as u8);
                                self.emit_u8(I32Ext::Const as u8);
                                self.emit_i32(0);
                            }
                            handled = true;
                        }
                        "return" => {
                            if !args.is_empty() {
                                self.generate_node(egraph, args[0], false)?;
                            } else {
                                self.emit_u8(Opcode::I32Ext as u8);
                                self.emit_u8(I32Ext::Const as u8);
                                self.emit_i32(0);
                            }
                            self.emit_u8(Opcode::Return as u8);
                            // After return, we don't need to pop even if is_statement is true
                            // because the frame is gone.
                            handled = true;
                        }
                        "add" | "sub" | "mul" | "div" | "eq" | "lt" | "gt" => {
                            self.generate_node(egraph, args[0], false)?;
                            self.generate_node(egraph, args[1], false)?;
                            self.emit_u8(Opcode::I32Ext as u8);
                            let op = match name.as_str() {
                                "add" => I32Ext::Add,
                                "sub" => I32Ext::Sub,
                                "mul" => I32Ext::Mul,
                                "div" => I32Ext::DivS,
                                "eq" => I32Ext::Eq,
                                "lt" => I32Ext::LtS,
                                "gt" => I32Ext::GtS,
                                _ => unreachable!(),
                            };
                            self.emit_u8(op as u8);
                            if is_statement {
                                self.emit_u8(Opcode::Pop as u8);
                            }
                            handled = true;
                        }
                        "member" => {
                            // object.property
                            self.generate_node(egraph, args[0], false)?;
                            let prop_class = egraph.get_class(args[1]);
                            let mut prop_handled = false;
                            for prop_node in &prop_class.nodes {
                                if let IKun::Symbol(prop_name) = prop_node {
                                    let name_idx = self.constants.len() as u16;
                                    self.constants.push(Constant::String(prop_name.clone()));
                                    self.emit_u8(Opcode::GetField as u8);
                                    self.emit_u16(name_idx);
                                    prop_handled = true;
                                    break;
                                }
                            }
                            if !prop_handled {
                                // Fallback to index access if property is not a literal symbol
                                self.generate_node(egraph, args[1], false)?;
                                self.emit_u8(Opcode::GetElement as u8);
                            }
                            if is_statement {
                                self.emit_u8(Opcode::Pop as u8);
                            }
                            handled = true;
                        }
                        _ => {
                            // Unknown extension, ignore or error
                        }
                    }
                }
                IKun::Apply(func, args) => {
                    let func_class = egraph.get_class(*func);
                    let mut func_handled = false;

                    // Try to see if it's a method call like object.method(args)
                    for node in &func_class.nodes {
                        if let IKun::Extension(ext_name, ext_args) = node {
                            if ext_name == "member" && ext_args.len() == 2 {
                                // object.method(args) -> receiver = object, method = ext_args[1]
                                // 1. Push receiver
                                self.generate_node(egraph, ext_args[0], false)?;
                                // 2. Push arguments
                                for arg in args {
                                    self.generate_node(egraph, *arg, false)?;
                                }
                                // 3. InvokeMethod
                                let prop_class = egraph.get_class(ext_args[1]);
                                for prop_node in &prop_class.nodes {
                                    if let IKun::Symbol(prop_name) = prop_node {
                                        let name_idx = self.constants.len() as u16;
                                        self.constants.push(Constant::String(prop_name.clone()));
                                        self.emit_u8(Opcode::InvokeMethod as u8);
                                        self.emit_u16(name_idx);
                                        self.emit_u8(args.len() as u8);
                                        func_handled = true;
                                        break;
                                    }
                                }
                                if func_handled {
                                    break;
                                }
                            }
                        }
                    }

                    if !func_handled {
                        // Simplified call generation
                        for arg in args {
                            self.generate_node(egraph, *arg, false)?;
                        }

                        for func_node in &func_class.nodes {
                            if let IKun::Symbol(name) = func_node {
                                // Call by name (could be local function, exported function from another module, or built-in)
                                let name_idx = self.constants.len() as u16;
                                self.constants.push(Constant::String(name.clone()));
                                self.emit_u8(Opcode::CallSymbol as u8);
                                self.emit_u16(name_idx);
                                self.emit_u8(args.len() as u8);
                                func_handled = true;
                                break;
                            }
                        }
                    }

                    if !func_handled {
                        // Call by value (expression returning a function)
                        self.generate_node(egraph, *func, false)?;
                        self.emit_u8(Opcode::CallClosure as u8);
                        self.emit_u8(args.len() as u8);
                    }

                    if is_statement {
                        self.emit_u8(Opcode::Pop as u8);
                    }
                    handled = true;
                }
                _ => {}
            }
            if handled {
                break;
            }
        }

        if !handled {
            println!(
                "Codegen: Unhandled node class {:?}: {:?}",
                id, node_class.nodes
            );
        }

        Ok(())
    }
}
