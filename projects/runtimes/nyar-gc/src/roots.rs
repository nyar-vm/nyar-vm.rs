//! 宿主持久根：跨 `run` 调用仍须存活的托管引用。
//!
//! 解释器栈 / 帧 / 全局在单次 `collect` 时由 [`crate::GcRoots`] 传入；
//! 宿主把返回值或长期句柄挂在这里，否则对象会在下一次回收中消失。

use crate::value::Value;

/// 宿主根槽句柄（堆内下标，不跨进程序列化）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RootHandle(usize);

impl RootHandle {
    /// 由槽下标构造（deopt 解码 / 诊断）；不验证槽是否仍固定。
    pub fn from_index(index: usize) -> Self {
        Self(index)
    }

    /// 槽下标（诊断 / 测试）。
    pub fn index(self) -> usize {
        self.0
    }
}

/// 可固定 / 释放的宿主根表。
#[derive(Debug, Default)]
pub struct HostRoots {
    slots: Vec<Option<Value>>,
    free: Vec<usize>,
}

impl HostRoots {
    /// 空表。
    pub fn new() -> Self {
        Self::default()
    }

    /// 固定一个值，使其在后续 `collect` 中成为根。
    pub fn pin(&mut self, value: Value) -> RootHandle {
        if let Some(index) = self.free.pop() {
            self.slots[index] = Some(value);
            return RootHandle(index);
        }
        let index = self.slots.len();
        self.slots.push(Some(value));
        RootHandle(index)
    }

    /// 释放句柄；对应对象若无其它根可达则可被回收。
    pub fn unpin(&mut self, handle: RootHandle) {
        let index = handle.0;
        if index >= self.slots.len() {
            return;
        }
        if self.slots[index].take().is_some() {
            self.free.push(index);
        }
    }

    /// 读取固定值。
    pub fn get(&self, handle: RootHandle) -> Option<&Value> {
        self.slots.get(handle.0).and_then(|slot| slot.as_ref())
    }

    /// 可变读取（例如原地更新根指向）。
    pub fn get_mut(&mut self, handle: RootHandle) -> Option<&mut Value> {
        self.slots.get_mut(handle.0).and_then(|slot| slot.as_mut())
    }

    /// 所有非空根值。
    pub fn iter(&self) -> impl Iterator<Item = &Value> {
        self.slots.iter().filter_map(|slot| slot.as_ref())
    }

    /// 所有非空根值（可变，供晋升后改写引用）。
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Value> {
        self.slots.iter_mut().filter_map(|slot| slot.as_mut())
    }

    /// 当前固定根数量。
    pub fn live_count(&self) -> usize {
        self.slots.iter().filter(|slot| slot.is_some()).count()
    }
}
