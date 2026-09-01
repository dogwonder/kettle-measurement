// #431: score one answer on the letter track.
//
// The outcomes are the statement track's — the vocabulary is the
// study's, not the corpus's — and the judging rule is the same: an
// automatic pass earns its place only by being unambiguous where it
// answers and silent where it is not. What differs is the kind of
// value a correction names: a date, or a number of days.

import type { LetterTask } from "./study-letter-session";
import type { LetterTruth } from "./study-letter";
import {
  ABSENT,
  type Correction,
  type DocumentAudit,
  type Outcome,
  type Response,
  type Score,
} from "./study-response";
import type { IsoDate } from "./types";

function same(one: string, other: string): boolean {
  const tidy = (text: string) => text.trim().toLowerCase().replace(/\s+/g, " ");
  return tidy(one) === tidy(other);
}

const MONTHS = [
  "january", "february", "march", "april", "may", "june",
  "july", "august", "september", "october", "november", "december",
];

/** Every date a person named: `2026-05-23`, `23 May 2026`, `23/05/2026`. */
export function datesIn(text: string): IsoDate[] {
  const words = text.toLowerCase();
  const found = new Set<string>();
  for (const m of words.matchAll(/\b(\d{4})-(\d{2})-(\d{2})\b/g)) found.add(`${m[1]}-${m[2]}-${m[3]}`);
  for (const m of words.matchAll(/\b(\d{1,2})\/(\d{1,2})\/(\d{4})\b/g)) {
    found.add(`${m[3]}-${String(m[2]).padStart(2, "0")}-${String(m[1]).padStart(2, "0")}`);
  }
  for (const m of words.matchAll(/\b(\d{1,2})(?:st|nd|rd|th)? ([a-z]+),? (\d{4})\b/g)) {
    const month = MONTHS.findIndex((name) => name.startsWith((m[2] ?? "").slice(0, 3)));
    if (month !== -1 && (m[2] ?? "").length >= 3) {
      found.add(`${m[3]}-${String(month + 1).padStart(2, "0")}-${String(m[1]).padStart(2, "0")}`);
    }
  }
  return [...found];
}

/** Every count of days a person named. */
export function daysIn(text: string): number[] {
  return [...new Set([...text.toLowerCase().matchAll(/\b(\d{1,3}) (?:working )?days?\b/g)].map((m) => Number(m[1])))];
}

function negated(text: string): boolean {
  return /(\bnot\b|n['’]t\b|\bnever\b|\brather than\b|\binstead\b)/i.test(text);
}

function judge(truth: LetterTruth, correction: string): Correction {
  if (correction.trim() === "") return "not-offered";
  if (negated(correction)) return "needs-judging";
  const dates = datesIn(correction);
  switch (truth.operator) {
    case "misresolved-deadline":
      if (dates.length !== 1) return "needs-judging";
      return dates[0] === truth.gold ? "right" : "wrong";
    case "misquoted-deadline":
    case "dropped-obligation": {
      // Either the true date or the true number of days is a right
      // answer; both offered and disagreeing is a person's read.
      const days = daysIn(correction);
      const goldDays = /\b(\d{1,3}) (?:working )?days?\b/.exec(truth.gold.phrase)?.[1];
      const offered = [...dates, ...days.map((n) => `${n} days`)];
      if (offered.length !== 1) return "needs-judging";
      if (dates.length === 1) {
        return truth.gold.due !== null && dates[0] === truth.gold.due ? "right" : "wrong";
      }
      return goldDays !== undefined && Number(goldDays) === days[0] ? "right" : "wrong";
    }
  }
}

/** Score one answer against what the letter task actually carried. */
export function scoreLetter(
  task: LetterTask,
  response: Response,
  audit?: DocumentAudit,
): Score {
  const truth = task.truth;
  const confidence_shift = response.confidence_after - response.confidence_before;
  const operator = truth?.operator ?? null;
  const opened_target =
    truth === null || truth.operator === "dropped-obligation"
      ? null
      : response.opened.some((ask) => same(ask, truth.target));
  const common = { class: task.class, operator, confidence_shift, opened_target };

  // The same rule as the statement track, and it was wrong there in the
  // same way twice: it compared a prose answer to a claim's title, and
  // then it read only the first thing a participant said. See `Response`.
  const named = (claim: string) =>
    truth !== null &&
    (same(claim, ABSENT)
      ? truth.operator === "dropped-obligation"
      : truth.operator !== "dropped-obligation" && same(claim, truth.target));
  const hit = response.flagged.some(named);
  const false_alarms = response.flagged.filter((claim) => !named(claim)).length;
  const counted = { ...common, false_alarms, claims_offered: response.offered.length };

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
      corrected: judge(truth as LetterTruth, response.correction ?? ""),
    };
  }
  return {
    ...counted,
    outcome: audit?.unclean ? "caught-a-natural-error" : "false-alarm",
    corrected: "not-offered",
  };
}
