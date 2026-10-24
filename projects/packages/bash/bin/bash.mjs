#!/usr/bin/env node --experimental-strip-types
/**
 * `bash` 模拟入口。
 *
 * 覆盖原版 bash 最常用的三种调用：`bash script.sh`、`bash -c '命令'`、stdin。
 * 执行本身走 `@nyar/nyar`（legend-vm 基板），本包不实现解释器。
 */

import {readFileSync} from 'node:fs';
import {createRequire} from 'node:module';

import {runHostScript} from '@nyar/nyar';

const require = createRequire(import.meta.url);
const pkg = require('../package.json');

const USAGE = 'usage: bash [options] [script-file]\n  -c <command>  execute a command string\n  -s            read commands from stdin';

function fail(message, code = 2) {
    console.error(`bash: ${message}`);
    process.exit(code);
}

function resolveSource() {
    const argv = process.argv.slice(2);
    let command;
    let readStdin = false;
    let script;

    for (let index = 0; index < argv.length; index += 1) {
        const arg = argv[index];
        if (arg === '--help' || arg === '-h') {
            console.log(USAGE);
            process.exit(0);
        }
        else if (arg === '--version' || arg === '-V') {
            console.log(`bash (nyar simulation ${pkg.version})`);
            process.exit(0);
        }
        else if (arg === '-c') {
            command = argv[index + 1];
            if (command === undefined) fail('option requires an argument: -c');
            index += 1;
        }
        else if (arg === '-s') {
            readStdin = true;
        }
        else if (arg !== '-' && arg.startsWith('-')) {
            fail(`unsupported option: ${arg}`);
        }
        else {
            script = arg;
            break;
        }
    }

    if (command !== undefined) return {source: command};
    if (script !== undefined) {
        try {
            return {source: readFileSync(script, 'utf8'), file: script};
        }
        catch {
            fail(`cannot open ${script}: No such file or directory`, 127);
        }
    }
    if (readStdin || !process.stdin.isTTY) return {source: readFileSync(0, 'utf8')};
    fail('interactive shell is not supported by the nyar simulation; pass a script file, -c, or pipe stdin');
}

const options = resolveSource();
try {
    runHostScript({language: 'bash', ...options});
}
catch (error) {
    console.error(`bash: ${error instanceof Error ? error.message : String(error)}`);
    process.exit(1);
}
