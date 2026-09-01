// #431: what a participant said, and what it counts as.
//
// The measures were frozen on the issue before anyone is recruited —
// false acceptance, correct detection, false alarm, correction
// accuracy, review time, evidence interaction, confidence before and
// after checking. This is where they stop being prose and become
// something the analysis can compute, so the definitions are the tests.
//
// The one that decides the primary measure is what counts as a *false
// alarm*, and it is defined here as **a correct claim rejected**. That
// happens on a seeded report as readily as on a clean one: a
// participant who senses something is off and points at the wrong row
// would "correct" a right figure, which is the same harm whichever
// report they were shown.

import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { monthName } from "./format";
import type { SeededTruth } from "./study-fixtures";
import { plan, type Task } from "./study-session";
import { ABSENT, score, type Response } from "./study-response";
import type { Period, RunReport } from "./types";

const repoRoot = join(__dirname, "../../..");

/** The committed study corpus: ten statements and the ten reports
 * Kettle produced from them. The scorer is tested on the reports the
 * study actually shows, not on a shape invented here. */
function corpus(): RunReport[] {
  return Array.from({ length: 10 }, (_, i) => {
    const name = `report-${String(i + 1).padStart(2, "0")}.json`;
    return JSON.parse(
      readFileSync(join(repoRoot, "fixtures/study", name), "utf8"),
    ) as RunReport;
  });
}

const accepted: Response = {
  verdict: "accept",
  flagged: [],
  offered: [],
  correction: null,
  confidence_before: 3,
  confidence_after: 3,
  opened: [],
  elapsed_ms: 60_000,
};

function taskOf(tasks: Task[], wanted: Task["class"]): Task {
  const found = tasks.find((task) => task.class === wanted);
  if (found === undefined) throw new Error(`no ${wanted} task in the session`);
  return found;
}

const PERIOD_NOUN: Record<Period, string> = {
  weekly: "week",
  monthly: "month",
  quarterly: "quarter",
  yearly: "year",
};

/** The gold answer written the way a person writes it, per operator. */
function asPersonWrites(truth: SeededTruth): string {
  switch (truth.operator) {
    case "wrong-amount":
    case "dropped-claim":
      return `£${truth.gold}`;
    case "wrong-period":
      return `every ${PERIOD_NOUN[truth.gold]}`;
    case "wrong-rise-month":
      return `${monthName(truth.gold)} ${truth.gold.slice(0, 4)}`;
  }
}

/** What the faulty report told them — never a correction. */
function shown(truth: SeededTruth): string {
  return truth.operator === "dropped-claim" ? "0.00" : String(truth.shown);
}

describe("scoring one participant's answer", () => {
  const tasks = plan("p01", corpus()).tasks;

  it("a_seeded_claim_named_is_a_detection_and_a_correct_claim_rejected_is_a_false_alarm", () => {
    const seeded = taskOf(tasks, "invention");
    const target = seeded.truth?.target as string;

    // Named the claim that was actually wrong.
    expect(
      score(seeded, { ...accepted, verdict: "reject", flagged: [target] }).outcome,
    ).toBe("correct-detection");

    // Accepted it. The primary harm: a wrong claim carried away.
    expect(score(seeded, accepted).outcome).toBe("false-acceptance");

    // Rejected a claim that was right, on a report that did carry a
    // seed. Same outcome as on a clean report, because it is the same
    // mistake and would change the same kind of correct figure.
    const other = seeded.report.recurring.find((f) => f.merchant !== target);
    expect(
      score(seeded, {
        ...accepted,
        verdict: "reject",
        flagged: [other?.merchant ?? "nobody"],
      }).outcome,
    ).toBe("false-alarm");

    const clean = taskOf(tasks, "clean");
    expect(score(clean, accepted).outcome).toBe("correct-acceptance");
    expect(
      score(clean, {
        ...accepted,
        verdict: "reject",
        flagged: [clean.report.recurring[0]?.merchant ?? "nobody"],
      }).outcome,
    ).toBe("false-alarm");
  });

  it("doubt_with_nothing_named_is_neither_a_detection_nor_a_false_alarm", () => {
    // A real answer, and it stays distinguishable from both. Not a
    // detection: the participant cannot say what to fix, and neither
    // could a Kettle user holding the same report. Not a false alarm
    // either: no correct claim has been rejected. Folding it into
    // either would move the primary measure by how the form was worded.
    const seeded = taskOf(tasks, "mis-relation");
    expect(
      score(seeded, { ...accepted, verdict: "reject", flagged: [] }).outcome,
    ).toBe("unattributed-doubt");

    const clean = taskOf(tasks, "clean");
    expect(
      score(clean, { ...accepted, verdict: "reject", flagged: [] }).outcome,
    ).toBe("unattributed-doubt");
  });

  it("correction_accuracy_is_judged_against_the_gold_of_the_operator_that_was_seeded", () => {
    for (const task of tasks) {
      const truth = task.truth;
      if (truth === null) continue;
      const detected: Response = {
        ...accepted,
        verdict: "reject",
        // What a detection is depends on the operator: a claim on the
        // page is pointed at, and an absence has nothing to point at.
        flagged: [truth.operator === "dropped-claim" ? ABSENT : truth.target],
      };

      expect(
        score(task, { ...detected, correction: asPersonWrites(truth) }).corrected,
      ).toBe("right");

      // Repeating what the report said is not a correction.
      expect(score(task, { ...detected, correction: shown(truth) }).corrected).toBe(
        "wrong",
      );

      // Nothing offered is nothing to judge — it is not a wrong answer,
      // and scoring it as one would put "couldn't say" in the same
      // column as "said something false".
      expect(score(task, detected).corrected).toBe("not-offered");
      expect(score(task, accepted).corrected).toBe("not-offered");
    }
  });

  it("the_scorer_hands_back_anything_it_would_have_to_guess_at", () => {
    // A free-text field is hand-judged in the end, and an automatic
    // pass that guesses is worse than one that declines: it puts a
    // silent misreading into the primary correction measure, where
    // nobody looks again. #568's rung 1 is the precedent — a proximity
    // check would have reported a clean 4.5% invention rate by
    // accepting near answers as right ones.
    for (const task of tasks) {
      const truth = task.truth;
      if (truth === null) continue;
      const detected: Response = {
        ...accepted,
        verdict: "reject",
        // What a detection is depends on the operator: a claim on the
        // page is pointed at, and an absence has nothing to point at.
        flagged: [truth.operator === "dropped-claim" ? ABSENT : truth.target],
      };
      const right = asPersonWrites(truth);

      // Two candidate answers in one sentence: which one is the
      // correction is a reading, not a parse.
      expect(
        score(task, { ...detected, correction: `${shown(truth)} → ${right}` })
          .corrected,
      ).toBe("needs-judging");

      // Saying what it is not says nothing about what it is, and a
      // single value behind a negation would otherwise read as an
      // assertion of that value.
      expect(
        score(task, { ...detected, correction: `not ${shown(truth)}` }).corrected,
      ).toBe("needs-judging");

      // An answer of no recognisable kind — the participant wrote
      // about something else, or about the claim in words.
      expect(
        score(task, { ...detected, correction: "the row underneath it" }).corrected,
      ).toBe("needs-judging");
    }
  });

  it("evidence_interaction_and_confidence_shift_travel_with_every_answer", () => {
    const seeded = taskOf(tasks, "invention");
    const target = seeded.truth?.target as string;

    const checked = score(seeded, {
      ...accepted,
      opened: [target],
      confidence_before: 2,
      confidence_after: 5,
    });
    // The secondary prediction is directional — confidence rises for a
    // mis-relation and falls for an invention — so the shift is signed.
    expect(checked.confidence_shift).toBe(3);
    expect(checked.opened_target).toBe(true);
    expect(score(seeded, accepted).opened_target).toBe(false);

    // An omission has no row to open, and that is the finding rather
    // than a gap in the data: `false` would record a participant as
    // having declined to check evidence that was never on the page.
    expect(score(taskOf(tasks, "omission"), accepted).opened_target).toBeNull();
    expect(score(taskOf(tasks, "clean"), accepted).opened_target).toBeNull();
  });

  it("the_score_carries_the_class_and_operator_so_the_analysis_never_re_derives_them", () => {
    // Every result is reported per error type before any overall mean,
    // because the classes have different base rates *and* different
    // detectability — a mean over them is an artefact of whichever was
    // seeded most. Carrying the class on the score makes the right
    // analysis the easy one.
    for (const task of tasks) {
      const scored = score(task, accepted);
      expect(scored.class).toBe(task.class);
      expect(scored.operator).toBe(task.truth?.operator ?? null);
    }
  });
});
