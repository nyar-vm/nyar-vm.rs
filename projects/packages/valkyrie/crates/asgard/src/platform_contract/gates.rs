//! 跨平台 UiHost / ASGARD 硬门禁 ID。

/// 硬门禁标识。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GateId {
    /// 制品含 `ASGARDUI` 尾段。
    MagUi,
    /// 制品含 `ASGARDNT` 尾段。
    MagNt,
    /// 禁 `ASGDHOST` / `asgard/Placeholder`。
    NoLegacy,
    /// Android dist 禁 `native/*.c`。
    NoAndroidC,
    /// iOS/desktop 允许薄桥 `.c`（白名单）。
    BridgeC,
    /// runtime 含 `mount` 语义。
    AbiMount,
    /// native→UI `patch` 回写链。
    AbiPatch,
    /// 事件 `on_event` → AOT export。
    AbiEvent,
    /// `awsl_call_*` 命名约定。
    AbiExport,
    /// Android ELF dynsym JNI 符号。
    JniSym,
    /// Android 非 stub Compose 运行时。
    AndroidCompose,
    /// 终端运行时含 `asgard_terminal_mount/patch/on_event` 与 `asgard_invoke_export`。
    TerminalRuntime,
}

impl GateId {
    /// 全部门禁。
    pub const ALL: &[GateId] = &[
        GateId::MagUi,
        GateId::MagNt,
        GateId::NoLegacy,
        GateId::NoAndroidC,
        GateId::BridgeC,
        GateId::AbiMount,
        GateId::AbiPatch,
        GateId::AbiEvent,
        GateId::AbiExport,
        GateId::JniSym,
        GateId::AndroidCompose,
        GateId::TerminalRuntime,
    ];

    /// 稳定字符串 ID（文档 / 测试表驱动）。
    pub fn as_str(self) -> &'static str {
        match self {
            GateId::MagUi => "G-MAG-UI",
            GateId::MagNt => "G-MAG-NT",
            GateId::NoLegacy => "G-NO-LEGACY",
            GateId::NoAndroidC => "G-NO-ANDROID-C",
            GateId::BridgeC => "G-BRIDGE-C",
            GateId::AbiMount => "G-ABI-MOUNT",
            GateId::AbiPatch => "G-ABI-PATCH",
            GateId::AbiEvent => "G-ABI-EVENT",
            GateId::AbiExport => "G-ABI-EXPORT",
            GateId::JniSym => "G-JNI-SYM",
            GateId::AndroidCompose => "G-ANDROID-COMPOSE",
            GateId::TerminalRuntime => "G-TERMINAL-RUNTIME",
        }
    }
}
