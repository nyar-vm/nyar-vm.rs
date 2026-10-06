# nyar-package-registry

Pluggable registry adapters for the Nyar package manager.

## Overview

This crate defines the `Registry` trait and ships concrete drivers for npm, JSR, NuGet, Maven, Conda, and Valhalla endpoints. `nyar-package-manager` consumes these adapters through dependency injection; this crate stays free of lockfile parsing and dependency solving.

## Built-in registries

| Key | Type | Default endpoint constant |
|:---|:---|:---|
| `npm` | `NpmRegistry` | `NpmRegistry::DEFAULT_ENDPOINT` |
| `jsr` | `JsrRegistry` | `JsrRegistry::DEFAULT_ENDPOINT` |
| `nuget` | `NugetRegistry` | `NugetRegistry::DEFAULT_ENDPOINT` |
| `maven` | `MavenRegistry` | `MavenRegistry::DEFAULT_ENDPOINT` |
| `conda` | `CondaRegistry` | `CondaRegistry::DEFAULT_ENDPOINT` |
| `valhalla` | `ValhallaRegistry` | product-specific |

`default_registries()` builds the standard set. `registries_with_endpoints` overrides URLs from a `BTreeMap<String, String>`.

## `Registry` contract

Each adapter implements metadata fetch (`get_package`, `search_packages`, `get_package_versions`), tarball publish/download, and token verification. Optional helpers include `discover_credential`, `sync_credential`, `package_exists`, and `get_latest_version`. Shared value types: `Package`, `PublishOptions`, `PublishResult`, `TokenVerifyResult`, `PublisherKey`.

HTTP utilities (`extract_tarball`, `sha256_hex`, `verify_sri`, `proxy_env_hint`) and credential stores live in submodules re-exported at the crate root.

## Usage

```rust
use nyar_package_registry::{default_registries, Registry};

let registries = default_registries()?;
let npm = registries.get("npm").expect("npm adapter");
let pkg = npm.get_package("@scope/name", "1.0.0")?;
```

## Boundaries

- No `legion.toml` / manifest parsing.
- No dependency graph resolution or lockfile IO.
- `MockRegistry` is provided for unit tests in the manager crate.
