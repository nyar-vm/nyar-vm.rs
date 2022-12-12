# Package Management and Symbol Resolution Mechanism

This document introduces Valkyrie's package management mechanism, focusing on the distinction between Valkyrie's internal symbol resolution and the external package management introduced by Legion, and providing integration guides for custom package managers.

## 1. Core Concepts Distinction

In Valkyrie, "package management" is divided into two levels:

### 1.1 Internal Symbol Resolution
This is part of the compiler core (HIR stage). It handles `namespace` and `using` statements in the source code and binds names to specific symbols (such as functions, classes, constants).

- **Responsibilities**: Establish symbol tables, handle scopes, and verify access permissions.
- **Core Data Structures**: `GlobalSymbol`, `NamespaceMap`.
- **Resolution Logic**: When the compiler encounters `using A::B`, it looks for a top-level namespace named `A` and locates `B` within it.
- **Limitations**: The internal mechanism itself does not know how to load files from disk that have not yet been recognized by the compiler. It relies on an externally provided "file system view."

### 1.2 Legion Package Management (External Package Management)
Legion is the default package manager for Valkyrie. It exists outside the compiler core (usually driven by LSP or CLI) and is responsible for mapping the physical file system to the compiler's logical view. Legion supports two operating modes:

#### Single Project Mode
Driven by `legion.json` in the project root directory.
- **Applicable Scenarios**: Independent libraries or applications.
- **Core Logic**: Legion uses the directory where `legion.json` is located as the root, parses its `dependencies` field, and manages the project's private `vendor` cache (if it exists).
- **Identification**: `name` and `version` are defined in the configuration file as the top-level namespace for the package in the symbol system.

#### Workspace Mode
Driven by `legions.json` (note the plural) in the root directory.
- **Applicable Scenarios**: Large projects containing multiple interrelated packages (Monorepo).
- **Core Logic**: 
    - `legions.json` defines the workspace member paths (`members`).
    - Member packages can refer to each other directly by package name without declaring relative path dependencies in their respective `legion.json`.
    - The workspace shares a unified `vendor` directory to avoid duplicate downloads of the same dependencies.
- **Property Inheritance**: Member packages can automatically inherit common properties (such as `version`, `description`, `authors`, `license`, etc.) from `legions.json`, avoiding duplicate declarations in sub-projects.
- **Dependency Promotion**: Workspace-level dependency definitions serve as a fallback for all member packages, known as "workspace dependencies" in the resolution order. Member packages explicitly declare the use of the workspace-defined version by setting the dependency value to `true`.

- **Responsibilities**: Discover workspace projects, resolve dependency versions, manage the `vendor` directory, and provide mapping from package names to physical paths.
- **Dependency Resolution Order**:
    1.  **Current Project**: Check files within the current project first.
    2.  **Project Dependencies**: Check direct dependencies defined in `legion.json`.
    3.  **Workspace Dependencies**: Check shared dependencies defined in `legions.json`.
    4.  **Vendor Cache**: Recursively search for matching versions in the `%LEGION_ROOT%/vendor/` directory.

---

## 2. Collaboration Flow: From Using to File Loading

When a user writes `using my_pkg::utils` in the code, the collaboration flow is as follows:

1.  **Symbol Resolution Request**: The compiler attempts to find `my_pkg` in the currently known namespaces.
2.  **Trigger Package Lookup**: If `my_pkg` is not found, the compiler (or LSP state manager) requests `LegionManager` to resolve the package name.
3.  **Path Location**: `LegionManager` finds the root directory of `my_pkg` on disk based on the resolution order (e.g., `vendor/github.com/user/my_pkg`).
4.  **On-Demand Indexing**: The LSP retrieves all `.vk` source files in that directory and compiles them into the symbol table.
5.  **Symbol Binding**: Once the package is loaded, the internal symbol resolution mechanism can successfully find `utils` and complete the binding.

---

## 3. File Architecture Examples

### 3.1 Single Project Mode Layout
```text
my_project/
├── legion.json          # Required: Project metadata and dependency definitions
├── library/             # Core: Library source code directory
│   ├── _.vk             # Recommended: Namespace root definition
│   └── utils.vk         # Submodule (corresponds to namespace my_project::utils)
├── binary/              # Run: Executable command directory
│   ├── simple.vk        # Registered as command `simple`
│   └── complex/
│       └── main.vk      # Registered as command `complex`
├── script/              # Auxiliary: Script directory (registers commands but doesn't install)
│   └── setup.vk         # Registered as command `setup`
├── test/                # Test: Unit test code directory
├── vendor/              # Cache: Private dependency cache for this project
└── tests/               # Recommended: Integration test code
```

### 3.2 Workspace Mode Layout
```text
my_workspace/
├── legions.json         # Required: Workspace member management and shared dependencies
├── vendor/              # Recommended: Dependency cache shared by all workspace members
├── projects/
│   ├── core/            # Member project A
│   │   ├── legion.json
│   │   └── library/ ...
│   └── app/             # Member project B
│       ├── legion.json
│       └── library/ ...
```

---

## 4. Legion Directory Structure Specification

Legion enforces the following directory structure to ensure consistency and assigns different behaviors based on directory type:

### 4.1 Core Directory Behaviors
- **`library/`**: Stores core library code. If it exists, Legion will only load `.vk` or `.valkyrie` files from this directory as library members (the latter is the full extension, but the shorthand `.vk` is recommended).
- **`binary/`**: Stores executable programs.
    - Files like `binary/simple.vk` or `binary/simple.valkyrie` are automatically registered as commands with the same name as the file (`simple`).
    - Directories like `binary/complex/main.vk` are registered as commands with the same name as the directory (`complex`).
    - These commands are deployed to the system's executable path during project installation (`install`).
- **`script/`**: Stores auxiliary scripts.
    - Behavior is similar to `binary/`, and files are registered as commands.
    - **Important Difference**: Script commands are for local development only and are not installed during the project installation (`install`) process.
- **`test/`**: Stores unit test and integration test code.

### 4.2 Namespace and State Sharing
All code located in `library/`, `binary/`, `script/`, and `test/` **shares the same top-level namespace** in development mode (Dev Mode).

This means:
- Code in `binary` or `script` can directly `using` symbols defined in the same project's `library` without additional configuration.
- They share the project's `dev` state environment during compilation, ensuring that development tools (like LSP) can provide consistent symbol navigation and completion experiences.

### 4.3 Other Directories
- **`vendor/`**: Stores external dependencies. Supports multiple levels of nesting (e.g., `vendor/github.com/owner/repo`).

---

## 5. Custom Package Manager Integration Guide

If you wish to implement an alternative package manager to Legion for Valkyrie (e.g., integrating into an existing build system), please follow these guidelines:

### 5.1 Implement Path Mapping Interface
Your package manager must be able to answer the following questions:
-   Given a package name, where is its physical root directory?
-   Given a package directory, which files are its source files?
-   Which packages does the current workspace contain?

### 5.2 Integrate with LSP State
In `ServerState`, you need to replace or extend `LegionManager`. Key integration points include:
-   **Workspace Scanning**: Inform the LSP of all available packages when `set_workspace_root` is called.
-   **File Watching**: Watch for specific configuration files of your package manager and notify the LSP to refresh when they change.
-   **Asynchronous Resolution**: Ensure your path lookup logic is efficient and preferably supports asynchrony to avoid blocking the LSP's main response loop.

### 5.3 Naming Convention Recommendations
-   Package names should be globally unique (using reverse domain names or organization prefixes is recommended).
-   Version numbers should follow the Semantic Versioning (SemVer) specification.
-   The logical entry point of a package should always be clear to avoid circular dependencies.
