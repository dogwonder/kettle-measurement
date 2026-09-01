// #431: what a participant said, and what it counts as.
//
// The issue's measures — false acceptance, correct detection, false
// alarm, correction accuracy, review time, evidence interaction,
// confidence before and after checking — were frozen before anyone is
// recruited. Turning them into code is where they acquire edges, and
// two of those edges are decisions rather than mechanics.
//
// ## A false alarm is a correct claim rejected
//
// Not "a clean report rejected". A participant shown a seeded report
// who senses something is off and points at the wrong row has rejected
// a claim that was right, and would have changed a correct figure. The
// harm is identical on both kinds of report, so the definition is one
// rule applied to both — which also stops the seeded reports from
// having a false-alarm rate of zero by construction.
//
// ## Doubt with nothing named is its own outcome
//
// "Something here is wrong but I can't say what" is a real answer, and
// it is neither of the two it would otherwise be forced into. It is not
// a detection: nothing can be fixed on it, by the participant or by a
// Kettle user holding the same report. It is not a false alarm either,
// because no correct claim has been rejected. A form that made the
// participant choose would move the primary measure by its wording.
//
// The classes are not equally able to produce it, and that is the
// point: an invention can be pinned to the row whose evidence refutes
// it, while a mis-relation confirms every figure it touches, so doubt
// with nowhere to point is the shape a mis-relation is expected to
// produce. Collapsing it would delete the thing the study is powered
// to see.

import { monthName } from "./format";
import type { SeededError, SeededTruth } from "./study-fixtures";
import type { LetterOperator } from "./study-letter";
import type { Task, TaskClass } from "./study-session";
import type { IsoMonth, Money, Period } from "./types";

/** Whether the participant let the report stand. */
export type Verdict = "accept" | "reject";

/** The five-point confidence scale, asked twice per task. */
export type Rating = 1 | 2 | 3 | 4 | 5;

/** One participant's answer to one task. */
export interface Response {
  verdict: Verdict;
  /**
   * Every claim they say is wrong, empty for accepted and for doubt
   * they could not place.
   *
   * A set, because a report does not always have one thing wrong with
   * it: three of the author's first thirty answers put a second
   * complaint in the correction box, having nowhere else to put it, and
   * a participant who saw the seed *and* something else had to choose
   * between them. `ABSENT` may be ticked alongside a claim — "this card
   * is wrong and something is missing" is the commonest of those three.
   */
  flagged: string[];
  /**
   * The claims the report offered, so a false alarm has a denominator.
   *
   * Recorded rather than recomputed for the same reason `drawn` is: the
   * report a participant saw is the only honest denominator, and a
   * later change to what the harness would render must not silently
   * restate what they were choosing between.
   */
  offered: string[];
  /** What they say the right answer is, as they wrote it. */
  correction: string | null;
  /** Before opening any evidence. */
  confidence_before: Rating;
  /** After checking, whatever checking they did. */
  confidence_after: Rating;
  /** Merchants whose evidence disclosure they opened. */
  opened: string[];
  /** Review time for this task. */
  elapsed_ms: number;
}

/**
 * How a correction was judged.
 *
 * Four states rather than a boolean, because a blank, a wrong answer
 * and an answer the harness cannot read are three different things and
 * only one of them belongs in the wrong column. `needs-judging` is the
 * work a person still has to do, counted rather than absorbed.
 */
export type Correction = "right" | "wrong" | "not-offered" | "needs-judging";

/**
 * What a participant picks when what is wrong is an absence.
 *
 * The omission operators drop a claim, so the gold answer names
 * something that is not on the page and no list of what *is* there can
 * offer it. A sentinel in `flagged` rather than a second field, so that
 * everything reading an answer — the floor, both scorers — reads one
 * thing; and the sentinel is the words the participant clicked, so the
 * published file says what they picked rather than encoding it.
 */
export const ABSENT = "Something that should be here is missing";

export type Outcome =
  /** Named the claim that was seeded. */
  | "correct-detection"
  /** Let a seeded claim stand. */
  | "false-acceptance"
  /** Rejected a claim that was right — on either kind of report. */
  | "false-alarm"
  /** Rejected the report, named nothing. */
  | "unattributed-doubt"
  /** Let a clean report stand. */
  | "correct-acceptance"
  /**
   * Rejected a document the audit records as carrying an error the
   * pipeline made on its own (#577).
   *
   * Not a false alarm and not a detection of the seed. The audit is a
   * person's signed read of what the pipeline got wrong before any
   * error was seeded, and a participant who catches one of those is
   * right about the report in front of them. Scoring it as a false
   * alarm contaminates the rate in the direction that makes the most
   * careful participant look like the noisiest.
   */
  | "caught-a-natural-error";

/**
 * What the audit says about the document a task was built from.
 *
 * Read from `fixtures/study/audit-letters.json` (letters) or
 * `fixtures/study/audit.json` (statements), both of which a person
 * signs. The scorer never decides for itself whether a proposed ask is
 * a fair reading — that judgement is the audit's, made once, by
 * somebody answerable for it.
 */
export interface DocumentAudit {
  /** The pipeline's own output on this document was wrong. */
  unclean: boolean;
}

export interface Score {
  outcome: Outcome;
  /** Carried, never re-derived: results are reported per error type
   * before any overall mean. */
  class: TaskClass;
  operator: SeededError["operator"] | LetterOperator | null;
  /**
   * How the correction was judged. Correction accuracy's denominator is
   * `right + wrong`: a blank is not a wrong answer, and neither is an
   * answer the harness declined to read. Both are reported as their own
   * rates beside it, so declining is visible rather than free.
   */
  corrected: Correction;
  /** After minus before. Signed, because the prediction has a sign. */
  confidence_shift: number;
  /**
   * Whether they opened the seeded claim's evidence, or `null` when
   * there was no such row — a clean report, or an omission. `false`
   * there would record a decision not to check evidence that was never
   * on the page.
   */
  opened_target: boolean | null;
  /**
   * Ticked claims that carried no seed — a false alarm each, against
   * `claims_offered`.
   *
   * Reported beside the outcome rather than folded into it, because a
   * participant who ticks everything detects every seed. A hit rate of
   * one next to a false-alarm rate of one is visibly uninformative;
   * "correct-detection" on its own would have read as a perfect score.
   */
  false_alarms: number;
  /** How many claims the report offered to tick. */
  claims_offered: number;
}

/** Merchant names compare on their visible characters, not their spacing. */
function same(one: string, other: string): boolean {
  const tidy = (text: string) => text.trim().toLowerCase().replace(/\s+/g, " ");
  return tidy(one) === tidy(other);
}

/**
 * Every amount-shaped token in the text, canonicalised: `£12.99`,
 * `12.99`, `£1,299`, `1299` all read as two-decimal strings.
 *
 * All of them, not the first — a correction reading "£129.90 → £12.99"
 * carries two candidate answers, and which one the participant meant is
 * a reading rather than a parse.
 */
function amountsIn(text: string): Money[] {
  const found = text.match(/-?\d[\d,]*(?:\.\d{1,2})?/g) ?? [];
  return Array.from(
    new Set(
      found.map((token) => {
        const bare = token.replace(/,/g, "");
        const [pounds, pence = "0"] = bare.split(".");
        return `${pounds}.${pence.padEnd(2, "0")}`;
      }),
    ),
  );
}

const PERIOD_WORDS: Record<Period, string[]> = {
  weekly: ["weekly", "week"],
  monthly: ["monthly", "month"],
  quarterly: ["quarterly", "quarter", "3 months", "three months"],
  yearly: ["yearly", "year", "annual", "annually", "12 months"],
};

/** Every period a person named, however they named it. */
function periodsIn(text: string): Period[] {
  const words = text.trim().toLowerCase();
  const named = (Object.entries(PERIOD_WORDS) as [Period, string[]][])
    .filter(([, forms]) => forms.some((form) => new RegExp(`\\b${form}\\b`).test(words)))
    .map(([period]) => period);
  return Array.from(new Set(named));
}

/**
 * Every month a person named: `2025-06`, `June`, `June 2025`.
 *
 * A bare month name is accepted. The report carries one price rise for
 * the merchant in question and prints the year beside it, so requiring
 * the year would score reading comprehension rather than detection. A
 * year that *is* given is used, because naming another year is naming
 * another rise.
 */
function monthsIn(text: string, fallbackYear: string): IsoMonth[] {
  const words = text.trim().toLowerCase();
  const iso = Array.from(words.matchAll(/\b(\d{4})-(\d{2})\b/g)).map(
    (match) => `${match[1]}-${match[2]}`,
  );
  const year = /\b(\d{4})\b/.exec(words)?.[1] ?? fallbackYear;
  const named = Array.from({ length: 12 }, (_, i) => i)
    .filter((i) => {
      const name = monthName(`0000-${String(i + 1).padStart(2, "0")}`).toLowerCase();
      return new RegExp(`\\b${name.slice(0, 3)}`).test(words);
    })
    .map((i) => `${year}-${String(i + 1).padStart(2, "0")}`);
  return Array.from(new Set([...iso, ...named]));
}

/**
 * A single value behind a negation is not an assertion of that value.
 *
 * "not quarterly" names one period and claims the opposite of it, and
 * without this guard it would score as a confident wrong correction —
 * the one systematic misreading a single-candidate parse can make.
 */
function negated(text: string): boolean {
  return /(\bnot\b|n['’]t\b|\bnever\b|\brather than\b|\binstead\b)/i.test(text);
}

/** The candidate answers of the kind this operator was scored on. */
function candidates(truth: SeededTruth, correction: string): string[] {
  switch (truth.operator) {
    case "wrong-amount":
    case "dropped-claim":
      return amountsIn(correction);
    case "wrong-period":
      return periodsIn(correction);
    case "wrong-rise-month":
      return monthsIn(correction, truth.gold.slice(0, 4));
  }
}

/**
 * Judge a correction, and decline rather than guess.
 *
 * The precedent is #568's rung 1: a proximity check over a dense
 * financial table would have reported a clean 4.5% invention rate by
 * accepting near answers as right ones. A free-text correction is
 * hand-judged in the end, and an automatic pass earns its place only by
 * being unambiguous where it answers and silent where it is not.
 */
function judge(truth: SeededTruth, correction: string): Correction {
  if (correction.trim() === "") return "not-offered";
  const offered = candidates(truth, correction);
  if (offered.length !== 1 || negated(correction)) return "needs-judging";
  return offered[0] === truth.gold ? "right" : "wrong";
}

/** Score one answer against what the task actually carried. */
export function score(task: Task, response: Response, audit?: DocumentAudit): Score {
  const truth = task.truth;
  const confidence_shift = response.confidence_after - response.confidence_before;
  const operator = truth?.operator ?? null;
  // A clean report has no seeded row, and an omission's row is the one
  // thing not on the page — in neither case is there a disclosure the
  // participant could have opened.
  const opened_target =
    truth === null || truth.operator === "dropped-claim"
      ? null
      : response.opened.some((merchant) => same(merchant, truth.target));

  const common = { class: task.class, operator, confidence_shift, opened_target };

  // What the seed would be, if the participant were right about it.
  // Ticking `ABSENT` is the answer on an omission and wrong on
  // anything else; ticking a claim is the reverse.
  const named = (claim: string) =>
    truth !== null &&
    (same(claim, ABSENT)
      ? truth.operator === "dropped-claim"
      : truth.operator !== "dropped-claim" && same(claim, truth.target));
  const hit = response.flagged.some(named);
  // Every other tick is a false alarm of its own, against the claims
  // the report offered. Ticking everything therefore detects every seed
  // and says nothing.
  const false_alarms = response.flagged.filter((claim) => !named(claim)).length;
  const claims_offered = response.offered.length;
  const counted = { ...common, false_alarms, claims_offered };

  if (response.verdict === "accept") {
    return {
      ...counted,
      false_alarms: 0,
      outcome: truth === null ? "correct-acceptance" : "false-acceptance",
      corrected: "not-offered",
    };
  }

  if (response.flagged.length === 0) {
    return { ...counted, outcome: "unattributed-doubt", corrected: "not-offered" };
  }

  if (hit) {
    return {
      ...counted,
      outcome: "correct-detection",
      corrected: judge(truth as never, response.correction ?? ""),
    };
  }

  // Nothing seeded was ticked. That is a false alarm only where the
  // document itself was right (#577).
  return {
    ...counted,
    outcome: audit?.unclean ? "caught-a-natural-error" : "false-alarm",
    corrected: "not-offered",
  };
}
