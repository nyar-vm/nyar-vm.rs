//! DELETED route — do not restore IntrinsicOpcode as Semantic MIR / Call authority (ADR 0010/0011).
//!
//! Array/std ops lower via first-class InstructionKind (ArrayGet/Set/Length/…) or
//! Invoke → std adaptor → BackendPrivatePlan. This module remains only so historical
//! `use …::intrinsic_opcode` paths fail closed at the type layer.

#![allow(dead_code)]
