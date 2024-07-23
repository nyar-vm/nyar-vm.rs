//! 当前 HIR lowering 绑定的源码文本（单线程、单次 compile 作用域）。

use std::cell::RefCell;

thread_local! {
    static HIR_LOWER_SOURCE: RefCell<Option<String>> = RefCell::new(None);
}

/// 在 `compile_source` 进入 HIR lowering 前绑定源码。
pub(crate) fn set_hir_lower_source(source: &str) {
    HIR_LOWER_SOURCE.with(|cell| *cell.borrow_mut() = Some(source.to_string()));
}

/// 离开 HIR lowering 后清除绑定。
pub(crate) fn clear_hir_lower_source() {
    HIR_LOWER_SOURCE.with(|cell| *cell.borrow_mut() = None);
}

/// 读取当前绑定的源码副本。
pub(crate) fn hir_lower_source() -> Option<String> {
    HIR_LOWER_SOURCE.with(|cell| cell.borrow().clone())
}

/// 在单次 HIR lowering 作用域内绑定源码。
pub(crate) fn with_hir_lower_source<R>(source: &str, f: impl FnOnce() -> R) -> R {
    set_hir_lower_source(source);
    let result = f();
    clear_hir_lower_source();
    result
}
