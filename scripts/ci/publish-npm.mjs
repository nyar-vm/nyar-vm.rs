/**
 * GitHub Actions：发布 @nyar/* monorepo 真包。
 *
 * - tag vX.Y.Z → version X.Y.Z
 * - 幂等：目标版本已在 registry 则 skip
 * - 不使用 NPM_TOKEN；OIDC Trusted Publisher（permissions.id-token: write）
 * - 工作流合同：file=publish-npm.yml env=NPM_PUBLISH repo=nyar-vm/nyar-vm.rs
 *
 * 前置：native 产物在 NYAR_NATIVE_ARTIFACTS/<short>/；wasm 在 NYAR_WASM_ARTIFACTS/ 或同目录 unknown-wasm32/
 */

import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const PACKAGES_ROOT = path.join(ROOT, 'projects', 'packages');

/** @type {{ short: string, packageDir: string, os: string[], cpu: string[], artifact: string }[]} */
const NATIVE_PLATFORMS = [
    { short: 'win32-x64', packageDir: 'nyar-win32-x64', os: ['win32'], cpu: ['x64'], artifact: 'nyar_napi.dll' },
    { short: 'linux-x64', packageDir: 'nyar-linux-x64', os: ['linux'], cpu: ['x64'], artifact: 'libnyar_napi.so' },
    { short: 'darwin-x64', packageDir: 'nyar-darwin-x64', os: ['darwin'], cpu: ['x64'], artifact: 'libnyar_napi.dylib' },
    {
        short: 'darwin-arm64',
        packageDir: 'nyar-darwin-arm64',
        os: ['darwin'],
        cpu: ['arm64'],
        artifact: 'libnyar_napi.dylib',
    },
];

const WASM_COLLECT = {
    short: 'unknown-wasm32',
    packageDir: 'nyar-unknown-wasm32',
    artifact: 'nyar_wasm.wasm',
};

const MAIN_PACKAGE = {
    dir: path.join(PACKAGES_ROOT, 'nyar'),
    name: '@nyar/nyar',
};

function fail(msg) {
    console.error(`ci-publish-npm: ${msg}`);
    process.exit(1);
}

function run(cmd, args, opts = {}) {
    const r = spawnSync(cmd, args, {
        cwd: opts.cwd ?? ROOT,
        encoding: 'utf8',
        shell: process.platform === 'win32',
        env: opts.env ?? process.env,
        stdio: opts.stdio ?? 'pipe',
    });
    return {
        status: r.status ?? 1,
        stdout: String(r.stdout ?? '').trim(),
        stderr: String(r.stderr ?? '').trim(),
    };
}

function resolveVersion() {
    const fromArg = process.argv.find((a) => a.startsWith('--version='))?.slice('--version='.length);
    if (fromArg) return fromArg.replace(/^v/, '');
    const ref = process.env.GITHUB_REF ?? '';
    const m = ref.match(/^refs\/tags\/(?:placeholder-)?v?(\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?)$/);
    if (m) return m[1];
    fail('need --version=X.Y.Z or GITHUB_REF=refs/tags/vX.Y.Z');
}

function readJson(p) {
    return JSON.parse(fs.readFileSync(p, 'utf8'));
}

function writeJson(p, obj) {
    fs.writeFileSync(p, `${JSON.stringify(obj, null, 2)}\n`);
}

function copyIfExists(from, to) {
    if (!fs.existsSync(from)) return false;
    fs.mkdirSync(path.dirname(to), { recursive: true });
    fs.copyFileSync(from, to);
    return true;
}

function copyTree(src, dest) {
    fs.mkdirSync(dest, { recursive: true });
    for (const name of fs.readdirSync(src)) {
        if (name === 'node_modules' || name === '.git') continue;
        const from = path.join(src, name);
        const to = path.join(dest, name);
        const st = fs.statSync(from);
        if (st.isDirectory()) copyTree(from, to);
        else fs.copyFileSync(from, to);
    }
}

function rewriteWorkspaceDeps(deps, version) {
    if (!deps) return deps;
    /** @type {Record<string, string>} */
    const out = {};
    for (const [k, v] of Object.entries(deps)) {
        if (typeof v === 'string' && (v.startsWith('workspace:') || v === '*')) {
            out[k] = version;
        } else {
            out[k] = v;
        }
    }
    return out;
}

function rewriteDepsField(pkg, version) {
    for (const field of ['dependencies', 'optionalDependencies', 'peerDependencies']) {
        if (pkg[field]) pkg[field] = rewriteWorkspaceDeps(pkg[field], version);
    }
    return pkg;
}

function isAlreadyPublished(blob) {
    return /cannot publish over existing|EPUBLISHCONFLICT|previously published versions|version already exists|cannot publish.*same version|you cannot publish over/i.test(
        blob,
    );
}

function isAuthFailure(blob) {
    return /ENEEDAUTH|Unable to authenticate|not authorized|OIDC|trusted publisher|two-factor|need to be logged|login|identity token|do not have permission to access it|Access token expired or revoked/i.test(
        blob,
    );
}

function isMissingPackage(blob) {
    if (isAuthFailure(blob)) return false;
    return /Package not found|does not exist on the registry|cannot publish.*before creating|This package has not been created|is not in this registry/i.test(
        blob,
    );
}

function versionExists(name, version) {
    const r = run('npm', ['view', `${name}@${version}`, 'version']);
    return r.status === 0 && r.stdout === version;
}

function npmPublish(stagingDir, name, version) {
    const args = ['publish', '--access', 'public'];
    console.log(`\n=== ${name}@${version} npm ${args.join(' ')} ===`);
    const r = run('npm', args, { cwd: stagingDir });
    if (r.stdout) process.stdout.write(`${r.stdout}\n`);
    if (r.stderr) process.stderr.write(`${r.stderr}\n`);
    const blob = `${r.stdout}\n${r.stderr}`;
    if (r.status === 0) return 'published';
    if (isAlreadyPublished(blob) || versionExists(name, version)) return 'exists';
    if (isAuthFailure(blob)) return 'auth';
    if (isMissingPackage(blob)) return 'missing';
    if (versionExists(name, version)) return 'exists';
    console.error(blob.slice(0, 1200));
    return 'other';
}

function stageCollectPackage(spec, version, artifactsRoot) {
    const abs = path.join(PACKAGES_ROOT, spec.packageDir);
    if (!fs.existsSync(abs)) fail(`missing package dir ${spec.packageDir}`);
    const raw = readJson(path.join(abs, 'package.json'));
    const name = raw.name;
    if (!name) fail(`no name in ${spec.packageDir}/package.json`);

    const stage = path.join(os.tmpdir(), `nyar-pub-${spec.short}-${version}`);
    fs.rmSync(stage, { recursive: true, force: true });
    fs.mkdirSync(stage, { recursive: true });

    const artDir = path.join(artifactsRoot, spec.short);
    const artifactName = spec.artifact;
    const fromArtifact = path.join(artDir, artifactName);
    const fromPackage = path.join(abs, artifactName);
    if (!copyIfExists(fromArtifact, path.join(stage, artifactName)) && !copyIfExists(fromPackage, path.join(stage, artifactName))) {
        console.log(` · ${name}: no ${artifactName} in artifacts or package tree — publish metadata-only`);
    }

    for (const doc of ['readme.md', 'README.md']) {
        copyIfExists(path.join(abs, doc), path.join(stage, doc));
    }
    if (!fs.existsSync(path.join(stage, 'README.md')) && fs.existsSync(path.join(stage, 'readme.md'))) {
        fs.copyFileSync(path.join(stage, 'readme.md'), path.join(stage, 'README.md'));
    }

    const pkg = rewriteDepsField({ ...raw }, version);
    pkg.version = version;
    delete pkg.private;
    pkg.publishConfig = { ...(pkg.publishConfig ?? {}), access: 'public' };
    if (pkg.scripts) {
        delete pkg.scripts.prepack;
        delete pkg.scripts.prepare;
        if (Object.keys(pkg.scripts).length === 0) delete pkg.scripts;
    }
    writeJson(path.join(stage, 'package.json'), pkg);
    return { stage, name };
}

function publishCollect(spec, version, artifactsRoot) {
    const { stage, name } = stageCollectPackage(spec, version, artifactsRoot);
    if (versionExists(name, version)) {
        console.log(` ✓ ${name}@${version} already on registry — skip`);
        return 'exists';
    }
    const outcome = npmPublish(stage, name, version);
    if (outcome === 'published') return 'published';
    if (outcome === 'exists') {
        console.log(` ✓ ${name}@${version} already on registry — skip`);
        return 'exists';
    }
    if (outcome === 'auth') {
        fail(`OIDC/auth failed for ${name}. Add Trusted Publisher: file=publish-npm.yml env=NPM_PUBLISH repo=nyar-vm/nyar-vm.rs`);
    }
    if (outcome === 'missing') {
        fail(`${name} missing on registry — run pnpm placeholder:publish, then pnpm placeholder:trust -- --only ${name}`);
    }
    fail(`publish failed for ${name}`);
}

function publishMain(version, artifactsRoot) {
    const abs = MAIN_PACKAGE.dir;
    const raw = readJson(path.join(abs, 'package.json'));
    const name = MAIN_PACKAGE.name;

    if (versionExists(name, version)) {
        console.log(` ✓ ${name}@${version} already on registry — skip`);
        return 'exists';
    }

    const stage = path.join(os.tmpdir(), `nyar-pub-main-${version}`);
    fs.rmSync(stage, { recursive: true, force: true });
    fs.mkdirSync(stage, { recursive: true });

    for (const rel of raw.files ?? ['bin', 'src', 'readme.md', 'README.md']) {
        const from = path.join(abs, rel);
        if (!fs.existsSync(from)) continue;
        const to = path.join(stage, rel);
        if (fs.statSync(from).isDirectory()) copyTree(from, to);
        else {
            fs.mkdirSync(path.dirname(to), { recursive: true });
            fs.copyFileSync(from, to);
        }
    }
    copyIfExists(path.join(abs, 'package.json'), path.join(stage, 'package.json'));
    for (const doc of ['readme.md', 'README.md']) {
        copyIfExists(path.join(abs, doc), path.join(stage, doc));
    }
    if (!fs.existsSync(path.join(stage, 'README.md')) && fs.existsSync(path.join(stage, 'readme.md'))) {
        fs.copyFileSync(path.join(stage, 'readme.md'), path.join(stage, 'README.md'));
    }

    const pkg = rewriteDepsField({ ...raw }, version);
    pkg.name = name;
    pkg.version = version;
    delete pkg.private;
    pkg.publishConfig = { ...(pkg.publishConfig ?? {}), access: 'public' };
    if (pkg.scripts) {
        delete pkg.scripts.prepack;
        delete pkg.scripts.prepare;
        if (Object.keys(pkg.scripts).length === 0) delete pkg.scripts;
    }
    delete pkg.devDependencies;

    /** @type {Record<string, string>} */
    const optionalNatives = {};
    for (const plat of NATIVE_PLATFORMS) {
        const collectName = `@nyar/nyar-${plat.short}`;
        const artDir = path.join(artifactsRoot, plat.short);
        const hasArtifact =
            fs.existsSync(path.join(artDir, plat.artifact)) || fs.existsSync(path.join(PACKAGES_ROOT, plat.packageDir, plat.artifact));
        if (hasArtifact || versionExists(collectName, version)) {
            optionalNatives[collectName] = version;
        } else {
            console.log(` · ${collectName}@${version} not built this release — omit from optionalDependencies`);
        }
    }
    pkg.dependencies = rewriteWorkspaceDeps(pkg.dependencies ?? {}, version);
    pkg.optionalDependencies = { ...rewriteWorkspaceDeps(pkg.optionalDependencies ?? {}, version), ...optionalNatives };

    writeJson(path.join(stage, 'package.json'), pkg);

    const outcome = npmPublish(stage, name, version);
    if (outcome === 'published') return 'published';
    if (outcome === 'exists') {
        console.log(` ✓ ${name}@${version} already on registry — skip`);
        return 'exists';
    }
    if (outcome === 'auth') {
        fail(`OIDC/auth failed for ${name}. Add Trusted Publisher: file=publish-npm.yml env=NPM_PUBLISH repo=nyar-vm/nyar-vm.rs`);
    }
    if (outcome === 'missing') {
        fail(`${name} missing on registry — run pnpm placeholder:publish, then pnpm placeholder:trust -- --only ${name}`);
    }
    fail(`publish failed for ${name}`);
}

const version = resolveVersion();
console.log(`ci-publish-npm: version=${version}`);
console.log(` GITHUB_REF=${process.env.GITHUB_REF ?? '(none)'}`);
console.log(' Trusted Publisher contract: publish-npm.yml + env NPM_PUBLISH\n');

delete process.env.NODE_AUTH_TOKEN;
delete process.env.NPM_TOKEN;

const nativeArtifactsRoot = process.env.NYAR_NATIVE_ARTIFACTS || path.join(ROOT, 'dist', 'native-flat');
const wasmArtifactsRoot = process.env.NYAR_WASM_ARTIFACTS || nativeArtifactsRoot;

let published = 0;
let skipped = 0;

for (const plat of NATIVE_PLATFORMS) {
    const outcome = publishCollect(plat, version, nativeArtifactsRoot);
    if (outcome === 'published') published += 1;
    else skipped += 1;
}

const wasmOutcome = publishCollect(WASM_COLLECT, version, wasmArtifactsRoot);
if (wasmOutcome === 'published') published += 1;
else skipped += 1;

const mainOutcome = publishMain(version, nativeArtifactsRoot);
if (mainOutcome === 'published') published += 1;
else skipped += 1;

console.log(`\nci-publish-npm: done (published=${published} skipped=${skipped})`);
