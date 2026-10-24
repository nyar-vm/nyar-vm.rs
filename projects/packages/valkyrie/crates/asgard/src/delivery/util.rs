//! 交付工具函数。

use std::{
    fs::{self, File},
    io::Read,
    path::Path,
};

use miette::{IntoDiagnostic, Result};

pub(crate) fn read_optional_file(path: &Path) -> Result<Option<Vec<u8>>> {
    if !path.is_file() {
        return Ok(None);
    }
    let mut bytes = Vec::new();
    File::open(path).into_diagnostic()?.read_to_end(&mut bytes).into_diagnostic()?;
    Ok(Some(bytes))
}

pub(crate) fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst).into_diagnostic()?;
    for entry in fs::read_dir(src).into_diagnostic()? {
        let entry = entry.into_diagnostic()?;
        let path = entry.path();
        let dest = dst.join(entry.file_name());
        if path.is_dir() {
            copy_dir_recursive(&path, &dest)?;
        }
        else {
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent).into_diagnostic()?;
            }
            fs::copy(&path, &dest).into_diagnostic()?;
        }
    }
    Ok(())
}

pub(crate) fn parse_plist_bundle_name(plist: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(plist);
    let key = "<key>CFBundleName</key>";
    let start = text.find(key)? + key.len();
    let after = text[start..].trim_start();
    let open = after.find("<string>")? + "<string>".len();
    let close = after[open..].find("</string>")? + open;
    Some(after[open..close].trim().to_string())
}
