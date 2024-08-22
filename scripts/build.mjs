#!/usr/bin/env node
/**
 * Nyar 平台 collect 构建入口（`nyar-napi` / `nyar-wasm` 均为纯 cdylib，无 bin）。
 *
 * Usage:
 *   node scripts/build.mjs              # napi + wasm
 *   node scripts/build.mjs napi
 *   node scripts/build.mjs wasm
 *   node scripts/build.mjs --all
 *   node scripts/build.mjs napi --debug
 */

import { spawnSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readdirSync, unlinkSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");

/** @typedef {{ triple: string, packageDir: string, tool: "cargo" | "zigbuild", npmTriple: string, short: string }} NativeTarget */
/** @typedef {{ triple: string, packageDir: string, libName: string, destName: string }} WasmTarget */

const NATIVE_TARGETS = [
    {
        triple: "x86_64-pc-windows-msvc",
        packageDir: "nyar-win32-x64",
        tool: "cargo",
        npmTriple: "win32-x64-msvc",
        short: "win32-x64",
    },
    {
        triple: "x86_64-unknown-linux-musl",
        packageDir: "nyar-linux-x64",
        tool: "zigbuild",
        npmTriple: "linux-x64-gnu",
        short: "linux-x64",
    },
    {
        triple: "x86_64-apple-darwin",
        packageDir: "nyar-darwin-x64",
        tool: "zigbuild",
        npmTriple: "darwin-x64",
        short: "darwin-x64",
    },
    {
        triple: "aarch64-apple-darwin",
        packageDir: "nyar-darwin-arm64",
        tool: "zigbuild",
        npmTriple: "darwin-arm64",
        short: "darwin-arm64",
    },
];

const WASM_TARGETS = [
    {
        triple: "wasm32-unknown-unknown",
        packageDir: "nyar-unknown-wasm32",
        libName: "nyar_wasm",
        destName: "nyar_wasm.wasm",
    },
];

const MODES = new Set(["napi", "wasm"]);

/**
 * @param {string[]} argv
 */
function parseArgs(argv) {
    const flags = argv.filter((arg) => arg.startsWith("-"));
    const positionals = argv.filter((arg) => !arg.startsWith("-"));
    const mode = positionals.find((arg) => MODES.has(arg)) ?? "all";
    return {
        mode,
        all: flags.includes("--all"),
        debug: flags.includes("--debug"),
    };
}

/**
 * @param {string} command
 * @param {string[]} args
 */
function run(command, args) {
    const result = spawnSync(command, args, {
        cwd: ROOT,
        stdio: "inherit",
        shell: process.platform === "win32",
        env: process.env,
    });
    if ((result.status ?? 1) !== 0) {
        process.exit(result.status ?? 1);
    }
}

function hostTriple() {
    if (process.platform === "win32") {
        return "x86_64-pc-windows-msvc";
    }
    if (process.platform === "darwin") {
        return process.arch === "arm64" ? "aarch64-apple-darwin" : "x86_64-apple-darwin";
    }
    return "x86_64-unknown-linux-gnu";
}

/**
 * @param {NativeTarget[]} targets
 * @param {boolean} all
 */
function selectNativeTargets(targets, all) {
    if (all) {
        return targets;
    }
    // CI job can force a specific target regardless of host architecture,
    // e.g. arm64 runner cross-compiling x86_64-apple-darwin with Xcode clang.
    const forced = process.env.NYAR_NATIVE_TARGET;
    if (forced) {
        const hit = targets.find((t) => t.short === forced || t.triple === forced);
        if (hit) {
            return [hit];
        }
        console.warn(`NYAR_NATIVE_TARGET=${forced} did not match any native target; falling back to host selection`);
    }
    const host = hostTriple();
    const exact = targets.find((target) => target.triple === host);
    if (exact) {
        return [exact];
    }
    if (process.platform === "linux") {
        const linux = targets.find((target) => target.packageDir === "nyar-linux-x64");
        if (linux) {
            return [{ ...linux, triple: host }];
        }
    }
    return targets.slice(0, 1);
}

/**
 * 本机 runner 上优先 cargo；仅跨平台本地打包才走 zigbuild。
 * @param {{ triple: string, packageDir: string, tool: "cargo" | "zigbuild" }} target
 * @param {boolean} release
 * @param {string} crate
 */
function resolveNativeBuild(target) {
    // macOS: all Apple targets (arm64 native + x86_64 cross) use cargo + Xcode clang.
    // Rust Tier 2 x86_64-apple-darwin + Xcode toolchain is sufficient — no zigbuild needed.
    // NYAR_NATIVE_TARGET picks the correct package; this function just picks tool + triple.
    if (process.platform === "darwin") {
        return { triple: target.triple, tool: "cargo" };
    }
    const host = hostTriple();
    if (process.platform === "win32" && target.packageDir === "nyar-win32-x64") {
        return { triple: host, tool: "cargo" };
    }
    if (process.platform === "linux" && target.packageDir === "nyar-linux-x64") {
        return { triple: host, tool: "cargo" };
    }
    return { triple: target.triple, tool: target.tool };
}

/**
 * @param {string} triple
 * @param {boolean} release
 * @param {"cargo" | "zigbuild"} tool
 * @param {string} crate
 */
function cargoBuildTarget(triple, release, tool, crate) {
    const profile = release ? ["--release"] : [];
    if (tool === "zigbuild") {
        run("cargo", ["zigbuild", "build", ...profile, "--target", triple, "-p", crate]);
        return;
    }
    run("cargo", ["build", ...profile, "--target", triple, "-p", crate]);
}

/**
 * @param {string} triple
 * @param {boolean} release
 * @param {string} libName
 */
function nativeLibCandidates(triple, release, libName) {
    const profile = release ? "release" : "debug";
    const dir = join(ROOT, "target", triple, profile);
    if (triple.includes("windows")) {
        return [join(dir, `${libName}.dll`)];
    }
    if (triple.includes("apple")) {
        return [join(dir, `lib${libName}.dylib`)];
    }
    return [join(dir, `lib${libName}.so`), join(dir, `${libName}.so`)];
}

/**
 * @param {string} triple
 * @param {boolean} release
 * @param {string} libName
 */
function wasmLibCandidates(triple, release, libName) {
    const profile = release ? "release" : "debug";
    const dir = join(ROOT, "target", triple, profile);
    return [join(dir, `${libName}.wasm`)];
}

/**
 * @param {string[]} candidates
 */
function resolveArtifact(candidates) {
    for (const candidate of candidates) {
        if (existsSync(candidate)) {
            return candidate;
        }
    }
    throw new Error(`missing build artifact (tried: ${candidates.join(", ")})`);
}

/**
 * @param {string} src
 * @param {string} dest
 */
function copyArtifact(src, dest) {
    mkdirSync(dirname(dest), { recursive: true });
    copyFileSync(src, dest);
}

/**
 * @param {string} npmTriple
 */
function nativeCollectBinaryName(npmTriple) {
    return `nyar.${npmTriple}.node`;
}

/**
 * @param {string} triple
 * @param {boolean} release
 * @param {string} libName
 */
function findNativeArtifact(triple, release, libName) {
    const candidates = nativeLibCandidates(triple, release, libName);
    for (const candidate of candidates) {
        if (existsSync(candidate)) {
            return candidate;
        }
    }
    const profile = release ? "release" : "debug";
    const depsDir = join(ROOT, "target", triple, profile, "deps");
    if (existsSync(depsDir)) {
        for (const name of readdirSync(depsDir)) {
            if (
                (name === libName ||
                    name.startsWith(`${libName}.`) ||
                    name.startsWith(`lib${libName}.`)) &&
                (name.endsWith(".dll") || name.endsWith(".so") || name.endsWith(".dylib") || name.endsWith(".node"))
            ) {
                return join(depsDir, name);
            }
        }
    }
    throw new Error(`missing build artifact (tried: ${candidates.join(", ")})`);
}

/**
 * @param {{ all: boolean, debug: boolean }} opts
 */
function buildNapi(opts) {
    const release = !opts.debug;
    const targets = selectNativeTargets(NATIVE_TARGETS, opts.all);
    console.log(`build:napi → nyar-napi cdylib (${release ? "release" : "debug"}, ${targets.length} target(s))`);

    for (const target of targets) {
        const build = resolveNativeBuild(target);
        console.log(`\n→ ${build.triple} (${build.tool}) → projects/packages/${target.packageDir}`);
        cargoBuildTarget(build.triple, release, build.tool, "nyar-napi");

        const src = findNativeArtifact(build.triple, release, "nyar_napi");
        const destName = nativeCollectBinaryName(target.npmTriple);
        const destDir = join(ROOT, "projects", "packages", target.packageDir);
        const dest = join(destDir, "lib", destName);
        copyArtifact(src, dest);
        for (const legacy of ["nyar_napi.dll", "libnyar_napi.so", "libnyar_napi.dylib", "nyar.node", destName]) {
            const legacyPath = join(destDir, legacy);
            if (existsSync(legacyPath)) {
                unlinkSync(legacyPath);
            }
        }
        console.log(`  copied lib/${destName}`);
    }
}

/**
 * @param {{ all: boolean, debug: boolean }} opts
 */
function buildWasm(opts) {
    const release = !opts.debug;
    const targets = opts.all ? WASM_TARGETS : WASM_TARGETS;
    console.log(`build:wasm → nyar-wasm cdylib (${release ? "release" : "debug"}, ${targets.length} target(s))`);

    for (const target of targets) {
        console.log(`\n→ ${target.triple} → projects/packages/${target.packageDir}`);
        cargoBuildTarget(target.triple, release, "cargo", "nyar-wasm");

        const src = resolveArtifact(wasmLibCandidates(target.triple, release, target.libName));
        const dest = join(ROOT, "projects", "packages", target.packageDir, target.destName);
        copyArtifact(src, dest);
        console.log(`  copied ${target.destName}`);
    }
}

const opts = parseArgs(process.argv.slice(2));

if (opts.mode === "napi") {
    buildNapi(opts);
} else if (opts.mode === "wasm") {
    buildWasm(opts);
} else {
    buildNapi(opts);
    buildWasm(opts);
}

console.log("\nbuild complete");
