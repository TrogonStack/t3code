const SIGNAL_PATTERN = /receipt of signal: '(SIG[A-Z0-9]+)'/;
const MAX_CAUSE_DEPTH = 16;

/**
 * Effect reports a child killed by a signal only as an error whose message names it. Walk the
 * cause chain for that message, or for a wrapper that already recorded the signal.
 */
export const findProcessSignal = (cause: unknown): string | undefined => {
  const seen = new Set<unknown>();
  let current = cause;
  for (let depth = 0; depth < MAX_CAUSE_DEPTH; depth += 1) {
    if (typeof current !== "object" || current === null || seen.has(current)) return undefined;
    seen.add(current);
    const { signal, message, cause: next } = current as Record<string, unknown>;
    if (typeof signal === "string" && /^SIG[A-Z0-9]+$/.test(signal)) return signal;
    if (typeof message === "string") {
      const match = SIGNAL_PATTERN.exec(message);
      if (match?.[1] !== undefined) return match[1];
    }
    current = next;
  }
  return undefined;
};

/** Spread into an error constructor to carry a signal found on `cause`. */
export const signalField = (cause: unknown): { readonly signal?: string } => {
  const signal = findProcessSignal(cause);
  return signal === undefined ? {} : { signal };
};
