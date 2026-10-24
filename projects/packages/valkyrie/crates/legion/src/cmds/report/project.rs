//! Legion 报告工程路径（`valkyrie.v/projects/legion._/projects/legion.report`）。

use std::path::PathBuf;

use asgard::ssg::project::valkyrie_v_roots;
use miette::Result;

/// `valkyrie.v/projects/legion._/projects/legion.report` 工程目录。
pub fn legion_report_project_dir() -> Result<PathBuf> {
    let roots = valkyrie_v_roots()?;
    for root in &roots {
        let nested = root.join("projects/legion._/projects/legion.report");
        if nested.is_dir() && nested.join("legion.von").is_file() {
            return Ok(nested);
        }
    }
    Err(miette::miette!("missing Valyrie project: projects/legion._/projects/legion.report"))
}
