// @effect-diagnostics globalTimers:off -- A libuv getaddrinfo cannot be cancelled, so the lookup deadline is a plain timer cleared on every completion path.
import * as NodeDns from "node:dns";
import * as NodeModule from "node:module";
import * as NodeNet from "node:net";

import type * as Undici from "undici";

// undici is external to the CLI bundle; Node single-executables can load it from disk only with require.
const requireUndici = NodeModule.createRequire(import.meta.url);

/** Node 26 bundles h2-capable undici 8; Node 22/24 and Electron stay on HTTP/1.1 with 6/7. */
export const builtinFetchUsesHttp2 = (bundledUndiciVersion: string | undefined) =>
  Number.parseInt(bundledUndiciVersion ?? "", 10) >= 8;

const LOOKUP_TIMEOUT_MS = 3_000;

type LookupCallback = (
  error: NodeJS.ErrnoException | null,
  address?: string | Array<NodeDns.LookupAddress>,
  family?: number,
) => void;
type Lookup = (hostname: string, options: NodeDns.LookupOptions, callback: LookupCallback) => void;
type ResolveCallback = (error: Error | null, addresses?: Array<string>) => void;
type ResolverLike = {
  readonly resolve4: (hostname: string, callback: ResolveCallback) => void;
  readonly resolve6: (hostname: string, callback: ResolveCallback) => void;
};

const notFound = (hostname: string): NodeJS.ErrnoException =>
  Object.assign(new Error(`getaddrinfo ENOTFOUND ${hostname}`), {
    code: "ENOTFOUND",
    syscall: "getaddrinfo",
    hostname,
  });

const wantedFamily = (family: NodeDns.LookupOptions["family"]): 0 | 4 | 6 =>
  family === 4 || family === "IPv4" ? 4 : family === 6 || family === "IPv6" ? 6 : 0;

/**
 * getaddrinfo has no timeout and every hung call permanently holds one of the four libuv
 * threadpool threads (macOS mDNSResponder can stop answering one process for good). Bound the
 * wait, then answer from c-ares, which does its own socket I/O and uses no pool thread.
 */
export function makeBoundedLookup(
  options: {
    readonly timeoutMs?: number;
    readonly lookup?: Lookup;
    readonly resolver?: ResolverLike;
  } = {},
): NodeNet.LookupFunction {
  const timeoutMs = options.timeoutMs ?? LOOKUP_TIMEOUT_MS;
  const lookup = options.lookup ?? (NodeDns.lookup as unknown as Lookup);
  let resolver = options.resolver;

  return ((
    hostname: string,
    lookupOptions: NodeDns.LookupOptions | number,
    callback: LookupCallback,
  ) => {
    const normalized: NodeDns.LookupOptions =
      typeof lookupOptions === "number" ? { family: lookupOptions } : (lookupOptions ?? {});
    let settled = false;
    const deliver = (
      error: NodeJS.ErrnoException | null,
      addresses: Array<NodeDns.LookupAddress>,
    ) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      if (error !== null) return callback(error);
      if (normalized.all) return callback(null, addresses);
      const first = addresses[0];
      return first === undefined
        ? callback(notFound(hostname))
        : callback(null, first.address, first.family);
    };

    const timer = setTimeout(() => {
      if (settled) return;
      resolver ??= new NodeDns.Resolver({ timeout: 2_000, tries: 2 });
      const dns = resolver;
      const family = wantedFamily(normalized.family);
      const versions: Array<4 | 6> = family === 0 ? [4, 6] : [family];
      const collected: Partial<Record<4 | 6, Array<NodeDns.LookupAddress>>> = {};
      let pending = versions.length;
      for (const version of versions) {
        const onResult: ResolveCallback = (error, found) => {
          collected[version] =
            error || !found ? [] : found.map((address) => ({ address, family: version }));
          if (--pending > 0) return;
          const all = [...(collected[4] ?? []), ...(collected[6] ?? [])];
          deliver(all.length === 0 ? notFound(hostname) : null, all);
        };
        if (version === 4) dns.resolve4(hostname, onResult);
        else dns.resolve6(hostname, onResult);
      }
    }, timeoutMs);
    timer.unref();

    lookup(hostname, normalized, (error, address, resolvedFamily) => {
      if (error) return deliver(error, []);
      deliver(
        null,
        Array.isArray(address)
          ? address
          : [{ address: address ?? "", family: resolvedFamily ?? 4 }],
      );
    });
  }) as NodeNet.LookupFunction;
}

/** Preserves the EnvHttpProxyAgent installed by --use-env-proxy / NODE_USE_ENV_PROXY. */
export function installHttp1FetchDispatcher(
  options: { readonly lookup?: NodeNet.LookupFunction } = {},
): void {
  const undici = requireUndici("undici") as typeof Undici;
  const connectOptions =
    options.lookup === undefined ? {} : { connect: { lookup: options.lookup } };
  // Node's agent comes from its internal undici copy, so `instanceof` against the npm class is false.
  const usesEnvProxy = undici.getGlobalDispatcher().constructor.name === "EnvHttpProxyAgent";
  undici.setGlobalDispatcher(
    usesEnvProxy
      ? new undici.EnvHttpProxyAgent({
          allowH2: false,
          ...connectOptions,
          ...(options.lookup === undefined ? {} : { proxyTls: { lookup: options.lookup } }),
          // ProxyAgent's connector forces h2 and ignores allowH2; replace it for HTTPS proxies.
          clientFactory: (origin) => new undici.Pool(origin, { allowH2: false }),
        })
      : new undici.Agent({ allowH2: false, ...connectOptions }),
  );
}

/**
 * Avoids uncaught idle-timeout errors on h2 streams (https://github.com/nodejs/undici/issues/5936),
 * and on macOS bounds name resolution so a hung getaddrinfo cannot starve the libuv threadpool.
 */
export function keepOutboundFetchOnHttp1(
  // oxlint-disable-next-line t3code/no-global-process-runtime -- Runs before ACP dispatch without loading the Effect graph.
  platform = process.platform,
): void {
  const bounded = platform === "darwin";
  if (bounded || builtinFetchUsesHttp2(process.versions.undici)) {
    installHttp1FetchDispatcher(bounded ? { lookup: makeBoundedLookup() } : {});
  }
}

// @types/node does not declare setTypeOfService yet.
type TypeOfServiceSocket = {
  setTypeOfService?: (this: NodeNet.Socket, typeOfService: number) => NodeNet.Socket;
};

// Bundled undici calls this from a socket callback without catching. On macOS,
// EINVAL after a peer reset must not exit the server over an optional QoS hint.
export function ignoreMacosTypeOfServiceEinval(
  // oxlint-disable-next-line t3code/no-global-process-runtime -- Runs before ACP dispatch without loading the Effect graph.
  platform = process.platform,
  socketPrototype: TypeOfServiceSocket = NodeNet.Socket.prototype as TypeOfServiceSocket,
): void {
  if (platform !== "darwin") return;
  const setTypeOfService = socketPrototype.setTypeOfService;
  if (typeof setTypeOfService !== "function") return;
  socketPrototype.setTypeOfService = function (...args) {
    try {
      return setTypeOfService.apply(this, args);
    } catch (error) {
      if (error instanceof Error && "code" in error && error.code === "EINVAL") return this;
      throw error;
    }
  };
}

/**
 * Node's 250 ms Happy Eyeballs attempt timeout abandons a working-but-slow IPv4
 * connect when IPv6 is advertised but unroutable, which broke the ACP registry
 * fetch. A larger `--network-family-autoselection-attempt-timeout` still wins.
 */
export function extendConnectionAttemptTimeout(): void {
  NodeNet.setDefaultAutoSelectFamilyAttemptTimeout(
    Math.max(NodeNet.getDefaultAutoSelectFamilyAttemptTimeout(), 1_000),
  );
}
