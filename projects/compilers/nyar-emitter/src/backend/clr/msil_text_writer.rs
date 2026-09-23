use std_data::text::msil::{MsilAssembly, MsilInstruction, MsilInstructionOperand, MsilMethodBody, MsilMethodRef, MsilModule, MsilTypeDef};

pub struct MsilTextWriter {
    indent: usize,
    indent_text: String,
    output: String,
}

impl MsilTextWriter {
    pub fn new() -> Self {
        Self { indent: 0, indent_text: "    ".to_string(), output: String::new() }
    }

    pub fn with_indent_text(mut self, indent_text: impl Into<String>) -> Self {
        self.indent_text = indent_text.into();
        self
    }

    pub fn write_module(mut self, module: &MsilModule) -> String {
        self.write_assembly(&module.assembly);
        self.writeln("");

        for type_def in &module.types {
            self.write_type(type_def);
            self.writeln("");
        }

        if !module.global_methods.is_empty() {
            for method in &module.global_methods {
                self.write_method(method);
                self.writeln("");
            }
        }

        self.output
    }

    pub fn write_module_with_file_name(mut self, module: &MsilModule, file_name: &str) -> String {
        self.write_assembly(&module.assembly);
        self.writeln(&format!(".module {}", file_name));
        self.writeln("");

        for type_def in &module.types {
            self.write_type(type_def);
            self.writeln("");
        }

        if !module.global_methods.is_empty() {
            for method in &module.global_methods {
                self.write_method(method);
                self.writeln("");
            }
        }

        self.output
    }

    fn write_assembly(&mut self, assembly: &MsilAssembly) {
        for ext in &assembly.externs {
            self.writeln(&format!(".assembly extern {} {{}}", ext));
        }
        if !assembly.externs.is_empty() {
            self.writeln("");
        }
        self.writeln(&format!(".assembly {} {{}}", assembly.name));
    }

    fn write_type(&mut self, type_def: &MsilTypeDef) {
        if type_def.is_value_type {
            self.writeln(&format!(
                ".class public sequential ansi sealed beforefieldinit {} extends [mscorlib]System.ValueType",
                type_def.full_name
            ));
        }
        else {
            self.writeln(&format!(".class public auto ansi beforefieldinit {} extends [mscorlib]System.Object", type_def.full_name));
        }
        self.writeln("{");
        self.indent += 1;

        for field in &type_def.fields {
            let static_kw = if field.is_static { "static " } else { "" };
            self.writeln(&format!(".field public {}{} {}", static_kw, field.ty, field.name));
        }

        if type_def.methods.is_empty() {
            if type_def.fields.is_empty() {
                self.writeln("");
            }
        }
        else {
            if !type_def.fields.is_empty() {
                self.writeln("");
            }
            for (i, method) in type_def.methods.iter().enumerate() {
                if i > 0 {
                    self.writeln("");
                }
                self.write_method(method);
            }
        }

        self.indent -= 1;
        self.writeln("}");
    }

    fn write_method(&mut self, method: &MsilMethodBody) {
        // Instance `.ctor` must not be written as `static` — ilasm/readers treat that as a
        // signature mismatch with HASTHIS MethodDefs (and mirrors PE MethodAttributes).
        let is_instance_ctor = method.method.name == ".ctor";
        let is_static_cctor = method.method.name == ".cctor";
        let is_static = is_static_cctor
            || (!is_instance_ctor && (method.method.owner.is_none() || matches!(method.method.name.as_str(), "instance" | "get_instance")));
        let static_kw = if is_static { "static " } else { "instance " };
        let special = if is_instance_ctor || is_static_cctor { "specialname rtspecialname " } else { "" };
        self.writeln(&format!(
            ".method public hidebysig {special}{static_kw}{} {}{} cil managed",
            method.method.signature.return_type,
            method.method.name,
            method.method.signature.parameter_list_text()
        ));
        self.writeln("{");
        self.indent += 1;

        if method.is_entry_point {
            self.writeln(".entrypoint");
        }

        if !method.locals.is_empty() {
            let locals = method.locals.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ");
            self.writeln(&format!(".locals init ({locals})"));
        }

        self.writeln(&format!(".maxstack {}", method.max_stack));

        for instruction in &method.instructions {
            self.write_instruction(instruction);
        }

        self.indent -= 1;
        self.writeln("}");
    }

    fn write_instruction(&mut self, instruction: &MsilInstruction) {
        if let Some(label) = &instruction.label {
            self.writeln(&format!("{}:", label));
        }
        match &instruction.operand {
            Some(operand) => {
                let operand_str = format_operand(operand);
                self.writeln_indented(&format!("{} {}", instruction.opcode, operand_str), 1);
            }
            None => {
                self.writeln_indented(&instruction.opcode.to_string(), 1);
            }
        }
    }

    fn writeln(&mut self, text: &str) {
        self.writeln_indented(text, 0);
    }

    fn writeln_indented(&mut self, text: &str, extra_indent: usize) {
        if text.is_empty() {
            self.output.push('\n');
            return;
        }
        for _ in 0..(self.indent + extra_indent) {
            self.output.push_str(&self.indent_text);
        }
        self.output.push_str(text);
        self.output.push('\n');
    }
}

impl Default for MsilTextWriter {
    fn default() -> Self {
        Self::new()
    }
}

fn format_operand(operand: &MsilInstructionOperand) -> String {
    match operand {
        MsilInstructionOperand::Integer(value) => value.to_string(),
        MsilInstructionOperand::Float(text) => text.clone(),
        MsilInstructionOperand::StringLiteral(value) => format!("\"{}\"", escape_string(value)),
        MsilInstructionOperand::Symbol(name) => name.clone(),
        MsilInstructionOperand::Method(method) => format_method_ref(method),
        MsilInstructionOperand::Type(type_name) => type_name.clone(),
        MsilInstructionOperand::Token(token) => format!("0x{:08X}", token),
        MsilInstructionOperand::BranchTarget(label) => label.clone(),
        MsilInstructionOperand::Field(owner, name) => format!("{}::{}", owner, name),
        MsilInstructionOperand::Raw(text) => text.clone(),
    }
}

fn format_method_ref(method: &MsilMethodRef) -> String {
    match &method.owner {
        Some(owner) => format!("{} {}::{}{}", method.signature.return_type, owner, method.name, method.signature.parameter_list_text()),
        None => format!("{} {}{}", method.signature.return_type, method.name, method.signature.parameter_list_text()),
    }
}

fn escape_string(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            c => result.push(c),
        }
    }
    result
}
