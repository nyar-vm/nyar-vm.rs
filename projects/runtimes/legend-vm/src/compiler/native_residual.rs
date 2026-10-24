//! Native-bound PE residual (Futamura 1st projection product for `legacy-vm`).
//!
//! Guests residualize into [`nyar_language::ResidualSink`]; this crate's **product**
//! sink records ops for **native / x64 → Windows PE**, not NyarVM bytecode.
//!
//! Full x64 lowering is still incomplete; specialization always produces a
//! [`NativeResidualModule`], and [`emit_native_pe`] wraps a minimal PE (exit-code
//! when the residual main is a pure integer result, otherwise exit-0 stub).

use std::collections::HashMap;

use miette::{Result, miette};
use nyar_language::ResidualSink;

use super::PeCompiler;

/// Target lane for host-script PE residuals (product path).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NativeResidualTarget {
    /// Windows PE / x64 native frames (legacy-vm endpoint).
    #[default]
    Native,
}

/// One residual operation (language-neutral; mirrors [`ResidualSink`]).
#[derive(Debug, Clone, PartialEq)]
pub enum NativeResidualOp {
    /// Push i64.
    PushI64(i64),
    /// Push bool.
    PushBool(bool),
    /// Push string.
    PushString(String),
    /// Push nil.
    PushNil,
    /// Load local.
    Load(String),
    /// Store local.
    Store(String),
    /// Pop.
    Pop,
    /// `+`
    Add,
    /// `-`
    Sub,
    /// `*`
    Mul,
    /// `/`
    Div,
    /// `%`
    Rem,
    /// `==`
    Eq,
    /// `!=`
    Ne,
    /// `<`
    Lt,
    /// `<=`
    Le,
    /// `>`
    Gt,
    /// `>=`
    Ge,
    /// String concat.
    Concat,
    /// Logical not.
    Not,
    /// Print `argc` stack values.
    Print(usize),
    /// Label.
    Label(String),
    /// Jump.
    Jump(String),
    /// Jump if falsey.
    JumpIfFalse(String),
    /// Jump if truthy.
    JumpIfTrue(String),
    /// Call residual function.
    Call {
        /// Callee name.
        name: String,
        /// Argument count.
        argc: usize,
    },
    /// Return.
    Ret,
}

/// One residual function body.
#[derive(Debug, Clone, PartialEq)]
pub struct NativeResidualFunction {
    /// Function name.
    pub name: String,
    /// Parameter names.
    pub parameters: Vec<String>,
    /// Residual ops.
    pub ops: Vec<NativeResidualOp>,
}

/// Native-bound residual module (specialize product).
#[derive(Debug, Clone, PartialEq)]
pub struct NativeResidualModule {
    /// Module name.
    pub name: String,
    /// Product target (always [`NativeResidualTarget::Native`] on this path).
    pub target: NativeResidualTarget,
    /// Residual functions (includes `main`).
    pub functions: Vec<NativeResidualFunction>,
}

impl NativeResidualModule {
    /// Total residual ops across all functions (proves specialize ran).
    pub fn op_count(&self) -> usize {
        self.functions.iter().map(|function| function.ops.len()).sum()
    }

    /// Whether this residual is bound to the native product lane.
    pub fn is_native_bound(&self) -> bool {
        self.target == NativeResidualTarget::Native
    }

    /// Look up a function by name.
    pub fn function(&self, name: &str) -> Option<&NativeResidualFunction> {
        self.functions.iter().find(|function| function.name == name)
    }
}

/// [`ResidualSink`] that records ops for native / PE emission.
#[derive(Debug)]
pub struct NativeResidualSink {
    module_name: String,
    functions: Vec<NativeResidualFunction>,
    current: Option<NativeResidualFunction>,
}

impl NativeResidualSink {
    /// Create a sink for `module_name`.
    pub fn new(module_name: impl Into<String>) -> Self {
        Self { module_name: module_name.into(), functions: Vec::new(), current: None }
    }

    /// Finish specialization into a native-bound residual module.
    pub fn finish(mut self) -> NativeResidualModule {
        if self.current.is_some() {
            self.end_function();
        }
        NativeResidualModule { name: self.module_name, target: NativeResidualTarget::Native, functions: self.functions }
    }

    fn push_op(&mut self, op: NativeResidualOp) {
        let function = self.current.as_mut().expect("begin_function first");
        function.ops.push(op);
    }
}

impl ResidualSink for NativeResidualSink {
    fn push_i64(&mut self, value: i64) {
        self.push_op(NativeResidualOp::PushI64(value));
    }

    fn push_bool(&mut self, value: bool) {
        self.push_op(NativeResidualOp::PushBool(value));
    }

    fn push_string(&mut self, value: &str) {
        self.push_op(NativeResidualOp::PushString(value.to_string()));
    }

    fn push_nil(&mut self) {
        self.push_op(NativeResidualOp::PushNil);
    }

    fn load(&mut self, name: &str) {
        self.push_op(NativeResidualOp::Load(name.to_string()));
    }

    fn store(&mut self, name: &str) {
        self.push_op(NativeResidualOp::Store(name.to_string()));
    }

    fn pop(&mut self) {
        self.push_op(NativeResidualOp::Pop);
    }

    fn add(&mut self) {
        self.push_op(NativeResidualOp::Add);
    }

    fn sub(&mut self) {
        self.push_op(NativeResidualOp::Sub);
    }

    fn mul(&mut self) {
        self.push_op(NativeResidualOp::Mul);
    }

    fn div(&mut self) {
        self.push_op(NativeResidualOp::Div);
    }

    fn rem(&mut self) {
        self.push_op(NativeResidualOp::Rem);
    }

    fn eq(&mut self) {
        self.push_op(NativeResidualOp::Eq);
    }

    fn ne(&mut self) {
        self.push_op(NativeResidualOp::Ne);
    }

    fn lt(&mut self) {
        self.push_op(NativeResidualOp::Lt);
    }

    fn le(&mut self) {
        self.push_op(NativeResidualOp::Le);
    }

    fn gt(&mut self) {
        self.push_op(NativeResidualOp::Gt);
    }

    fn ge(&mut self) {
        self.push_op(NativeResidualOp::Ge);
    }

    fn concat(&mut self) {
        self.push_op(NativeResidualOp::Concat);
    }

    fn not(&mut self) {
        self.push_op(NativeResidualOp::Not);
    }

    fn print(&mut self, argc: usize) {
        self.push_op(NativeResidualOp::Print(argc));
    }

    fn label(&mut self, name: &str) {
        self.push_op(NativeResidualOp::Label(name.to_string()));
    }

    fn jump(&mut self, label: &str) {
        self.push_op(NativeResidualOp::Jump(label.to_string()));
    }

    fn jump_if_false(&mut self, label: &str) {
        self.push_op(NativeResidualOp::JumpIfFalse(label.to_string()));
    }

    fn jump_if_true(&mut self, label: &str) {
        self.push_op(NativeResidualOp::JumpIfTrue(label.to_string()));
    }

    fn begin_function(&mut self, name: &str) {
        if self.current.is_some() {
            self.end_function();
        }
        self.current = Some(NativeResidualFunction { name: name.to_string(), parameters: Vec::new(), ops: Vec::new() });
    }

    fn add_parameter(&mut self, name: &str) {
        let function = self.current.as_mut().expect("begin_function first");
        function.parameters.push(name.to_string());
    }

    fn end_function(&mut self) {
        let function = self.current.take().expect("begin_function first");
        self.functions.push(function);
    }

    fn call(&mut self, name: &str, argc: usize) {
        self.push_op(NativeResidualOp::Call { name: name.to_string(), argc });
    }

    fn ret(&mut self) {
        self.push_op(NativeResidualOp::Ret);
    }
}

/// Specialize `source` for `language` into a **native-bound** residual module.
pub fn specialize_language(language: &str, source: &str, module_name: &str) -> Result<NativeResidualModule, String> {
    match language {
        "lua" => {
            let mut sink = NativeResidualSink::new(module_name);
            nyar_language::specialize_lua_into(source, &mut sink)?;
            Ok(sink.finish())
        }
        "bash" | "sh" | "powershell" | "ps1" | "pwsh" | "tcl" | "c" => {
            Err(format!("PE specialization for '{language}' is stubbed; Lua → native residual is the first projection"))
        }
        other => Err(format!("no PE specialization hook for '{other}'")),
    }
}

/// Values used while interpreting a native residual (goldens / exit-code fold).
#[derive(Debug, Clone, PartialEq)]
pub enum ResidualValue {
    /// Nil.
    Nil,
    /// Bool.
    Bool(bool),
    /// Integer.
    Int(i64),
    /// String.
    String(String),
}

impl ResidualValue {
    fn is_truthy(&self) -> bool {
        !matches!(self, ResidualValue::Nil | ResidualValue::Bool(false))
    }

    fn as_int(&self) -> Option<i64> {
        match self {
            ResidualValue::Int(value) => Some(*value),
            ResidualValue::Bool(true) => Some(1),
            ResidualValue::Bool(false) | ResidualValue::Nil => Some(0),
            ResidualValue::String(_) => None,
        }
    }

    /// Stable string key for golden comparison.
    pub fn to_key(&self) -> String {
        match self {
            ResidualValue::Nil => String::new(),
            ResidualValue::Bool(value) => value.to_string(),
            ResidualValue::Int(value) => value.to_string(),
            ResidualValue::String(value) => value.clone(),
        }
    }
}

/// Interpret a native residual's `main` (semantic check without nyar-vm).
pub fn eval_native_residual(module: &NativeResidualModule) -> Result<ResidualValue, String> {
    let mut frames = Vec::new();
    call_function(module, "main", Vec::new(), &mut frames)
}

fn call_function(module: &NativeResidualModule, name: &str, args: Vec<ResidualValue>, frames: &mut Vec<()>) -> Result<ResidualValue, String> {
    if frames.len() > 64 {
        return Err("native residual call depth exceeded".into());
    }
    frames.push(());
    let function = module.function(name).ok_or_else(|| format!("missing residual function '{name}'"))?;
    let mut locals: HashMap<String, ResidualValue> = HashMap::new();
    for (index, param) in function.parameters.iter().enumerate() {
        locals.insert(param.clone(), args.get(index).cloned().unwrap_or(ResidualValue::Nil));
    }
    let labels = build_labels(&function.ops);
    let mut stack: Vec<ResidualValue> = Vec::new();
    let mut pc = 0usize;
    let result = loop {
        if pc >= function.ops.len() {
            break stack.pop().unwrap_or(ResidualValue::Nil);
        }
        match &function.ops[pc] {
            NativeResidualOp::PushI64(value) => stack.push(ResidualValue::Int(*value)),
            NativeResidualOp::PushBool(value) => stack.push(ResidualValue::Bool(*value)),
            NativeResidualOp::PushString(value) => stack.push(ResidualValue::String(value.clone())),
            NativeResidualOp::PushNil => stack.push(ResidualValue::Nil),
            NativeResidualOp::Load(name) => {
                stack.push(locals.get(name).cloned().unwrap_or(ResidualValue::Nil));
            }
            NativeResidualOp::Store(name) => {
                let value = stack.pop().unwrap_or(ResidualValue::Nil);
                locals.insert(name.clone(), value);
            }
            NativeResidualOp::Pop => {
                stack.pop();
            }
            NativeResidualOp::Add => bin_num(&mut stack, |a, b| a.wrapping_add(b))?,
            NativeResidualOp::Sub => bin_num(&mut stack, |a, b| a.wrapping_sub(b))?,
            NativeResidualOp::Mul => bin_num(&mut stack, |a, b| a.wrapping_mul(b))?,
            NativeResidualOp::Div => bin_num(&mut stack, |a, b| if b == 0 { 0 } else { a / b })?,
            NativeResidualOp::Rem => bin_num(&mut stack, |a, b| if b == 0 { 0 } else { a % b })?,
            NativeResidualOp::Eq => bin_cmp(&mut stack, |a, b| a == b)?,
            NativeResidualOp::Ne => bin_cmp(&mut stack, |a, b| a != b)?,
            NativeResidualOp::Lt => bin_num_cmp(&mut stack, |a, b| a < b)?,
            NativeResidualOp::Le => bin_num_cmp(&mut stack, |a, b| a <= b)?,
            NativeResidualOp::Gt => bin_num_cmp(&mut stack, |a, b| a > b)?,
            NativeResidualOp::Ge => bin_num_cmp(&mut stack, |a, b| a >= b)?,
            NativeResidualOp::Concat => {
                let right = stack.pop().unwrap_or(ResidualValue::Nil);
                let left = stack.pop().unwrap_or(ResidualValue::Nil);
                stack.push(ResidualValue::String(format!("{}{}", display_value(&left), display_value(&right))));
            }
            NativeResidualOp::Not => {
                let value = stack.pop().unwrap_or(ResidualValue::Nil);
                stack.push(ResidualValue::Bool(!value.is_truthy()));
            }
            NativeResidualOp::Print(argc) => {
                let mut args = Vec::with_capacity(*argc);
                for _ in 0..*argc {
                    args.push(stack.pop().unwrap_or(ResidualValue::Nil));
                }
                args.reverse();
                let text = args.iter().map(display_value).collect::<Vec<_>>().join("\t");
                println!("{text}");
                if let Some(last) = args.last() {
                    stack.push(last.clone());
                }
                else {
                    stack.push(ResidualValue::Nil);
                }
            }
            NativeResidualOp::Label(_) => {}
            NativeResidualOp::Jump(label) => {
                pc = *labels.get(label).ok_or_else(|| format!("unknown label '{label}'"))?;
                continue;
            }
            NativeResidualOp::JumpIfFalse(label) => {
                let cond = stack.pop().unwrap_or(ResidualValue::Nil);
                if !cond.is_truthy() {
                    pc = *labels.get(label).ok_or_else(|| format!("unknown label '{label}'"))?;
                    continue;
                }
            }
            NativeResidualOp::JumpIfTrue(label) => {
                let cond = stack.pop().unwrap_or(ResidualValue::Nil);
                if cond.is_truthy() {
                    pc = *labels.get(label).ok_or_else(|| format!("unknown label '{label}'"))?;
                    continue;
                }
            }
            NativeResidualOp::Call { name, argc } => {
                let mut args = Vec::with_capacity(*argc);
                for _ in 0..*argc {
                    args.push(stack.pop().unwrap_or(ResidualValue::Nil));
                }
                args.reverse();
                let value = call_function(module, name, args, frames)?;
                stack.push(value);
            }
            NativeResidualOp::Ret => {
                break stack.pop().unwrap_or(ResidualValue::Nil);
            }
        }
        pc += 1;
    };
    frames.pop();
    Ok(result)
}

fn build_labels(ops: &[NativeResidualOp]) -> HashMap<String, usize> {
    let mut labels = HashMap::new();
    for (index, op) in ops.iter().enumerate() {
        if let NativeResidualOp::Label(name) = op {
            labels.insert(name.clone(), index);
        }
    }
    labels
}

fn bin_num(stack: &mut Vec<ResidualValue>, op: impl FnOnce(i64, i64) -> i64) -> Result<(), String> {
    let right = stack.pop().and_then(|value| value.as_int()).ok_or("numeric RHS")?;
    let left = stack.pop().and_then(|value| value.as_int()).ok_or("numeric LHS")?;
    stack.push(ResidualValue::Int(op(left, right)));
    Ok(())
}

fn bin_cmp(stack: &mut Vec<ResidualValue>, op: impl FnOnce(&ResidualValue, &ResidualValue) -> bool) -> Result<(), String> {
    let right = stack.pop().unwrap_or(ResidualValue::Nil);
    let left = stack.pop().unwrap_or(ResidualValue::Nil);
    stack.push(ResidualValue::Bool(op(&left, &right)));
    Ok(())
}

fn bin_num_cmp(stack: &mut Vec<ResidualValue>, op: impl FnOnce(i64, i64) -> bool) -> Result<(), String> {
    let right = stack.pop().and_then(|value| value.as_int()).ok_or("numeric RHS")?;
    let left = stack.pop().and_then(|value| value.as_int()).ok_or("numeric LHS")?;
    stack.push(ResidualValue::Bool(op(left, right)));
    Ok(())
}

fn display_value(value: &ResidualValue) -> String {
    match value {
        ResidualValue::Nil => "nil".into(),
        ResidualValue::Bool(value) => value.to_string(),
        ResidualValue::Int(value) => value.to_string(),
        ResidualValue::String(value) => value.clone(),
    }
}

/// Lower a native residual to raw x64 bytes suitable for [`PeCompiler`].
///
/// Subset: when `main` evaluates to an integer, emit `mov rax, imm` so the PE
/// epilog exits with that code. Otherwise emit an empty body (exit 0 stub) —
/// residual ops remain the source of truth for future full codegen.
pub fn lower_native_residual_to_x64(module: &NativeResidualModule) -> Result<Vec<u8>, String> {
    if !module.is_native_bound() {
        return Err("residual is not native-bound".into());
    }
    match eval_native_residual(module) {
        Ok(ResidualValue::Int(value)) => Ok(encode_mov_rax_imm64(value as u64)),
        Ok(_) => Ok(Vec::new()),
        Err(_) => Ok(Vec::new()),
    }
}

fn encode_mov_rax_imm64(value: u64) -> Vec<u8> {
    // REX.W mov rax, imm64 → 48 B8 imm64 (subset until full x64 residual lowering lands).
    let mut text = Vec::with_capacity(10);
    text.push(0x48);
    text.push(0xB8);
    text.extend_from_slice(&value.to_le_bytes());
    text
}

/// Specialize → lower → Windows PE bytes (and optionally write to `output_path`).
pub fn emit_native_pe(module: &NativeResidualModule, output_path: Option<&std::path::Path>) -> Result<Vec<u8>> {
    let x64 = lower_native_residual_to_x64(module).map_err(|error| miette!("{error}"))?;
    let compiler = PeCompiler;
    match output_path {
        Some(path) => compiler.compile("main", &x64, path),
        None => compiler.compile_to_bytes(&x64),
    }
}
