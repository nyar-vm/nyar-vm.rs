use crate::nyar_backend_clr::{MsilInstruction, MsilInstructionOperand, MsilMethodRef, MsilMethodSignature, MsilOpcode};
use nyar::QualifiedName;

use crate::{
    FragmentSubmission,
    lowering::{sanitize_operation_symbol, sanitize_symbol},
};

/// 判断给定操作是否为接受命令行参数的 CLI 入口主函数。
///
/// 通过入口操作与 MIR 参数元数据判定，不按操作名特判任何具体入口。
/// 只要该操作是当前提交的入口操作且其 MIR 函数签名包含参数
/// （即接受 `args: [utf8]` 等 CLI 参数），就视为 CLI 入口主函数，
/// 使用 CLI 分发代码生成路径。
pub(crate) fn is_cli_entry_main(submission: &FragmentSubmission, operation: &QualifiedName) -> bool {
    let Some(entry) = submission.entry_operation.as_ref()
    else {
        return false;
    };
    let is_entry = entry == operation
        || (submission.fragment_id.as_str().starts_with("main_") && sanitize_operation_symbol(entry) == sanitize_operation_symbol(operation));
    if !is_entry {
        return false;
    }
    let Some(exec) = submission.executable.as_ref()
    else {
        return false;
    };
    let Some(view) = exec.get_function(operation)
    else {
        return false;
    };
    !view.function.param_types.is_empty()
}

/// 为 CLI 入口生成转发 IL：将 `Main(string[])` 的 args 直接转发给真实入口操作。
///
/// 这是 CLR 后端的入口方法代码生成，仅负责把运行时传入的 args 数组
/// 按入口操作的真实签名（通过 `method_signature_for` 查得的 MIR 元数据）
/// 转发给上层源码定义的 `legion(args: [utf8])` 等入口函数。
/// CLI 业务语义（命令解析、分发、帮助文本）完全由上层 `legion.tools`
/// 源码定义，本函数不做任何命令名特判。
pub(crate) fn lower_entry_with_cli_args(entry_operation: &QualifiedName, signature: MsilMethodSignature) -> Vec<MsilInstruction> {
    let entry_name = sanitize_operation_symbol(entry_operation);
    vec![
        MsilInstruction { label: None, opcode: MsilOpcode::Ldarg0, operand: None },
        MsilInstruction {
            label: None,
            opcode: MsilOpcode::Call,
            operand: Some(MsilInstructionOperand::Method(MsilMethodRef { owner: None, name: entry_name, signature })),
        },
        MsilInstruction { label: None, opcode: MsilOpcode::LdcI4_0, operand: None },
        MsilInstruction { label: None, opcode: MsilOpcode::Ret, operand: None },
    ]
}
