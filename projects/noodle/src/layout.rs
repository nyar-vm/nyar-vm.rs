//! Noodle product [`ProjectLayout`] (lockfile / home paths).

use nyar_package_manager::ProjectLayout;

fn no_entry_aliases(_: &str) -> &'static [&'static str] {
    &[]
}

/// Noodle on-disk layout: product-branded lockfile / home, no Node lockfile names.
///
/// Passed to [`PackageManager::open_with_manifest_layout`](nyar_package_manager::PackageManager::open_with_manifest_layout).
pub const fn project_layout() -> ProjectLayout {
    ProjectLayout {
        // Unused: noodle translates `package.json` in-memory rather than loading this file.
        package_manifest: "package.von",
        workspace_manifest: "workspace.von",
        ignore_file: ".packageignore",
        lockfile: "noodle-lock.von",
        home_dirname: ".noodle",
        home_env: "NOODLE_HOME",
        token_env_vars: &[],
        entry_aliases: no_entry_aliases,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_uses_noodle_lockfile_not_npm() {
        let layout = project_layout();
        assert_eq!(layout.lockfile, "noodle-lock.von");
        assert_eq!(layout.home_dirname, ".noodle");
        assert_eq!(layout.home_env, "NOODLE_HOME");
    }
}
