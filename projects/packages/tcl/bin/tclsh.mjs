#!/usr/bin/env node --experimental-strip-types
/**
 * `tclsh` 模拟入口。
 *
 * 覆盖原版 tclsh 最常用的调用：`tclsh script.tcl` 与 stdin。
 * 执行本身走 `@nyar/nyar`（legend-vm 基板），本包不实现解释器。
 */

import {readFileSync} from 'node:fs';
import {createRequire} from 'node:module';

import {runHostScript} from '@nyar/nyar';

const require = createRequire(import.meta.url);
const pkg = require('../package.json');

const USAGE = 'usage: tclsh [script-file]';

function fail(message, code = 2) {
    console.error(`tclsh: ${message}`);
    process.exit(code);
}

function resolveSource() {
    const argv = process.argv.slice(2);
    const [script] = argv;

    if (script === '--help' || script === '-h') {
        console.log(USAGE);
        process.exit(0);
    }
    if (script === '--version') {
        console.log(`tclsh (nyar simulation ${pkg.version})`);
        process.exit(0);
    }
    if (script !== undefined && script !== '-' && script.startsWith('-')) {
        fail(`unsupported option: ${script}`);
    }
    if (script === '-') return {source: readFileSync(0, 'utf8')};
    if (script !== undefined) {
        try {
            return {source: readFileSync(script, 'utf8'), file: script};
        }
        catch {
            fail(`couldn't read file "${script}": no such file or directory`, 1);
        }
    }
    if (!process.stdin.isTTY) return {source: readFileSync(0, 'utf8')};
    fail('interactive shell is not supported by the nyar simulation; pass a script file or pipe stdin');
}

const options = resolveSource();
try {
    runHostScript({language: 'tcl', ...options});
}
catch (error) {
    console.error(`tclsh: ${error instanceof Error ? error.message : String(error)}`);
    process.exit(1);
}
