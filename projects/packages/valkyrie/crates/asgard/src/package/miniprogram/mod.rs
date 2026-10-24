//! 微信小程序 dist 打包：`app.json`、页面四件套。

mod app_json;
mod page_files;

pub use app_json::{MpAppJsonOutput, generate_app_json};
pub use page_files::{MpPackageOutput, package_miniprogram_pages};
