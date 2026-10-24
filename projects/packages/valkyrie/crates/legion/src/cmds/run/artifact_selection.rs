use std::{
    fs,
    path::{Component, Path, PathBuf},
};

pub(crate) fn select_artifact<'entry>(
    output_dir: &Path,
    entries: impl Iterator<Item = &'entry str>,
    artifact_override: Option<&Path>,
) -> Result<(PathBuf, usize), String> {
    let entries = entries.collect::<Vec<_>>();
    if entries.is_empty() {
        return Err("execution manifest has no run contracts".into());
    }
    if artifact_override.is_none() && entries.len() != 1 {
        return Err("execution manifest has multiple run contracts; select one with --artifact".into());
    }
    let root = fs::canonicalize(output_dir).map_err(|error| error.to_string())?;
    let requested = artifact_override.map(fs::canonicalize).transpose().map_err(|error| error.to_string())?;
    let mut selected = None;
    for (index, entry) in entries.into_iter().enumerate() {
        let relative = Path::new(entry);
        if relative.as_os_str().is_empty() || relative.components().any(|component| !matches!(component, Component::Normal(_))) {
            return Err(format!("execution manifest has an invalid artifact path: '{entry}'"));
        }
        let artifact = fs::canonicalize(root.join(relative)).map_err(|error| error.to_string())?;
        if !artifact.starts_with(&root) || !artifact.is_file() {
            return Err("execution manifest artifact is not a file inside the output directory".into());
        }
        if requested.as_ref().is_none_or(|path| path == &artifact) {
            if selected.is_some() {
                return Err("artifact has multiple run contracts".into());
            }
            selected = Some((artifact, index));
        }
    }
    selected.ok_or_else(|| "explicit artifact is not listed by the current execution manifest".into())
}
#[cfg(test)]
#[path = "../../../tests/path_wired/cmds_run_artifact_selection.rs"]
mod cmds_run_artifact_selection_tests;
