# panda

`panda` 面向 Python。形态 **借鉴** Vite+（统一入口 + 自带 fmt/lint/check），不是「Python 版 Vite+」。

- CLI： **clap**
- fmt/lint/check：`nyar-language::python`（分析框架插件，无 ruff/black 套壳）
- 包管理：`PackageManager::open_with_manifest_layout` + 产品层读写 `pyproject.toml` / `requirements.txt` /
  `requirements-dev.txt`
- 布局：`panda::project_layout()` → `panda-lock.von` / `PANDA_HOME` / `.panda`（不写 Legion 清单）
- 依赖桶：PM `DependencyBucket` / `add_dev_dependency` / bucket-preserving `update_*`
- 兼容行为：读 `[tool.panda]` / `[tool.uv]` / `[tool.poetry]` / PEP 735 `dependency-groups` / `optional-dependencies.dev`
  ，不按 lockfile 猜
- build/run/test：`ScriptRunner` + stdlib `unittest`（不套壳 uv/poetry/pip/pytest）+ 产品层 `PYTHONPATH`

禁止把 Python / pip / conda 概念写进 `nyar-package-manager`。注册器 id（如 `conda`）由 panda 在产品层选择。

**Guard：** `src/architecture_guards.rs`（`cargo test -p panda --lib`）扫描 `src`，防止 `uv` / `pip` / `ruff` 等
`Command::new` 回潮。

## Traditional pip layout

Panda expects classic trees:

| Marker                        | Role                                                                  |
|-------------------------------|-----------------------------------------------------------------------|
| `pyproject.toml` (PEP 621)    | Preferred; `[project.dependencies]` + `[tool.panda] dev-dependencies` |
| `requirements.txt`            | Runtime deps when no pyproject                                        |
| `requirements-dev.txt`        | Dev deps (also `requirements_dev.txt`)                                |
| `src/<pkg>/` or flat `<pkg>/` | Importable package; `panda create` scaffolds **src layout**           |

### Install disk layout vs `.venv`

**Strategy (honest smallest slice):** keep PM installs under `vendors/{registry}/{name}@{version}` — do **not**
materialize a pip-style `.venv` / `site-packages` inside the package manager.

- `panda install` prints this layout and a `PYTHONPATH` hint.
- `panda run` / `test` / `build` set `PYTHONPATH` to: project root → `src/` (if present) → each vendor package root →
  any pre-existing `PYTHONPATH`.
- `.venv/` remains user-owned (scaffolded `.gitignore` ignores it); panda does not create or sync into it.

### Dev-dep read priority (pyproject)

1. `[tool.panda] dev-dependencies`
2. `[dependency-groups] dev` (PEP 735)
3. `[project.optional-dependencies] dev`
4. Poetry `group.dev` / legacy `dev-dependencies` (fallback when panda empty)

Saves always write `[tool.panda] dev-dependencies`. On Poetry-layout pyprojects (no PEP 621 `project.dependencies`),
saves also rewrite `[tool.poetry.group.dev.dependencies]` and drop legacy `[tool.poetry.dev-dependencies]`.

## Commands

| Command                               | Behavior                                                                                  |
|---------------------------------------|-------------------------------------------------------------------------------------------|
| `panda create <name>`                 | Scaffold PEP 621 + `src/<pkg>/` + `tests/` (unittest) + `scripts/build.py` + `.gitignore` |
| `panda install [--dev]`               | Install from translated manifest via PM → `vendors/`                                      |
| `panda add <pkg> [--version] [--dev]` | Install one + save native manifest                                                        |
| `panda remove <pkg>`                  | Remove + save native manifest                                                             |
| `panda update [pkg]`                  | Update + save native manifest                                                             |
| `panda fmt` / `lint` / `check`        | First-party Python format/lint                                                            |
| `panda build`                         | Run `scripts/build.py` via ScriptRunner (+ `PYTHONPATH`)                                  |
| `panda run` / `exec`                  | `python` / `python -m` via ScriptRunner (+ `PYTHONPATH`)                                  |
| `panda test`                          | `python -m unittest discover` (defaults to `-s tests` when present)                       |

## Gaps (explicit)

- No full pip resolver parity (`-r` includes, `--hash`, markers evaluation, conflict solving).
- `-e` editable / VCS URLs in requirements are skipped, not installed.
- No `.venv` / `site-packages` materialization or `pip install -e .`.
- Conda/PyPI artifact shapes inside `vendors/` may still need package-specific import roots.
- Poetry-layout saves rewrite `group.dev` (and migrate off legacy `dev-dependencies`); PEP 621 projects keep panda-only
  dev writes.
- When both `pyproject.toml` and `requirements.txt` exist, **pyproject wins** for load/save.
