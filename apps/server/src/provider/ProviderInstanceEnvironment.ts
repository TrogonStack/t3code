import type { ProviderInstanceEnvironment } from "@t3tools/contracts";

/**
 * An instance environment after its secret references have been read.
 *
 * `unresolved` names the variables the instance configures but whose value
 * could not be read. They are tracked separately because being absent from
 * `variables` is not enough: the child environment starts from the server's
 * own, so a name left alone keeps whatever the server inherited under it.
 * That is the wrong credential precisely when the user asked for a specific
 * one, and it hides as "authenticated".
 */
export interface ResolvedProviderInstanceEnvironment {
  readonly variables: ProviderInstanceEnvironment | undefined;
  readonly unresolved: ReadonlyArray<string>;
}

export function mergeProviderInstanceEnvironment(
  resolved: ResolvedProviderInstanceEnvironment,
  baseEnv: NodeJS.ProcessEnv = process.env,
): NodeJS.ProcessEnv {
  const { variables, unresolved } = resolved;
  if ((!variables || variables.length === 0) && unresolved.length === 0) {
    return baseEnv;
  }

  const next: NodeJS.ProcessEnv = { ...baseEnv };
  for (const name of unresolved) {
    delete next[name];
  }
  for (const variable of variables ?? []) {
    next[variable.name] = variable.value;
  }
  return next;
}
