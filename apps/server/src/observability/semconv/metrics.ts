// Generated from a semantic convention registry. Do not edit.
import * as Metric from "effect/Metric";
import * as Duration from "effect/Duration";

export const T3Outcome = {
  Failure: "failure",
  Interrupt: "interrupt",
  Success: "success",
} as const;

export type T3Outcome = (typeof T3Outcome)[keyof typeof T3Outcome];

export type T3GitCommandDurationAttributes = {
  readonly gitOperation: string;
  readonly outcome: T3Outcome;
};

const t3GitCommandDuration = Metric.mapInput(
  Metric.histogram("t3.git.command.duration", {
    description: "Git command execution duration.",
    attributes: { unit: "s" },
    boundaries: [
      0.005,
      0.01,
      0.025,
      0.05,
      0.075,
      0.1,
      0.25,
      0.5,
      0.75,
      1,
      2.5,
      5,
      7.5,
      10,
      Infinity,
    ],
  }),
  Duration.toSeconds,
);

export function updateT3GitCommandDuration(
  value: Duration.Duration,
  attributes: T3GitCommandDurationAttributes,
) {
  return Metric.update(
    Metric.withAttributes(t3GitCommandDuration, {
      "t3.git.operation": attributes.gitOperation,
      "t3.outcome": attributes.outcome,
    }),
    value,
  );
}
