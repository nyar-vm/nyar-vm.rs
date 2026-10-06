# nyar-package-manager

Core package manager for the Legion ecosystem: manifests, resolution, lockfiles, cache, publish, and scripts.

## Overview

`nyar-package-manager` orchestrates dependency graphs, on-disk project layout, vendor/auth stores, and registry-backed install/publish flows. Registry wire protocols live in `nyar-package-registry`; this crate focuses on **project state** and **solver behavior**. Product CLIs supply a `ProjectLayout` (manifest filenames, ignore rules, cache roots) rather than hard-coding Legion-specific paths inside the library.

## Features

- **`PackageManager`** — facade for install, query, publish, auth; supports `offline` and `frozen_lockfile` modes.
- **`ProjectMode`** — discovers single-package vs workspace roots from disk + layout.
- **`PackageManifest` / `WorkspaceManifest`** — parsed dependency specs and workspace members.
- **`DependencyResolver` / `LockFile`** — semver constraint solving and reproducible lock entries.
- **`PackageCache`** — downloaded artifact cache keyed by registry layout.
- **`PackagePublisher` / `pack` / `unpack`** — tarball packing and registry publish targets.
- **`RegistrySourceManager`** — merges configured registry endpoints into live `Registry` handles.
- **`VendorManager` / credential discovery** — vendor auth store and token sync helpers.
- **`SecurityAudit`** — license and vulnerability scan hooks for installed graphs.
- **`ScriptRunner`** — runs lifecycle scripts declared in manifests.

## Usage

```rust
use nyar_package_manager::{PackageManager, ProjectLayout};
use nyar_package_registry::default_registries;

let registries = default_registries()?;
let pm = PackageManager::open_with_registries(project_root, registries, ProjectLayout::neutral())?;
```

For tests, inject `MockRegistry` from `nyar_package_registry` via `open_with_registries`.

## Boundaries

- Does not compile source languages (delegates to `nyar-language` and product pipelines).
- Does not implement HTTP for individual registries (uses the `Registry` trait).
- `ProjectLayout::neutral()` is a generic default; Legion/Valkyrie products should pass their own layout.
