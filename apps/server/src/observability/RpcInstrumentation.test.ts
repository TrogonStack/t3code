import { assert, describe, it } from "@effect/vitest";
import { AuthOrchestrationReadScope, WS_METHODS, WsRpcGroup } from "@t3tools/contracts";
import * as Effect from "effect/Effect";
import * as Layer from "effect/Layer";
import * as Tracer from "effect/Tracer";
import type * as Rpc from "effect/rpc/Rpc";
import * as RpcClient from "effect/rpc/RpcClient";
import type * as RpcGroup from "effect/rpc/RpcGroup";
import * as RpcServer from "effect/rpc/RpcServer";

import { RPC_REQUIRED_SCOPES } from "../auth/RpcAuthorization.ts";
import * as RpcAuthorization from "../auth/RpcAuthorization.ts";
import {
  RpcInstrumentation,
  rpcInstrumentationLayer,
  rpcServerTracingOptions,
} from "./RpcInstrumentation.ts";

type WsRpcMethod = keyof typeof RPC_REQUIRED_SCOPES;

const group = WsRpcGroup.omit(
  ...[...WsRpcGroup.requests.keys()].filter(
    (tag): tag is Exclude<WsRpcMethod, typeof WS_METHODS.serverProbe> =>
      tag !== WS_METHODS.serverProbe,
  ),
).middleware(RpcInstrumentation);

const withEndedSpans = <A, E, R>(
  test: (ended: ReadonlyArray<Tracer.NativeSpan>) => Effect.Effect<A, E, R>,
) => {
  const ended: Array<Tracer.NativeSpan> = [];
  const tracer = Tracer.make({
    span: (options) => {
      const span = new Tracer.NativeSpan(options);
      const end = span.end.bind(span);
      span.end = (endTime, exit) => {
        end(endTime, exit);
        ended.push(span);
      };
      return span;
    },
  });
  return test(ended).pipe(Effect.scoped, Effect.withTracer(tracer));
};

/** An in-memory client whose server opens request spans the way ws.ts configures it. */
const makeClient = Effect.fnUntraced(function* <Rpcs extends Rpc.Any>(
  rpcGroup: RpcGroup.RpcGroup<Rpcs>,
) {
  let client!: Effect.Success<ReturnType<typeof RpcClient.makeNoSerialization<Rpcs, never>>>;
  const server = yield* RpcServer.makeNoSerialization(rpcGroup, {
    ...rpcServerTracingOptions,
    onFromServer: (response) => client.write(response),
  });
  client = yield* RpcClient.makeNoSerialization(rpcGroup, {
    supportsAck: true,
    onFromClient: ({ message }) => server.write(0, message),
  });
  return client.client;
});

const parentSpanId = (span: Tracer.NativeSpan | undefined) =>
  span?.parent._tag === "Some" ? span.parent.value.spanId : undefined;

describe("WS RPC instrumentation middleware", () => {
  it.effect("opens each request span as a child of the span the client sent", () =>
    withEndedSpans((ended) =>
      Effect.gen(function* () {
        const client = yield* makeClient(group).pipe(
          Effect.provide(
            Layer.mergeAll(
              group.toLayerHandler(WS_METHODS.serverProbe, () => Effect.succeed({})),
              RpcAuthorization.layer([AuthOrchestrationReadScope]),
              rpcInstrumentationLayer,
            ),
          ),
        );

        yield* client[WS_METHODS.serverProbe]({}).pipe(Effect.withSpan("client.call"));

        const rpcSpan = ended.find((span) => span.name.startsWith("ws.rpc."));
        const clientSpan = ended.find((span) => span.kind === "client");
        const callSpan = ended.find((span) => span.name === "client.call");
        assert.equal(rpcSpan?.traceId, callSpan?.traceId);
        assert.equal(parentSpanId(rpcSpan), clientSpan?.spanId);
        assert.equal(parentSpanId(clientSpan), callSpan?.spanId);
      }),
    ),
  );
});
