#!/usr/bin/env node
import { spawnSync } from 'node:child_process';
import { createRequire } from 'node:module';

import { hostPlatformKey, resolveCollect } from '../src/platform.ts';

const require = createRequire(import.meta.url);
const pkg = require('../package.json');

function printUsage() {
    console.error(`@nyar/nyar ${pkg.version} (${hostPlatformKey()})`);
    console.error('Usage: nyar <command> [args...]');
    console.error('Commands:');
    console.error('  host-info   Print resolved platform collect metadata');
    console.error('  version     Print package version');
}

function cmdHostInfo() {
    const collect = resolveCollect();
    console.log(JSON.stringify({ version: pkg.version, host: hostPlatformKey(), collect }, null, 2));
}

function cmdVersion() {
    console.log(pkg.version);
}

const [command, ...rest] = process.argv.slice(2);

if (!command || command === '--help' || command === '-h') {
    printUsage();
    process.exit(command ? 0 : 1);
}

if (command === 'version' || command === '--version' || command === '-V') {
    cmdVersion();
    process.exit(0);
}

if (command === 'host-info') {
    cmdHostInfo();
    process.exit(0);
}

// Forward unknown commands to wasmtime when the wasm collect is present (maintainer / CI path).
const collect = resolveCollect();
if (collect.kind === 'wasm' && rest.length > 0) {
    const result = spawnSync('wasmtime', [collect.entry, ...rest], { stdio: 'inherit', shell: false });
    process.exit(result.status ?? 1);
}

console.error(`nyar: unknown command '${command}'`);
printUsage();
process.exit(1);
