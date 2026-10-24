//! 文档源发现。

use std::path::{Path, PathBuf};

use super::nav::DocSection;

/// 标准文档分区（与 C# legion doc 对齐）。
const STANDARD_SECTIONS: &[(&str, &str, &str)] = &[
    ("language", "语言参考", "语言参考"),
    ("guides", "用户指南", "用户指南"),
    ("toolchain", "工具链", "工具链"),
    ("developer", "开发者文档", "开发者文档"),
    ("maintainer", "维护者文档", "维护者文档"),
];

/// 在项目目录中发现文档分区。
pub fn discover_sections(project_dir: &Path) -> Vec<DocSection> {
    let mut sections = Vec::new();
    let zh_hans = project_dir.join("documentation/pages/zh-hans");

    if zh_hans.is_dir() {
        for (dir_name, section_name, sidebar_title) in STANDARD_SECTIONS {
            let sub_dir = zh_hans.join(dir_name);
            if sub_dir.is_dir() {
                sections.push(DocSection {
                    name: (*section_name).to_string(),
                    sidebar_title: (*sidebar_title).to_string(),
                    source_dir: sub_dir,
                    pages: Vec::new(),
                });
            }
        }

        let flat_md: Vec<PathBuf> = collect_md_files(&zh_hans).into_iter().filter(|p| p.parent() == Some(zh_hans.as_path())).collect();
        let has_subdirs = STANDARD_SECTIONS.iter().any(|(dir, _, _)| zh_hans.join(dir).is_dir());
        if !flat_md.is_empty() && !has_subdirs {
            sections.push(DocSection {
                name: "用户文档".to_string(),
                sidebar_title: "用户文档".to_string(),
                source_dir: zh_hans.clone(),
                pages: Vec::new(),
            });
        }
        else if !flat_md.is_empty() {
            sections.push(DocSection {
                name: "文档".to_string(),
                sidebar_title: "文档".to_string(),
                source_dir: zh_hans.clone(),
                pages: Vec::new(),
            });
        }
    }

    sections
}

/// 递归收集目录下所有 `.md` 文件。
pub fn collect_md_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_md_files_inner(dir, &mut files);
    files.sort();
    files
}

fn collect_md_files_inner(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("md") {
            out.push(path);
        }
        else if path.is_dir() {
            collect_md_files_inner(&path, out);
        }
    }
}

/// 判断项目是否包含可渲染文档。
pub fn has_documentation(project_dir: &Path) -> bool {
    !discover_sections(project_dir).is_empty()
}
