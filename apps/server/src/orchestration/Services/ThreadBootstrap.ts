/**
 * ThreadBootstrapService - Service interface for bootstrapping a thread's
 * first turn.
 *
 * Owns the combined "create thread + prepare worktree + run setup script +
 * start the first turn" flow used when a `thread.turn.start` command carries
 * a `bootstrap` payload. This lets callers (websocket RPC today, an MCP
 * toolkit tomorrow) create a thread and kick off its first turn with a
 * single dispatch instead of orchestrating the sub-steps themselves.
 *
 * Uses Effect `Context.Service` for dependency injection.
 *
 * @module ThreadBootstrapService
 */
import type {
  OrchestrationClientOrigin,
  OrchestrationCommand,
  OrchestrationDispatchCommandError,
} from "@t3tools/contracts";
import * as Context from "effect/Context";
import type * as Effect from "effect/Effect";

/**
 * ThreadBootstrapShape - Service API for bootstrapping a thread turn start.
 */
export interface ThreadBootstrapShape {
  /**
   * Dispatch a `thread.turn.start` command that carries a `bootstrap`
   * payload, optionally creating the thread, preparing a worktree, and
   * running the project's setup script before starting the first turn.
   *
   * @param command - `thread.turn.start` command with an optional bootstrap payload.
   * @param options - Optional client origin, stamped into every event the
   *   bootstrap produces. A caller with no client behind it, such as the MCP
   *   toolkit, leaves it out.
   * @returns Effect containing the sequence of the persisted `thread.turn.start` event.
   *
   * On failure, rolls back a thread it created (`thread.delete`) before
   * failing with an `OrchestrationDispatchCommandError`.
   */
  readonly dispatchBootstrapTurnStart: (
    command: Extract<OrchestrationCommand, { type: "thread.turn.start" }>,
    options?: { readonly origin?: OrchestrationClientOrigin },
  ) => Effect.Effect<{ readonly sequence: number }, OrchestrationDispatchCommandError>;
}

/**
 * ThreadBootstrapService - Service tag for thread bootstrap turn-start access.
 *
 * @example
 * ```ts
 * const program = Effect.gen(function* () {
 *   const threadBootstrap = yield* ThreadBootstrapService
 *   return yield* threadBootstrap.dispatchBootstrapTurnStart(command)
 * })
 * ```
 */
export class ThreadBootstrapService extends Context.Service<
  ThreadBootstrapService,
  ThreadBootstrapShape
>()("t3/orchestration/Services/ThreadBootstrap/ThreadBootstrapService") {}
