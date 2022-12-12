# Workspace Mode

## Overview

Workspace mode is the multi-project management mode for the Valkyrie language, defining workspace configurations through the `legions.json` file. When a `legions.json` file exists in the project root directory, that directory is recognized as being in workspace mode.

## File Structure

```
workspace-root/
├── legions.json          # Workspace configuration file
├── project-a/
│   ├── legion.json       # Project A configuration
│   ├── library/
│   │   └── _.vk         # Recommended: Namespace root definition
│   └── binary/
│       └── main.vk      # Registered as command `main`
├── project-b/
│   ├── legion.json       # Project B configuration
│   ├── library/
│   │   └── _.vk         # Library code directory
│   └── binary/
│       ├── tool1.vk     # Registered as command `tool1`
│       └── tool2/
│           └── _.vk     # Registered as command `tool2`
└── shared/
    └── common/
        └── _.vk
```

## legions.json Configuration

`legions.json` is a configuration file in JSON5 format, supporting comments and more flexible syntax:

```json5
{
    // Basic Workspace Information
    "name": "valkyrie-workspace",
    "version": "1.0.0",
    "description": "Valkyrie language workspace",
    
    // Private workspace flag
    "private": true,
    
    // List of member projects
    "members": [
        "projects/*",
        "tools/build-tools"
    ],
    
    // Excluded directories
    "exclude": [
        "legacy/*",
        "experiments/*",
        "temp/*"
    ],
    
    // Default members (for quick building)
    "default-members": [
        "projects/valkyrie-core",
        "projects/valkyrie-std"
    ],
    
    // Workspace-level scripts
    "scripts": {
        "build": "legion build --release",
        "test": "legion test --release",
        "fmt": "legion fmt --all",
        "clean": "legion clean",
        "publish": "git push && git push --tags --prune",
        "upgrade": "legion upgrade --workspace"
    },
    
    // Shared dependency configuration
    "dependencies": {
        "shared": {
            "serde": "^1.0",
            "tokio": "^1.0"
        }
    },
    
    // Build configuration
    "build": {
        "profile": {
            "release": {
                "lto": true,
                "opt-level": "s"
            }
        }
    }
}
```

## Semantic Features

### 1. Project Discovery

- **Automatic Discovery**: Automatically discover subprojects based on `members` patterns.
- **Explicit Exclusion**: Exclude unwanted directories via `exclude`.

### 2. Property Inheritance

Workspaces support an automatic property inheritance mechanism. If member projects do not explicitly define certain metadata, they will automatically inherit it from the `legions.json` in the workspace root.

#### Workspace Configuration (`legions.json`)
```json5
{
    "name": "my-workspace",
    "version": "1.2.3",
    "description": "A unified workspace",
    "authors": ["Valkyrie Team"],
    "license": "MIT",
    "members": ["projects/*"],
    "dependencies": {
        "shared_lib": "1.0.0"
    }
}
```

#### Member Project Configuration (`projects/my-pkg/legion.json`)
Common metadata (such as version, authors, license, etc.) is **automatically inherited**, requiring no extra configuration:

```json5
{
    "name": "my-pkg",
    // version is automatically inherited as 1.2.3
    // description is automatically inherited as "A unified workspace"
    
    "dependencies": {
        // Dependencies are not automatically inherited and must be explicitly declared
        // To use the version defined by the workspace, simply set it to true
        "shared_lib": true
    }
}
```

Automatically inherited properties include:
- `version`
- `description`
- `authors`
- `license`
- `repository`
- `homepage`
- `edition`

### 3. Dependency Sharing

- **Direct Reference**: Packages within the workspace can directly `using` each other by name.
- **Dependency Declaration**: Although dependency versions can be inherited from the workspace, each package must still explicitly list the dependencies it uses in its `legion.json`.
- **Dependency Promotion**: Dependencies defined in `legions.json` act as a shared pool, referenced by member projects via `true`.
- **Recursive Search**: Supports wildcard patterns for recursive project discovery.

### 4. Dependency Management

- **Shared Dependencies**: Define shared dependency versions at the workspace level.
- **Version Unification**: Ensure all member projects use consistent dependency versions.
- **Dependency Resolution**: Optimize dependency resolution and build caching.

### 5. Build Coordination

- **Parallel Building**: Support parallel building of member projects.
- **Incremental Building**: Intelligently detect changes and build only necessary projects.
- **Build Order**: Automatically determine build order based on dependencies.

### 6. Script Execution

- **Workspace Scripts**: Script commands executed at the workspace level.
- **Batch Operations**: Execute the same operation on all member projects.
- **Conditional Execution**: Execute scripts conditionally based on project status.

## Inter-Project Dependencies

### Internal Dependencies

```json5
// project-a/legion.json
{
    "dependencies": {
        "project-b": { "path": "../project-b" },
        "shared-utils": true
    }
}
```

### Dependency Resolution Rules

1. **Path Dependencies**: Use relative paths to reference other member projects.
2. **Workspace Dependencies**: Use `true` to reference workspace-level dependencies.
3. **Version Constraints**: Support version ranges and exact version constraints.

## Development Workflow

### 1. Initializing a Workspace

```bash
# Create a new workspace
mkdir my-workspace
cd my-workspace

# Initialize legions.json
echo '{ "private": true, "members": ["projects/*"] }' > legions.json
```

### 2. Adding Member Projects

```bash
# Create a new project
mkdir projects/my-project
cd projects/my-project

# Initialize project configuration
echo '{ "name": "my-project", "version": "0.1.0" }' > legion.json
```

### 3. Building and Testing

```bash
# Build all projects
valkyrie build

# Test all projects
valkyrie test

# Build a specific project
valkyrie build --package my-project

# Run binary programs
v tool1                    # Runs binary/tool1.vk
v tool2                    # Runs binary/tool2/_.vk
```

## Best Practices

### 1. Project Organization

- **Logical Grouping**: Organize projects by function or layer.
- **Clear Naming**: Use consistent project naming conventions.
- **Complete Documentation**: Provide complete documentation for each project.

### 2. Dependency Management

- **Version Locking**: Lock critical dependency versions at the workspace level.
- **Minimal Dependencies**: Avoid introducing unnecessary dependencies.
- **Regular Updates**: Regularly update and review dependencies.

### 3. Build Optimization

- **Cache Utilization**: Make full use of the build cache.
- **Parallel Building**: Properly configure parallel build parameters.
- **Incremental Building**: Optimize code structure to support incremental building.

## Tool Integration

### IDE Support

- **Project Import**: IDEs automatically recognize and import workspace structures.
- **Code Navigation**: Cross-project code navigation and reference finding.
- **Debugging Support**: Unified debugging and running configurations.

### CI/CD Integration

- **Build Matrix**: Support build matrices for multiple projects.
- **Test Reports**: Aggregated test results and coverage reports.
- **Deployment Coordination**: Coordinate deployment processes for multiple projects.

## Migration Guide

### From Single Project to Workspace

1. **Create legions.json**: Create workspace configuration in the root directory.
2. **Reorganize Project Structure**: Move existing code to subproject directories.
3. **Update Dependencies**: Adjust dependencies between projects.
4. **Test Build**: Verify building and testing with the new structure.

### Compatibility Considerations

- **Backward Compatibility**: Maintain compatibility with the existing toolchain.
- **Incremental Migration**: Support incremental migration of projects.
- **Tool Adaptation**: Ensure development tools correctly identify the new structure.
