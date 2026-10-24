import { type NyarCollect, resolveCollect } from './platform.ts';

export { hostPlatformKey, type NyarCollect, resolveCollect } from './platform.ts';
export { nativeBinaryName, platformShort, platformTriple } from './platform-triple.ts';
export {
    HOST_PROTOCOL,
    getHostInfo,
    listRuntimeFamilies,
    loadNative,
    prepareRuntimeCommand,
    runHostScript,
    runPreparedCommand,
    type HostInfo,
    type HostScriptOptions,
    type HostScriptResult,
    type PrepareRuntimeOptions,
    type PreparedRuntimeCommand,
} from './native.ts';
export { runCli } from './cli/cli.ts';

export type NyarHost = {
    /** Resolved platform collect metadata for diagnostics and tooling. */
    collect: NyarCollect;
    /** Host protocol from nyar-napi. */
    hostProtocol: string;
};

/**
 * Create a Nyar host handle for the current platform.
 *
 * Native collects are preferred when the matching optional dependency is installed;
 * otherwise the Wasm collect from `@nyar/nyar-unknown-wasm32` is used.
 */
export async function createNyarHost(): Promise<NyarHost> {
    const collect = resolveCollect();
    if (collect.kind === 'native') {
        const { getHostInfo } = await import('./native.ts');
        const info = getHostInfo();
        return { collect, hostProtocol: info.hostProtocol };
    }
    return { collect, hostProtocol: 'nyar-host/0' };
}
