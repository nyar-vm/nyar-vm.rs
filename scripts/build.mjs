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
import { copyFileSync, existsSync, mkdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");

/** @typedef {{ triple: string, packageDir: string, tool: "cargo" | "zigbuild" }} NativeTarget */
/** @typedef {{ triple: string, packageDir: string, libName: string, destName: string }} WasmTarget */

const NATIVE_TARGETS = [
    { triple: "x86_64-pc-windows-msvc", packageDir: "nyar-win32-x64", tool: "cargo" },
    { triple: "x86_64-unknown-linux-musl", packageDir: "nyar-linux-x64", tool: "zigbuild" },
    { triple: "x86_64-apple-darwin", packageDir: "nyar-darwin-x64", tool: "zigbuild" },
    { triple: "aarch64-apple-darwin", packageDir: "nyar-darwin-arm64", tool: "zigbuild" },
];

const WASM_TARGETS = [
    {
        triple: "wasm32-wasip1",
        packageDir: "nyar-wasm32-wasi",
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
 * @param {string} triple
 * @param {string} libName
 */
function nativeCollectDestName(triple, libName) {
    if (triple.includes("windows")) {
        return `${libName}.dll`;
    }
    if (triple.includes("apple")) {
        return `lib${libName}.dylib`;
    }
    return `lib${libName}.so`;
}

/**
 * @param {{ all: boolean, debug: boolean }} opts
 */
function buildNapi(opts) {
    const release = !opts.debug;
    const targets = selectNativeTargets(NATIVE_TARGETS, opts.all);
    console.log(`build:napi → nyar-napi cdylib (${release ? "release" : "debug"}, ${targets.length} target(s))`);

    for (const target of targets) {
        console.log(`\n→ ${target.triple} → packages/${target.packageDir}`);
        cargoBuildTarget(target.triple, release, target.tool, "nyar-napi");

        const src = resolveArtifact(nativeLibCandidates(target.triple, release, "nyar_napi"));
        const destName = nativeCollectDestName(target.triple, "nyar_napi");
        const dest = join(ROOT, "packages", target.packageDir, destName);
        copyArtifact(src, dest);
        console.log(`  copied ${destName}`);
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
        console.log(`\n→ ${target.triple} → packages/${target.packageDir}`);
        cargoBuildTarget(target.triple, release, "cargo", "nyar-wasm");

        const src = resolveArtifact(wasmLibCandidates(target.triple, release, target.libName));
        const dest = join(ROOT, "packages", target.packageDir, target.destName);
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
