import { readFile } from 'node:fs/promises';

import { type NyarCollect, resolveCollect } from './platform.ts';

export { hostPlatformKey, type NyarCollect, resolveCollect } from './platform.ts';

export type NyarHost = {
    /** Resolved platform collect metadata for diagnostics and tooling. */
    collect: NyarCollect;
    /** Load bytecode bytes (0.0.0 placeholder — execution wiring lands with Node-API host exports). */
    runBytecode(bytes: Uint8Array): Promise<void>;
};

/**
 * Create a Nyar host for the current platform.
 *
 * Native collects are preferred when the matching optional dependency is installed;
 * otherwise the Wasm GC collect from `@nyar/nyar-unknown-wasm32` is used.
 */
export async function createNyarHost(): Promise<NyarHost> {
    const collect = resolveCollect();
    return {
        collect,
        async runBytecode(bytes: Uint8Array) {
            if (bytes.length === 0) {
                throw new Error('nyar host received empty bytecode payload');
            }
            // 0.0.0 ships collect layout only; bytecode execution is wired in the next host export drop.
            await readFile(collect.entry);
            throw new Error(
                `nyar host collect resolved (${collect.kind} via ${collect.packageName}) but bytecode execution is not wired in 0.0.0`,
            );
        },
    };
}

/** @deprecated Use {@link createNyarHost}. */
export const NYAR_CLI_PLACEHOLDER = 'nyar';
