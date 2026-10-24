#!/usr/bin/env node --experimental-strip-types
/**
 * `lua` 模拟入口。
 *
 * 覆盖原版 Lua 解释器最常用的调用：`lua script.lua`、`lua -e '代码'`、`lua -`（stdin）。
 * 执行本身走 `@nyar/nyar`（legend-vm 基板），本包不实现解释器。
 */

import {readFileSync} from 'node:fs';
import {createRequire} from 'node:module';

import {runHostScript} from '@nyar/nyar';

const require = createRequire(import.meta.url);
const pkg = require('../package.json');

const USAGE = 'usage: lua [options] [script [args]]\n  -e <stat>  execute a statement\n  -v         show version\n  -          read the script from stdin';

function fail(message, code = 2) {
    console.error(`lua: ${message}`);
    process.exit(code);
}

function resolveSource() {
    const argv = process.argv.slice(2);
    let statement;
    let script;

    for (let index = 0; index < argv.length; index += 1) {
        const arg = argv[index];
        if (arg === '--help' || arg === '-h') {
            console.log(USAGE);
            process.exit(0);
        }
        else if (arg === '--version' || arg === '-v') {
            console.log(`Lua (nyar simulation ${pkg.version})`);
            process.exit(0);
        }
        else if (arg === '-e') {
            statement = argv[index + 1];
            if (statement === undefined) fail('option requires an argument: -e');
            index += 1;
        }
        else if (arg !== '-' && arg.startsWith('-')) {
            fail(`unsupported option: ${arg}`);
        }
        else {
            script = arg;
            break;
        }
    }

    if (statement !== undefined) return {source: statement};
    if (script === '-') return {source: readFileSync(0, 'utf8')};
    if (script !== undefined) {
        try {
            return {source: readFileSync(script, 'utf8'), file: script};
        }
        catch {
            fail(`cannot open ${script}: No such file or directory`, 1);
        }
    }
    if (!process.stdin.isTTY) return {source: readFileSync(0, 'utf8')};
    fail('interactive interpreter is not supported by the nyar simulation; pass a script file, -e, or pipe stdin');
}

const options = resolveSource();
try {
    runHostScript({language: 'lua', ...options});
}
catch (error) {
    console.error(`lua: ${error instanceof Error ? error.message : String(error)}`);
    process.exit(1);
}
