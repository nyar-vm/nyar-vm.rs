/**
 * `@nyar/bash` —— Bash 语言包。
 *
 * 只声明语言身份，宿主能力全部委托给 `@nyar/nyar`；lexer / parser / interpret
 * 属于 `examples/rusty-bash`，本包不重复实现。
 */

import path from 'node:path';
import {fileURLToPath} from 'node:url';

/** Bash 语言身份。 */
export const LANGUAGE = {
    /** Nyar 语言 id。 */
    id: 'bash',
    /** 源文件扩展名。 */
    extensions: ['.sh', '.bash'],
} as const;

const packageRoot = fileURLToPath(new URL('..', import.meta.url));

/** 该语言 guest 模块的约定路径（`guest/bash.nyar`，由 Nyar 构建管线产出）。 */
export function guestArtifact(): string {
    return path.join(packageRoot, 'guest', `${LANGUAGE.id}.nyar`);
}

export {createNyarHost, resolveCollect} from '@nyar/nyar';
export type {NyarCollect, NyarHost} from '@nyar/nyar';
