import { createRequire } from 'node:module';
import { arch, platform } from 'node:os';
import { fileURLToPath } from 'node:url';

const HOST_KEY = `${platform()}-${arch()}`;

const OPTIONAL_NATIVE_PACKAGES: Record<string, string> = {
    'win32-x64': '@nyar/nyar-win32-x64',
    'linux-x64': '@nyar/nyar-linux-x64',
    'darwin-x64': '@nyar/nyar-darwin-x64',
    'darwin-arm64': '@nyar/nyar-darwin-arm64',
};

const WASM_PACKAGE = '@nyar/nyar-unknown-wasm32';

export type NyarCollectKind = 'native' | 'wasm';

export type NyarCollect = {
    kind: NyarCollectKind;
    packageName: string;
    entry: string;
};

export function hostPlatformKey(): string {
    return HOST_KEY;
}

export function resolveNativeCollectPackage(): string | undefined {
    return OPTIONAL_NATIVE_PACKAGES[HOST_KEY];
}

export function resolveWasmCollectPackage(): string {
    return WASM_PACKAGE;
}

function requireOptional(packageName: string): { resolve(request: string): string } | undefined {
    const require = createRequire(fileURLToPath(import.meta.url));
    try {
        require.resolve(packageName);
        return createRequire(require.resolve(`${packageName}/package.json`));
    } catch {
        return undefined;
    }
}

export function resolveCollect(): NyarCollect {
    const nativePackage = resolveNativeCollectPackage();
    if (nativePackage) {
        const req = requireOptional(nativePackage);
        if (req) {
            const entry =
                HOST_KEY === 'win32-x64'
                    ? req.resolve('./nyar_napi.dll')
                    : req.resolve(`./libnyar_napi.${HOST_KEY.startsWith('darwin') ? 'dylib' : 'so'}`);
            return { kind: 'native', packageName: nativePackage, entry };
        }
    }

    const wasmReq = requireOptional(WASM_PACKAGE);
    if (!wasmReq) {
        throw new Error(`missing required dependency ${WASM_PACKAGE}; install workspace collects before publishing`);
    }
    return {
        kind: 'wasm',
        packageName: WASM_PACKAGE,
        entry: wasmReq.resolve(`${WASM_PACKAGE}/nyar.wasm`),
    };
}
