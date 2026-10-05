//! 并发标记后台节拍线程。
//!
//! **不访问堆**：只递增原子计数，供 mutator 在 safepoint 读取后推进
//! [`crate::ConcurrentMarkController`]。`ConcurrentTrace` 会按节拍增量在 mutator
//! 侧多跑有界灰切片；真实对象扫描仍只在协作点完成，避免与 `ObjectHeap`
//! 可变借用并发。

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

/// 后台节拍：周期唤醒并 `ticks += 1`，默认不启动。
#[derive(Debug, Default)]
pub struct ConcurrentMarkTicker {
    stop: Arc<AtomicBool>,
    ticks: Arc<AtomicU64>,
    join: Option<JoinHandle<()>>,
}

impl ConcurrentMarkTicker {
    /// 未启动的 ticker。
    pub fn new() -> Self {
        Self::default()
    }

    /// 是否已有后台线程。
    pub fn running(&self) -> bool {
        self.join.is_some() && !self.stop.load(Ordering::Acquire)
    }

    /// 已产生的节拍次数（自启动累计）。
    pub fn ticks(&self) -> u64 {
        self.ticks.load(Ordering::Acquire)
    }

    /// 启动后台节拍（幂等：已运行则忽略）。
    ///
    /// `interval` 为两次节拍间隔；过短会空转，过长拖慢 ConcurrentTrace 推进。
    pub fn start(&mut self, interval: Duration) {
        if self.join.is_some() {
            return;
        }
        self.stop.store(false, Ordering::Release);
        let stop = Arc::clone(&self.stop);
        let ticks = Arc::clone(&self.ticks);
        self.join = Some(thread::spawn(move || {
            while !stop.load(Ordering::Acquire) {
                thread::sleep(interval);
                if stop.load(Ordering::Acquire) {
                    break;
                }
                ticks.fetch_add(1, Ordering::Release);
            }
        }));
    }

    /// 请求停止并 join（可重复调用）。
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(handle) = self.join.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for ConcurrentMarkTicker {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ticker_increments_while_running() {
        let mut ticker = ConcurrentMarkTicker::new();
        assert!(!ticker.running());
        ticker.start(Duration::from_millis(5));
        assert!(ticker.running());
        thread::sleep(Duration::from_millis(40));
        let n = ticker.ticks();
        assert!(n >= 1, "expected at least one tick, got {n}");
        ticker.stop();
        assert!(!ticker.running());
        let after = ticker.ticks();
        thread::sleep(Duration::from_millis(20));
        assert_eq!(ticker.ticks(), after, "ticks must freeze after stop");
    }
}
