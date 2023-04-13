//! 并发标记状态机骨架（WP13）。
//!
//! 默认关闭：尚未启动后台线程，也未改写 mutator 写屏障协议。
//! 本模块只固定合法状态转移，供后续 SATB / 增量更新实验挂接。

/// 并发标记阶段（单 mutator + 后台 collector 第一版）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConcurrentMarkState {
    /// 空闲：无进行中的并发周期。
    #[default]
    Idle,
    /// 已请求开始标记。
    StartMark,
    /// 正在快照根。
    RootSnapshot,
    /// 后台并发遍历。
    ConcurrentTrace,
    /// 终止检测（drain 缓冲 / 检查灰色对象）。
    TerminationCheck,
    /// 最终重新标记。
    Remark,
    /// 清扫。
    Sweep,
}

/// 驱动状态机的事件。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConcurrentMarkEvent {
    /// 启动新周期。
    BeginCycle,
    /// 根快照完成。
    RootsReady,
    /// 并发遍历一轮完成。
    TraceSliceDone,
    /// 终止条件满足。
    TerminationOk,
    /// 终止条件未满足，继续遍历。
    TerminationRetry,
    /// 重新标记完成。
    RemarkDone,
    /// 清扫完成。
    SweepDone,
    /// 强制中止并回到空闲（测试 / 失败路径）。
    Abort,
}

/// 非法状态转移。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConcurrentMarkError {
    /// 当前状态。
    pub from: ConcurrentMarkState,
    /// 事件。
    pub event: ConcurrentMarkEvent,
}

impl std::fmt::Display for ConcurrentMarkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "illegal concurrent-mark transition: {:?} + {:?}", self.from, self.event)
    }
}

impl std::error::Error for ConcurrentMarkError {}

/// 并发标记控制器（无线程；仅协议）。
#[derive(Debug, Default)]
pub struct ConcurrentMarkController {
    state: ConcurrentMarkState,
    /// 产品路径必须保持 `false`，直到写屏障与别名合同落地。
    enabled: bool,
    cycles_started: u64,
}

impl ConcurrentMarkController {
    /// 默认关闭的控制器。
    pub fn new() -> Self {
        Self::default()
    }

    /// 是否已启用（当前恒可查询；启用不自动开线程）。
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// 显式启用协议实验（仍无后台线程）。
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.state = ConcurrentMarkState::Idle;
        }
    }

    /// 当前状态。
    pub fn state(&self) -> ConcurrentMarkState {
        self.state
    }

    /// mutator 写路径是否必须记录 SATB（仅在已启用且处于并发标记可见阶段）。
    pub fn requires_satb(&self) -> bool {
        self.enabled
            && matches!(
                self.state,
                ConcurrentMarkState::ConcurrentTrace
                    | ConcurrentMarkState::TerminationCheck
                    | ConcurrentMarkState::Remark
            )
    }

    /// 已启动周期计数（含中止）。
    pub fn cycles_started(&self) -> u64 {
        self.cycles_started
    }

    /// 尝试状态转移。
    pub fn transition(&mut self, event: ConcurrentMarkEvent) -> Result<ConcurrentMarkState, ConcurrentMarkError> {
        if !self.enabled && !matches!(event, ConcurrentMarkEvent::Abort) {
            return Err(ConcurrentMarkError { from: self.state, event });
        }
        let next = match (self.state, event) {
            (ConcurrentMarkState::Idle, ConcurrentMarkEvent::BeginCycle) => {
                self.cycles_started = self.cycles_started.saturating_add(1);
                ConcurrentMarkState::StartMark
            }
            (ConcurrentMarkState::StartMark, ConcurrentMarkEvent::RootsReady) => ConcurrentMarkState::RootSnapshot,
            (ConcurrentMarkState::RootSnapshot, ConcurrentMarkEvent::TraceSliceDone) => ConcurrentMarkState::ConcurrentTrace,
            (ConcurrentMarkState::ConcurrentTrace, ConcurrentMarkEvent::TraceSliceDone) => ConcurrentMarkState::ConcurrentTrace,
            (ConcurrentMarkState::ConcurrentTrace, ConcurrentMarkEvent::TerminationOk) => ConcurrentMarkState::TerminationCheck,
            (ConcurrentMarkState::TerminationCheck, ConcurrentMarkEvent::TerminationRetry) => ConcurrentMarkState::ConcurrentTrace,
            (ConcurrentMarkState::TerminationCheck, ConcurrentMarkEvent::TerminationOk) => ConcurrentMarkState::Remark,
            (ConcurrentMarkState::Remark, ConcurrentMarkEvent::RemarkDone) => ConcurrentMarkState::Sweep,
            (ConcurrentMarkState::Sweep, ConcurrentMarkEvent::SweepDone) => ConcurrentMarkState::Idle,
            (_, ConcurrentMarkEvent::Abort) => ConcurrentMarkState::Idle,
            (from, event) => return Err(ConcurrentMarkError { from, event }),
        };
        self.state = next;
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_controller_rejects_begin() {
        let mut ctrl = ConcurrentMarkController::new();
        assert!(!ctrl.enabled());
        assert!(ctrl.transition(ConcurrentMarkEvent::BeginCycle).is_err());
    }

    #[test]
    fn happy_path_reaches_idle() {
        let mut ctrl = ConcurrentMarkController::new();
        ctrl.set_enabled(true);
        ctrl.transition(ConcurrentMarkEvent::BeginCycle).unwrap();
        ctrl.transition(ConcurrentMarkEvent::RootsReady).unwrap();
        ctrl.transition(ConcurrentMarkEvent::TraceSliceDone).unwrap();
        ctrl.transition(ConcurrentMarkEvent::TraceSliceDone).unwrap();
        ctrl.transition(ConcurrentMarkEvent::TerminationOk).unwrap();
        ctrl.transition(ConcurrentMarkEvent::TerminationOk).unwrap();
        ctrl.transition(ConcurrentMarkEvent::RemarkDone).unwrap();
        ctrl.transition(ConcurrentMarkEvent::SweepDone).unwrap();
        assert_eq!(ctrl.state(), ConcurrentMarkState::Idle);
        assert_eq!(ctrl.cycles_started(), 1);
    }

    #[test]
    fn abort_returns_to_idle() {
        let mut ctrl = ConcurrentMarkController::new();
        ctrl.set_enabled(true);
        ctrl.transition(ConcurrentMarkEvent::BeginCycle).unwrap();
        ctrl.transition(ConcurrentMarkEvent::Abort).unwrap();
        assert_eq!(ctrl.state(), ConcurrentMarkState::Idle);
    }

    #[test]
    fn satb_required_only_during_mutator_visible_mark() {
        let mut ctrl = ConcurrentMarkController::new();
        assert!(!ctrl.requires_satb());
        ctrl.set_enabled(true);
        ctrl.transition(ConcurrentMarkEvent::BeginCycle).unwrap();
        assert!(!ctrl.requires_satb());
        ctrl.transition(ConcurrentMarkEvent::RootsReady).unwrap();
        ctrl.transition(ConcurrentMarkEvent::TraceSliceDone).unwrap();
        assert!(ctrl.requires_satb());
        assert_eq!(ctrl.state(), ConcurrentMarkState::ConcurrentTrace);
    }
}
