//! 宿主导入能力：加载期把符号解析为 [`HostOp`]，热路径只按枚举分派。

use crate::error::NyarRuntimeError;
use nyar_gc::ObjectHeap;
use nyar_bytecode::NyarImport;

use crate::value::Value;

/// 与 emitter 约定的宿主导入模块名。
pub const HOST_IMPORT_MODULE: &str = "nyar.host";

/// `nyar.host` 上已冻结的宿主操作（稠密枚举，非字符串合同）。
///
/// 结构分配与字段访问不在此枚举：由 `ObjectNew` / `FieldGet` / `FieldSet` 闭合。
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostOp {
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
    /// 进入业务阶段（参数：阶段名 `String`，可选 `i32` 暂停预算毫秒）。
    BeginPhase,
    /// 结束业务阶段（可选参数：阶段名 `String`；缺省弹出栈顶）。
    EndPhase,
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
        "begin_phase" => HostOp::BeginPhase,
        "end_phase" => HostOp::EndPhase,
        _ => return None,
    })
}

/// 执行已解析的宿主操作（热路径无符号字符串）。
pub fn execute_host_op(op: HostOp, args: &[Value], heap: &mut ObjectHeap) -> Result<Value, NyarRuntimeError> {
    match op {
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
        HostOp::BeginPhase => {
            let phase = match args.first() {
                Some(Value::String(name)) if !name.is_empty() => name.clone(),
                Some(other) => {
                    return Err(NyarRuntimeError::TypeMismatch {
                        expected: "non-empty string phase name",
                        actual: other.type_name().to_string(),
                    });
                }
                None => {
                    return Err(NyarRuntimeError::UnsupportedFeature(
                        "begin_phase requires a phase name string argument".into(),
                    ));
                }
            };
            let pause_budget_ms = match args.get(1) {
                Some(Value::I32(value)) if *value >= 0 => Some(*value as u32),
                Some(Value::I64(value)) if *value >= 0 && *value <= u32::MAX as i64 => Some(*value as u32),
                Some(other) => {
                    return Err(NyarRuntimeError::TypeMismatch {
                        expected: "non-negative i32 pause_budget_ms",
                        actual: other.type_name().to_string(),
                    });
                }
                None => None,
            };
            let mut intent = nyar_gc::WorkloadIntent::empty(nyar_gc::IntentSource::PhaseEvent);
            intent.phase = Some(phase);
            intent.pause_budget_ms = pause_budget_ms;
            heap.begin_phase(intent).map_err(|error| NyarRuntimeError::ModuleLoad(error.to_string()))?;
            Ok(Value::Null)
        }
        HostOp::EndPhase => {
            let phase = match args.first() {
                Some(Value::String(name)) => Some(name.as_str()),
                Some(other) => {
                    return Err(NyarRuntimeError::TypeMismatch {
                        expected: "string phase name",
                        actual: other.type_name().to_string(),
                    });
                }
                None => None,
            };
            heap.end_phase(phase).map_err(|error| NyarRuntimeError::ModuleLoad(error.to_string()))?;
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
        assert!(parse_host_op("alloc_record").is_none(), "struct ops must not revive string host bridge");
        assert_eq!(parse_host_op("print"), Some(HostOp::Print));
        assert_eq!(parse_host_op("i64_add"), Some(HostOp::I64Add));
        assert_eq!(parse_host_op("begin_phase"), Some(HostOp::BeginPhase));
        assert_eq!(parse_host_op("end_phase"), Some(HostOp::EndPhase));
        assert!(parse_host_op("not_a_real_host_op").is_none());
    }

    #[test]
    fn begin_and_end_phase_update_heap_strategy() {
        let mut heap = ObjectHeap::new();
        execute_host_op(HostOp::BeginPhase, &[Value::String("request".into()), Value::I32(5)], &mut heap).expect("begin");
        assert_eq!(heap.strategy().phase_depth(), 1);
        assert_eq!(heap.policy().hints.phase.as_deref(), Some("request"));
        assert_eq!(heap.policy().hints.pause_budget_ms, Some(5));
        execute_host_op(HostOp::EndPhase, &[Value::String("request".into())], &mut heap).expect("end");
        assert_eq!(heap.strategy().phase_depth(), 0);
    }

    #[test]
    fn resolves_non_host_as_external() {
        let import = NyarImport {
            kind: nyar_bytecode::NyarImportKind::Function,
            module_name: "guest.mod".into(),
            symbol_name: "whatever".into(),
        };
        assert_eq!(resolve_import(&import).unwrap(), ResolvedImport::External);
    }
}
