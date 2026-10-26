// Generated from a semantic convention registry. Do not edit.
import * as Metric from "effect/Metric";
import * as Duration from "effect/Duration";

export const T3codeOutcome = {
  Failure: "failure",
  Interrupt: "interrupt",
  Success: "success",
} as const;

export type T3codeOutcome = (typeof T3codeOutcome)[keyof typeof T3codeOutcome];

export type T3codeGitCommandDurationAttributes = {
  readonly gitOperation: string;
  readonly outcome: T3codeOutcome;
};

const t3codeGitCommandDuration = Metric.mapInput(
  Metric.histogram("t3code.git.command.duration", {
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

export function updateT3codeGitCommandDuration(
  value: Duration.Duration,
  attributes: T3codeGitCommandDurationAttributes,
) {
  return Metric.update(
    Metric.withAttributes(t3codeGitCommandDuration, {
      "t3code.git.operation": attributes.gitOperation,
      "t3code.outcome": attributes.outcome,
    }),
    value,
  );
}
