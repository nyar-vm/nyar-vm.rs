//! 宿主导入能力：加载期把符号解析为 [`HostOp`]，热路径只按枚举分派。

use crate::error::NyarRuntimeError;
use nyar_gc::{ObjectHeap, ObjectPayload};
use std_data::binary::nyar_ir::NyarImport;

use crate::value::Value;

/// 与 emitter 约定的宿主导入模块名。
pub const HOST_IMPORT_MODULE: &str = "nyar.host";

/// `nyar.host` 上已冻结的宿主操作（稠密枚举，非字符串合同）。
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostOp {
    AllocRecord,
    RecordGet,
    RecordSet,
    Print,
    StringConcat,
    ConsoleLog,
    I32ToI64,
    I32Div,
    I64Add,
    I64Sub,
    I64Mul,
    I64Div,
    I64Rem,
    I64Neg,
    I64Eq,
    I64Ne,
    I64Lt,
    I64Le,
    I64Gt,
    I64Ge,
    BoolNot,
    BoolAnd,
    BoolOr,
}

/// 加载期解析后的导入槽。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedImport {
    /// `nyar.host` 内建操作。
    Host(HostOp),
    /// 非宿主模块：热路径不得再按符号名猜；须由宿主按 import 下标另行绑定。
    External,
}

/// 将导入表项解析为 [`ResolvedImport`]（仅加载 / verify 期调用）。
pub fn resolve_import(import: &NyarImport) -> Result<ResolvedImport, NyarRuntimeError> {
    if import.module_name == HOST_IMPORT_MODULE {
        let op = parse_host_op(&import.symbol_name).ok_or_else(|| {
            NyarRuntimeError::ModuleLoad(format!(
                "unknown host import `{HOST_IMPORT_MODULE}::{}`",
                import.symbol_name
            ))
        })?;
        Ok(ResolvedImport::Host(op))
    }
    else {
        Ok(ResolvedImport::External)
    }
}

/// 加载期符号 → [`HostOp`]；未知符号返回 `None`。
pub fn parse_host_op(symbol: &str) -> Option<HostOp> {
    Some(match symbol {
        "alloc_record" => HostOp::AllocRecord,
        "record_get" => HostOp::RecordGet,
        "record_set" => HostOp::RecordSet,
        "print" => HostOp::Print,
        "string_concat" => HostOp::StringConcat,
        "console_log" => HostOp::ConsoleLog,
        "i32_to_i64" => HostOp::I32ToI64,
        "i32_div" => HostOp::I32Div,
        "i64_add" => HostOp::I64Add,
        "i64_sub" => HostOp::I64Sub,
        "i64_mul" => HostOp::I64Mul,
        "i64_div" => HostOp::I64Div,
        "i64_rem" => HostOp::I64Rem,
        "i64_neg" => HostOp::I64Neg,
        "i64_eq" => HostOp::I64Eq,
        "i64_ne" => HostOp::I64Ne,
        "i64_lt" => HostOp::I64Lt,
        "i64_le" => HostOp::I64Le,
        "i64_gt" => HostOp::I64Gt,
        "i64_ge" => HostOp::I64Ge,
        "bool_not" => HostOp::BoolNot,
        "bool_and" => HostOp::BoolAnd,
        "bool_or" => HostOp::BoolOr,
        _ => return None,
    })
}

/// 执行已解析的宿主操作（热路径无符号字符串）。
pub fn execute_host_op(op: HostOp, args: &[Value], heap: &mut ObjectHeap) -> Result<Value, NyarRuntimeError> {
    match op {
        HostOp::AllocRecord => {
            let type_name = match args.first() {
                Some(Value::String(name)) => name.clone(),
                Some(other) => {
                    return Err(NyarRuntimeError::TypeMismatch { expected: "string", actual: other.type_name().to_string() });
                }
                None => {
                    return Err(NyarRuntimeError::TypeMismatch { expected: "string", actual: "empty".to_string() });
                }
            };
            let object_id = heap.alloc(ObjectPayload::Record(vec![("__type__".to_string(), Value::String(type_name))]));
            Ok(Value::Object(object_id))
        }
        HostOp::RecordGet => {
            let field = match args.get(1) {
                Some(Value::String(name)) => name.as_str(),
                Some(other) => {
                    return Err(NyarRuntimeError::TypeMismatch { expected: "string", actual: other.type_name().to_string() });
                }
                None => return Ok(Value::Null),
            };
            let object_id = match args.first() {
                Some(Value::Object(id)) => *id,
                Some(Value::Null) => return Ok(Value::Null),
                Some(other) => {
                    return Err(NyarRuntimeError::TypeMismatch { expected: "object", actual: other.type_name().to_string() });
                }
                None => return Ok(Value::Null),
            };
            let payload = heap.get(object_id).ok_or_else(|| NyarRuntimeError::ModuleLoad(format!("invalid object id {object_id}")))?;
            Ok(match payload {
                ObjectPayload::Record(fields) => {
                    fields.iter().find(|(key, _)| key == field).map(|(_, value)| value.clone()).unwrap_or(Value::Null)
                }
                ObjectPayload::Coroutine(_) => Value::Null,
            })
        }
        HostOp::RecordSet => {
            let value = args.get(2).cloned().unwrap_or(Value::Null);
            let field = match args.get(1) {
                Some(Value::String(name)) => name.clone(),
                Some(other) => {
                    return Err(NyarRuntimeError::TypeMismatch { expected: "string", actual: other.type_name().to_string() });
                }
                None => return Ok(Value::Null),
            };
            if let Some(Value::Object(object_id)) = args.first() {
                if let Some(ObjectPayload::Record(fields)) = heap.get_mut(*object_id) {
                    if let Some(entry) = fields.iter_mut().find(|(key, _)| key == &field) {
                        entry.1 = value;
                    }
                    else {
                        fields.push((field, value));
                    }
                }
            }
            Ok(Value::Null)
        }
        HostOp::Print => {
            let text = args.iter().map(value_display).collect::<Vec<_>>().join("\t");
            println!("{text}");
            Ok(args.last().cloned().unwrap_or(Value::Null))
        }
        HostOp::StringConcat => {
            let left = args.first().map(value_display).unwrap_or_default();
            let right = args.get(1).map(value_display).unwrap_or_default();
            Ok(Value::String(format!("{left}{right}")))
        }
        HostOp::ConsoleLog => {
            if let Some(value) = args.first() {
                println!("{value}");
            }
            else {
                println!();
            }
            Ok(Value::Null)
        }
        HostOp::I32ToI64 => match args.first() {
            Some(Value::I32(value)) => Ok(Value::I64(*value as i64)),
            Some(Value::I64(value)) => Ok(Value::I64(*value)),
            Some(other) => Err(NyarRuntimeError::TypeMismatch { expected: "i32", actual: other.type_name().to_string() }),
            None => Ok(Value::I64(0)),
        },
        HostOp::I32Div => {
            let lhs = i32_arg(args, 0)?;
            let rhs = i32_arg(args, 1)?;
            Ok(Value::I32(if rhs == 0 { 0 } else { lhs / rhs }))
        }
        HostOp::I64Add => Ok(Value::I64(i64_arg(args, 0)? + i64_arg(args, 1)?)),
        HostOp::I64Sub => Ok(Value::I64(i64_arg(args, 0)? - i64_arg(args, 1)?)),
        HostOp::I64Mul => Ok(Value::I64(i64_arg(args, 0)? * i64_arg(args, 1)?)),
        HostOp::I64Div => {
            let lhs = i64_arg(args, 0)?;
            let rhs = i64_arg(args, 1)?;
            Ok(Value::I64(if rhs == 0 { 0 } else { lhs / rhs }))
        }
        HostOp::I64Rem => {
            let lhs = i64_arg(args, 0)?;
            let rhs = i64_arg(args, 1)?;
            Ok(Value::I64(if rhs == 0 { 0 } else { lhs % rhs }))
        }
        HostOp::I64Neg => Ok(Value::I64(-i64_arg(args, 0)?)),
        HostOp::I64Eq => Ok(Value::Bool(i64_arg(args, 0)? == i64_arg(args, 1)?)),
        HostOp::I64Ne => Ok(Value::Bool(i64_arg(args, 0)? != i64_arg(args, 1)?)),
        HostOp::I64Lt => Ok(Value::Bool(i64_arg(args, 0)? < i64_arg(args, 1)?)),
        HostOp::I64Le => Ok(Value::Bool(i64_arg(args, 0)? <= i64_arg(args, 1)?)),
        HostOp::I64Gt => Ok(Value::Bool(i64_arg(args, 0)? > i64_arg(args, 1)?)),
        HostOp::I64Ge => Ok(Value::Bool(i64_arg(args, 0)? >= i64_arg(args, 1)?)),
        HostOp::BoolNot => Ok(Value::Bool(!args.first().map(Value::to_bool).unwrap_or(false))),
        HostOp::BoolAnd => {
            Ok(Value::Bool(args.first().map(Value::to_bool).unwrap_or(false) && args.get(1).map(Value::to_bool).unwrap_or(false)))
        }
        HostOp::BoolOr => {
            Ok(Value::Bool(args.first().map(Value::to_bool).unwrap_or(false) || args.get(1).map(Value::to_bool).unwrap_or(false)))
        }
    }
}

fn value_display(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::Bool(value) => value.to_string(),
        Value::I32(value) => value.to_string(),
        Value::I64(value) => value.to_string(),
        Value::F32(value) => value.to_string(),
        Value::F64(value) => {
            if value.fract() == 0.0 && value.is_finite() {
                format!("{:.0}", value)
            }
            else {
                value.to_string()
            }
        }
        Value::String(value) => value.clone(),
        Value::Object(_) => "object".to_string(),
        Value::Coroutine(_) => "coroutine".to_string(),
    }
}

fn i64_arg(args: &[Value], index: usize) -> Result<i64, NyarRuntimeError> {
    match args.get(index) {
        Some(Value::I64(value)) => Ok(*value),
        Some(Value::I32(value)) => Ok(*value as i64),
        Some(other) => Err(NyarRuntimeError::TypeMismatch { expected: "i64", actual: other.type_name().to_string() }),
        None => Ok(0),
    }
}

fn i32_arg(args: &[Value], index: usize) -> Result<i32, NyarRuntimeError> {
    match args.get(index) {
        Some(Value::I32(value)) => Ok(*value),
        Some(Value::I64(value)) => Ok(*value as i32),
        Some(other) => Err(NyarRuntimeError::TypeMismatch { expected: "i32", actual: other.type_name().to_string() }),
        None => Ok(0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_all_seed_host_ops() {
        assert_eq!(parse_host_op("alloc_record"), Some(HostOp::AllocRecord));
        assert_eq!(parse_host_op("i64_add"), Some(HostOp::I64Add));
        assert!(parse_host_op("not_a_real_host_op").is_none());
    }

    #[test]
    fn resolves_non_host_as_external() {
        let import = NyarImport {
            kind: std_data::binary::nyar_ir::NyarImportKind::Function,
            module_name: "guest.mod".into(),
            symbol_name: "whatever".into(),
        };
        assert_eq!(resolve_import(&import).unwrap(), ResolvedImport::External);
    }
}
