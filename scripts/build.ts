import { spawnSync } from 'node:child_process';
import { join, resolve, dirname } from 'node:path';
import { copyFileSync, mkdirSync, existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { platform } from 'node:os';

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = resolve(__dirname, '..');

interface Target {
    target: string;
    pkgSuffix: string;
    binName: string;
    // 如果不指定 tool，则根据宿主环境自动选择
    tool?: 'cargo' | 'zigbuild';
    altBin?: string;
}

const commonTargets: Target[] = [
    { target: 'x86_64-pc-windows-msvc', pkgSuffix: 'win32-x64', binName: 'nyar.exe' },
    { target: 'x86_64-unknown-linux-musl', pkgSuffix: 'linux-x64', binName: 'nyar', tool: 'zigbuild' },
    { target: 'x86_64-apple-darwin', pkgSuffix: 'darwin-x64', binName: 'nyar', tool: 'zigbuild' },
    { target: 'aarch64-apple-darwin', pkgSuffix: 'darwin-arm64', binName: 'nyar', tool: 'zigbuild' },
    { target: 'wasm32-wasip1', pkgSuffix: 'wasm32-wasi', binName: 'nyar.wasm', tool: 'cargo', altBin: 'nyar.wasm' },
];

function getTool(t: Target): string {
    if (t.tool) return t.tool;
    
    // 如果目标是 WASM，始终使用 cargo
    if (t.target.includes('wasm')) return 'cargo';

    // 如果目标与当前操作系统匹配，可以使用 cargo，否则必须使用 zigbuild 进行交叉编译
    const isWindows = platform() === 'win32';
    const targetIsWindows = t.target.includes('windows');
    
    if (isWindows && targetIsWindows) {
        return 'cargo';
    }
    
    // 默认使用 zigbuild 进行跨平台构建
    return 'zigbuild';
}

function build(projectName: string) {
    console.log(`\x1b[36m--- Building ${projectName} for all targets ---\x1b[0m`);

    for (const t of commonTargets) {
        const binName = t.binName;
        const altBin = t.altBin;
        const pkgName = `${projectName}-${t.pkgSuffix}`;
        const pkgPath = join(root, 'packages', pkgName);
        const tool = getTool(t);

        console.log(`\x1b[33mBuilding for ${t.target} using ${tool}...\x1b[0m`);

        const args = ['--release', '--target', t.target, '-p', projectName];
        
        if (tool === 'zigbuild') {
            args.unshift('zigbuild');
        } else {
            args.unshift('build');
        }

        const result = spawnSync('cargo', args, { 
            stdio: 'inherit', 
            cwd: root, 
            shell: platform() === 'win32' 
        });

        if (result.status !== 0) {
            console.error(`\x1b[31mBuild failed for ${t.target}\x1b[0m`);
            process.exit(result.status ?? 1);
        }

        let srcBin = join(root, 'target', t.target, 'release', binName);
        if (altBin && !existsSync(srcBin)) {
            srcBin = join(root, 'target', t.target, 'release', altBin);
        }

        if (!existsSync(srcBin)) {
            console.error(`\x1b[31mCould not find built binary at ${srcBin}\x1b[0m`);
            continue;
        }

        if (!existsSync(pkgPath)) {
            mkdirSync(pkgPath, { recursive: true });
        }

        const destBin = join(pkgPath, binName);
        console.log(`\x1b[32mCopying ${binName} to ${pkgName}...\x1b[0m`);
        copyFileSync(srcBin, destBin);
    }
}

const project = process.argv[2] || 'nyar';
build(project);

console.log(`\x1b[36m--- Build Complete ---\x1b[0m`);
