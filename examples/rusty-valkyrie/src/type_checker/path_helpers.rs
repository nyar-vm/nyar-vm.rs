//! 共享名称路径显示 helper。

use crate::types::{Identifier, NamePath};

pub(super) fn display_path(path: &NamePath) -> String {
    path.parts().iter().map(|part| part.to_string()).collect::<Vec<_>>().join("::")
}

pub(super) fn last_name(path: &NamePath) -> Option<Identifier> {
    path.parts().last().cloned()
}
