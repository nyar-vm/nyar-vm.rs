//! Task 8.1 + 8.2 集成测试：后端模式匹配最小契约的不变量校验。
//!
//! 这些测试手动构造含 `MirCaseChain` 的 `MirFunction`，调用
//! [`nyar_emitter::validate_pattern_matching_invariants`]，验证：
//! - 合法的 case chain（block 引用齐全、fallthrough 指向下一 arm entry）通过校验；
//! - case chain 引用不存在的 block 时返回
//!   [`nyar_emitter::PatternMatchingContractError::InvalidCaseChainLabel`]；
//! - fallthrough 目标非法时返回
//!   [`nyar_emitter::PatternMatchingContractError::InvalidFallthroughTarget`]。

use nyar_emitter::{PatternMatchingContractError, executable_provider::ExecutableFunction, validate_pattern_matching_invariants};
use nyar_language::{
    MirBlock, MirBlockRef, MirConstant, MirFunction, MirInstruction, MirInstructionKind, MirTerminator,
    mir::ssa::{MirCaseArm, MirCaseChain},
    types::hir::ValkyrieType,
};

/// 构造一个含两条 arm 的合法 `MirCaseChain` 的 `MirFunction`。
///
/// Block 布局：
/// - 0: dispatch / entry
/// - 1: arm0 entry + body
/// - 2: arm1 entry + body
/// - 3: no_match
/// - 4: exit
fn build_valid_case_chain_function() -> MirFunction {
    let blocks = vec![
        MirBlock {
            id: MirBlockRef(0),
            label: "dispatch".to_string(),
            parameters: Vec::new(),
            instructions: Vec::new(),
            terminator: MirTerminator::Jump { target: MirBlockRef(1), arguments: Vec::new() },
        },
        MirBlock {
            id: MirBlockRef(1),
            label: "arm0".to_string(),
            parameters: Vec::new(),
            instructions: Vec::new(),
            terminator: MirTerminator::Jump { target: MirBlockRef(4), arguments: Vec::new() },
        },
        MirBlock {
            id: MirBlockRef(2),
            label: "arm1".to_string(),
            parameters: Vec::new(),
            instructions: Vec::new(),
            terminator: MirTerminator::Jump { target: MirBlockRef(4), arguments: Vec::new() },
        },
        MirBlock {
            id: MirBlockRef(3),
            label: "no_match".to_string(),
            parameters: Vec::new(),
            instructions: Vec::new(),
            terminator: MirTerminator::Jump { target: MirBlockRef(4), arguments: Vec::new() },
        },
        MirBlock {
            id: MirBlockRef(4),
            label: "exit".to_string(),
            parameters: Vec::new(),
            instructions: vec![MirInstruction {
                output: None,
                kind: MirInstructionKind::LoadConstant { constant: MirConstant::Unit, ty: None },
            }],
            terminator: MirTerminator::Return { value: None },
        },
    ];

    let chain = MirCaseChain {
        dispatch_block: MirBlockRef(0),
        first_arm: MirBlockRef(1),
        no_match_block: MirBlockRef(3),
        exit_block: MirBlockRef(4),
        produce_value: false,
        arms: vec![
            MirCaseArm {
                entry_block: MirBlockRef(1),
                check_block: None,
                guard_block: None,
                body_block: MirBlockRef(1),
                next_arm_target: MirBlockRef(2),
                exit_target: MirBlockRef(4),
                fallthrough_target: Some(MirBlockRef(2)),
            },
            MirCaseArm {
                entry_block: MirBlockRef(2),
                check_block: None,
                guard_block: None,
                body_block: MirBlockRef(2),
                next_arm_target: MirBlockRef(3),
                exit_target: MirBlockRef(4),
                fallthrough_target: None,
            },
        ],
    };

    MirFunction {
        symbol: "match_fn".to_string(),
        return_type: ValkyrieType::Unit,
        param_types: Vec::new(),
        value_types: Default::default(),
        entry: MirBlockRef(0),
        values: Vec::new(),
        intrinsic: None,
        suspend_points: Vec::new(),
        frame_layouts: Vec::new(),
        continuations: Vec::new(),
        case_chains: vec![chain],
        #[allow(deprecated)]
        state_machine: None,
        suspend_plan: None,
        state_machine_lowered: true,
        blocks,
        diagnostics: Vec::new(),
    }
}

/// 合法的 case chain 应通过契约校验。
#[test]
fn valid_case_chain_passes_validation() {
    let function = build_valid_case_chain_function();
    let contract: ExecutableFunction = function.into();
    let result = validate_pattern_matching_invariants(&contract);
    assert!(result.is_ok(), "valid case chain should pass validation, got {result:?}");
}

/// case chain 的 arm 引用不存在的 block 时应返回 `InvalidCaseChainLabel`。
#[test]
fn dangling_arm_block_label_is_rejected() {
    let mut function = build_valid_case_chain_function();
    function.case_chains[0].arms[0].body_block = MirBlockRef(99);

    let contract: ExecutableFunction = function.into();
    let error = validate_pattern_matching_invariants(&contract).expect_err("dangling block label should be rejected");
    match error {
        PatternMatchingContractError::InvalidCaseChainLabel { chain_index, arm_index, block } => {
            assert_eq!(chain_index, 0);
            assert_eq!(arm_index, Some(0));
            assert_eq!(block, MirBlockRef(99));
        }
        other => panic!("expected InvalidCaseChainLabel, got {other:?}"),
    }
}

/// case chain 顶层字段引用不存在的 block 时应返回 `InvalidCaseChainLabel`（arm_index 为 None）。
#[test]
fn dangling_chain_block_label_is_rejected() {
    let mut function = build_valid_case_chain_function();
    function.case_chains[0].no_match_block = MirBlockRef(42);

    let contract: ExecutableFunction = function.into();
    let error = validate_pattern_matching_invariants(&contract).expect_err("dangling chain block label should be rejected");
    match error {
        PatternMatchingContractError::InvalidCaseChainLabel { chain_index, arm_index, block } => {
            assert_eq!(chain_index, 0);
            assert_eq!(arm_index, None);
            assert_eq!(block, MirBlockRef(42));
        }
        other => panic!("expected InvalidCaseChainLabel, got {other:?}"),
    }
}

/// arm 的 fallthrough_target 不指向下一 arm 的 entry 时应返回 `InvalidFallthroughTarget`。
#[test]
fn invalid_fallthrough_target_is_rejected() {
    let mut function = build_valid_case_chain_function();
    // arm0 的 fallthrough 应为 Some(MirBlockRef(2))（arm1 的 entry），改为指向 no_match。
    function.case_chains[0].arms[0].fallthrough_target = Some(MirBlockRef(3));

    let contract: ExecutableFunction = function.into();
    let error = validate_pattern_matching_invariants(&contract).expect_err("invalid fallthrough target should be rejected");
    match error {
        PatternMatchingContractError::InvalidFallthroughTarget { chain_index, arm_index, actual, expected } => {
            assert_eq!(chain_index, 0);
            assert_eq!(arm_index, 0);
            assert_eq!(actual, MirBlockRef(3));
            assert_eq!(expected, Some(MirBlockRef(2)));
        }
        other => panic!("expected InvalidFallthroughTarget, got {other:?}"),
    }
}

/// 最后一个 arm 若带 fallthrough_target 应返回 `InvalidFallthroughTarget`（expected 为 None）。
#[test]
fn fallthrough_on_last_arm_is_rejected() {
    let mut function = build_valid_case_chain_function();
    // arm1 是最后一个 arm，不应有 fallthrough；强行设置一个存在的 block 也应被拒。
    function.case_chains[0].arms[1].fallthrough_target = Some(MirBlockRef(4));

    let contract: ExecutableFunction = function.into();
    let error = validate_pattern_matching_invariants(&contract).expect_err("fallthrough on last arm should be rejected");
    match error {
        PatternMatchingContractError::InvalidFallthroughTarget { chain_index, arm_index, actual, expected } => {
            assert_eq!(chain_index, 0);
            assert_eq!(arm_index, 1);
            assert_eq!(actual, MirBlockRef(4));
            assert_eq!(expected, None);
        }
        other => panic!("expected InvalidFallthroughTarget, got {other:?}"),
    }
}
