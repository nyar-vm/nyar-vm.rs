//! Panda product [`ProjectLayout`] (lockfile / home paths).

use nyar_package_manager::ProjectLayout;

fn no_entry_aliases(_: &str) -> &'static [&'static str] {
    &[]
}

/// Panda on-disk layout: product lockfile / home; native deps stay in pyproject / requirements.
///
/// `package_manifest` is unused while panda opens via [`PackageManager::open_with_manifest_layout`]
/// with an in-memory translation (PM must not write a VON package file for panda projects).
pub const fn project_layout() -> ProjectLayout {
    ProjectLayout {
        package_manifest: "package.von",
        workspace_manifest: "workspace.von",
        ignore_file: ".packageignore",
        lockfile: "panda-lock.von",
        home_dirname: ".panda",
        home_env: "PANDA_HOME",
        token_env_vars: &[],
        entry_aliases: no_entry_aliases,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_uses_panda_lockfile_and_home() {
        let layout = project_layout();
        assert_eq!(layout.lockfile, "panda-lock.von");
        assert_eq!(layout.home_dirname, ".panda");
        assert_eq!(layout.home_env, "PANDA_HOME");
        assert_ne!(layout.lockfile, "legion-lock.von");
        assert_ne!(layout.home_env, "VALKYRIE_HOME");
    }
}
