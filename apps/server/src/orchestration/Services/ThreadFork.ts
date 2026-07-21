/**
 * ThreadForkService - Service interface for forking a thread.
 *
 * Owns the "fork a thread" flow used when a `thread.fork` command is
 * dispatched: resolves the source thread, generates a summary or attempts a
 * native provider fork depending on the requested mode, and finalizes the
 * new thread via the internal `thread.fork.finalize` command.
 *
 * Uses Effect `Context.Service` for dependency injection.
 *
 * @module ThreadForkService
 */
import type { OrchestrationCommand, OrchestrationDispatchCommandError } from "@t3tools/contracts";
import * as Context from "effect/Context";
import type * as Effect from "effect/Effect";

/**
 * ThreadForkShape - Service API for dispatching a thread fork.
 */
export interface ThreadForkShape {
  /**
   * Dispatch a `thread.fork` command: resolves the source thread, generates
   * a summary (summary mode) or attempts a native provider fork
   * (full-history mode), then dispatches the internal
   * `thread.fork.finalize` command to create the new thread.
   *
   * @param command - `thread.fork` command from the client command bus.
   * @returns Effect containing the sequence of the persisted `thread.forked` event.
   */
  readonly dispatchFork: (
    command: Extract<OrchestrationCommand, { type: "thread.fork" }>,
  ) => Effect.Effect<{ readonly sequence: number }, OrchestrationDispatchCommandError>;
}

/**
 * ThreadForkService - Service tag for thread fork dispatch.
 *
 * @example
 * ```ts
 * const program = Effect.gen(function* () {
 *   const threadFork = yield* ThreadForkService
 *   return yield* threadFork.dispatchFork(command)
 * })
 * ```
 */
export class ThreadForkService extends Context.Service<ThreadForkService, ThreadForkShape>()(
  "t3/orchestration/Services/ThreadFork/ThreadForkService",
) {}
