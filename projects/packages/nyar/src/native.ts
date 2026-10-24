import {createRequire} from 'node:module';
import {existsSync} from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';

import {hostPlatformKey} from './platform.ts';
import {nativeBinaryName, platformShort, platformTriple} from './platform-triple.ts';

/** Node 宿主协议 id（与 nyar-napi `HOST_PROTOCOL` 对齐）。 */
export const HOST_PROTOCOL = 'nyar-host/0';

export type HostInfo = {
    hostProtocol: string;
    binding: string;
};

export type PrepareRuntimeOptions = {
    family: string;
    artifact: string;
    classpath?: string;
    entry: string;
    wasiP3?: boolean;
};

export type PreparedRuntimeCommand = {
    family: string;
    command: string;
    args: string[];
};

export type HostScriptOptions = {
    /** 语言 id（`bash` / `lua` / `tcl` …）。 */
    language: string;
    /** 脚本源码。 */
    source: string;
    /** 脚本路径，仅用于诊断。 */
    file?: string;
};

export type HostScriptResult = {
    /** 脚本最终值的字符串表示。 */
    value: string;
};

type NyarNativeAddon = {
    getHostInfo(): HostInfo;
    listRuntimeFamilies(): string[];
    parseRuntimeFamily(value: string): string;
    runHostScript(options: {language: string; source: string; file?: string}): HostScriptResult;
    prepareRuntimeCommand(options: {
        family: string;
        artifact: string;
        classpath?: string;
        entry: string;
        wasiP3?: boolean;
    }): PreparedRuntimeCommand;
    runPreparedCommand(prepared: PreparedRuntimeCommand, cwd?: string): number;
};

const pkgRoot = path.dirname(fileURLToPath(import.meta.url));
const require = createRequire(import.meta.url);

/**
 * Resolve native `.node` via the platform optionalDependency (`@nyar/nyar-<short>`).
 */
export function resolveNativePath(): string {
    const triple = platformTriple();
    const short = platformShort(triple);
    const packageName = `@nyar/nyar-${short}`;
    const binary = nativeBinaryName(triple);
    const candidates: string[] = [];

    try {
        const pkgJson = require.resolve(`${packageName}/package.json`);
        const pkgDir = path.dirname(pkgJson);
        candidates.push(path.join(pkgDir, 'lib', binary));
        candidates.push(path.join(pkgDir, binary));
    } catch {
        /* optional dep not installed */
    }

    candidates.push(path.join(pkgRoot, '..', '..', `nyar-${short}`, 'lib', binary));
    candidates.push(path.join(pkgRoot, '..', '..', `nyar-${short}`, binary));
    candidates.push(path.join(pkgRoot, 'node_modules', packageName, 'lib', binary));

    for (const candidate of candidates) {
        if (existsSync(candidate)) return candidate;
    }

    throw new Error(
        `nyar native addon not found for ${packageName}. Run: node scripts/build.mjs napi\n` +
        `Looked in:\n${candidates.map((c) => `  - ${c}`).join('\n')}\n` +
        `(host ${hostPlatformKey()})`,
    );
}

let nativeAddon: NyarNativeAddon | undefined;

/** 加载当前平台的 nyar-napi 绑定（正交能力面，不含 CLI）。 */
export function loadNative(): NyarNativeAddon {
    if (nativeAddon) return nativeAddon;
    const triple = platformTriple();
    const short = platformShort(triple);
    const packageName = `@nyar/nyar-${short}`;
    nativeAddon = require(resolveNativePath()) as NyarNativeAddon;
    return nativeAddon;
}

export function getHostInfo(): HostInfo {
    return loadNative().getHostInfo();
}

export function listRuntimeFamilies(): string[] {
    return loadNative().listRuntimeFamilies();
}

export function prepareRuntimeCommand(options: PrepareRuntimeOptions): PreparedRuntimeCommand {
    const native = loadNative();
    return native.prepareRuntimeCommand({
        family: options.family,
        artifact: options.artifact,
        classpath: options.classpath,
        entry: options.entry,
        wasiP3: options.wasiP3,
    });
}

export function runPreparedCommand(prepared: PreparedRuntimeCommand, cwd?: string): number {
    return loadNative().runPreparedCommand(prepared, cwd);
}

/** 通过 nyar-napi 的 legend-vm 基板执行宿主脚本。 */
export function runHostScript(options: HostScriptOptions): HostScriptResult {
    return loadNative().runHostScript({language: options.language, source: options.source, file: options.file});
}
