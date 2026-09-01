// #431: seed one known error into a letter's proposed actions, and keep
// the truth.
//
// The letter track of the study, added 27 August 2026 when the corpus
// moved to synthetic letters (see `app/study/README.md`). The rule is
// the one `study-fixtures.ts` set for statements: seeding returns the
// faulty artefact and its gold answer together, exactly one claim
// differs, and nothing else moves.
//
// ## The three operators, and what checking does to each
//
// The classes are separated by what *checking the evidence does*, as
// they are for statements. On a letter the claim is a proposed action
// — "Pay £480.00 towards the estate matter, by 30 April 2026, the
// letter says 'within 14 days'" — and its evidence is the passage it
// was read from, quoted verbatim.
//
// - `misquoted-deadline` (invention): the card quotes a deadline the
//   letter does not say — "within 28 days" for a letter that says
//   "within 14 days" — and the date it works out follows the false
//   quote. Opening the passage *refutes* it: the words are not there.
// - `misresolved-deadline` (mis-relation): the quote is genuine and
//   the date worked out from it is wrong. Opening the passage
//   *confirms* every word, and the reading is still wrong — the person
//   has to do the sum themselves.
// - `dropped-obligation` (omission): an ask the letter makes is not on
//   the page at all. There is nothing to open; only the letter itself
//   shows it.
//
// The gold is what the un-seeded run showed, as it is for statements:
// the seed is measured against the page it started from, and the bed's
// own expected answer travels in the corpus file so the audit can say
// where the pipeline was already wrong before anything was seeded.

import type { IsoDate, ProposedAction, ProposedActions } from "./types";

/** One letter of the corpus: the source, the genuine output, the bed's answer. */
export interface StudyLetter {
  schema: "kettle/study-letter@0";
  id: string;
  source: { file: string; hash: string; text: string };
  pack: { id: string; version: string };
  model: string;
  actions: ProposedActions;
  expected: ExpectedObligation[];
}

/** What the bed says the letter asks — the audit's reference, not the seed's. */
export interface ExpectedObligation {
  id: string;
  segment: string;
  kind: string;
  party: string;
  deadline: string;
  anchor: string;
  due: IsoDate | null;
}

export type LetterOperator =
  | "misquoted-deadline"
  | "misresolved-deadline"
  | "dropped-obligation";

/** One seeded mistake, named before the session runs. The target is the action's title — the ask, as the card heads it. */
export interface LetterError {
  operator: LetterOperator;
  target: string;
}

export type LetterTruth =
  | {
      operator: "misquoted-deadline";
      target: string;
      gold: { phrase: string; due: IsoDate | null };
      shown: { phrase: string; due: IsoDate | null };
    }
  | { operator: "misresolved-deadline"; target: string; gold: IsoDate; shown: IsoDate }
  /** `shown: null` is the finding itself: the ask is not on the page. */
  | { operator: "dropped-obligation"; target: string; gold: { phrase: string; due: IsoDate | null }; shown: null };

export interface SeededLetter {
  letter: StudyLetter;
  truth: LetterTruth;
}

const MONTHS = [
  "January", "February", "March", "April", "May", "June",
  "July", "August", "September", "October", "November", "December",
];

/** "2026-04-30" → "30 April 2026", the form the runner prints on a card. */
export function longDate(date: IsoDate): string {
  const [year = "", month = "", day = ""] = date.split("-");
  return `${Number(day)} ${MONTHS[Number(month) - 1] ?? ""} ${year}`;
}

/** A date moved by whole days, in the calendar rather than by arithmetic on strings. */
export function shifted(date: IsoDate, days: number): IsoDate {
  const [y = 0, m = 1, d = 1] = date.split("-").map(Number);
  const moved = new Date(Date.UTC(y, m - 1, d + days));
  return moved.toISOString().slice(0, 10);
}

/** By how far a seed moves a date. Seven days: not a typo's distance, and never lands on the same weekday's obvious neighbour. */
export const SHIFT_DAYS = 7;

const DAYS = /\b(\d{1,3}) (working )?days?\b/;
const WRITTEN_DATE = /\b(\d{1,2}) (January|February|March|April|May|June|July|August|September|October|November|December) (\d{4})\b/;

function contains(text: string, phrase: string): boolean {
  return text.toLowerCase().includes(phrase.toLowerCase());
}

/**
 * The phrase the invention shows in place of the genuine one, or
 * `null` when there is no honest way to misquote this deadline.
 *
 * A number of days doubles ("within 14 days" → "within 28 days"); a
 * written date moves by `SHIFT_DAYS`. Anything else — "by the end of
 * the month", "as soon as possible", "by the date shown beside it" —
 * has no value in it to misstate, and a misquote of it would be a
 * different sentence rather than a wrong one. The result must not
 * already be in the letter, or the "false" quote would be true.
 */
export function misquote(phrase: string, text: string): { phrase: string; days: number } | null {
  const days = DAYS.exec(phrase);
  if (days !== null) {
    const n = Number(days[1]);
    const replaced = phrase.replace(DAYS, `${n * 2} ${days[2] ?? ""}day${n * 2 === 1 ? "" : "s"}`);
    return contains(text, replaced) ? null : { phrase: replaced, days: n };
  }
  const written = WRITTEN_DATE.exec(phrase);
  if (written !== null) {
    const iso = `${written[3]}-${String(MONTHS.indexOf(written[2] ?? "") + 1).padStart(2, "0")}-${String(written[1]).padStart(2, "0")}`;
    const replaced = phrase.replace(WRITTEN_DATE, longDate(shifted(iso, SHIFT_DAYS)));
    return contains(text, replaced) ? null : { phrase: replaced, days: SHIFT_DAYS };
  }
  return null;
}

/** The actions in a letter that can carry a given operator, by title. */
export function eligible(letter: StudyLetter, operator: LetterOperator): string[] {
  return letter.actions.actions
    .filter((action) => {
      const phrase = action.evidence.in_the_letter ?? "";
      switch (operator) {
        case "misquoted-deadline":
          return misquote(phrase, letter.source.text) !== null;
        case "misresolved-deadline":
          // Only a date somebody worked out can be worked out wrong. A
          // date written in the deadline itself was read, not computed,
          // and shifting it would contradict the quote — an invention.
          return action.export.ics !== undefined && !WRITTEN_DATE.test(phrase);
        case "dropped-obligation":
          // Dropping the only ask leaves a letter that asks nothing,
          // which a page with no cards on it already says. The omission
          // the study is about is the one hidden among asks that are
          // there.
          return letter.actions.actions.length >= 2;
      }
    })
    .map((action) => action.title);
}

function replaceOnce(haystack: string, needle: string, replacement: string, where: string): string {
  const at = haystack.indexOf(needle);
  if (at === -1) {
    throw new Error(`cannot seed: "${needle}" is not in the action's ${where}`);
  }
  return haystack.slice(0, at) + replacement + haystack.slice(at + needle.length);
}

function find(letter: StudyLetter, target: string): ProposedAction {
  const matches = letter.actions.actions.filter((action) => action.title === target);
  if (matches.length !== 1) {
    throw new Error(
      `cannot seed: ${matches.length} actions in ${letter.id} are titled "${target}", and a seed must name exactly one`,
    );
  }
  return matches[0] as ProposedAction;
}

/**
 * Rewrite one action so its deadline reads as `phrase` and falls on
 * `due`, everywhere the card says either. Every field that carries the
 * deadline moves together — detail, evidence, export — or the
 * participant detects the artefact instead of the error.
 */
function withDeadline(
  action: ProposedAction,
  from: { phrase: string; due: IsoDate | null },
  to: { phrase: string; due: IsoDate | null },
): ProposedAction {
  let detail = replaceOnce(action.detail, `"${from.phrase}"`, `"${to.phrase}"`, "detail");
  const evidence = { ...action.evidence, in_the_letter: to.phrase };
  let ics = action.export.ics;
  if (from.due !== null && to.due !== null) {
    detail = replaceOnce(detail, longDate(from.due), longDate(to.due), "detail");
    if (ics === undefined) throw new Error("cannot seed: a dated action carries no calendar export");
    ics = { ...ics, date: to.due };
  }
  const text = replaceOnce(action.export.text, from.phrase, to.phrase, "export text");
  return {
    ...action,
    detail,
    evidence,
    export: ics === undefined ? { text } : { ics, text },
  };
}

/** Seed one named error, returning what the participant sees and what they should have seen. */
export function seedLetter(letter: StudyLetter, error: LetterError): SeededLetter {
  const action = find(letter, error.target);
  const phrase = action.evidence.in_the_letter ?? "";
  const due = action.export.ics?.date ?? null;
  const swap = (replacement: ProposedAction | null): ProposedActions => ({
    ...letter.actions,
    actions: letter.actions.actions
      .flatMap((each) => (each.title === error.target ? (replacement ? [replacement] : []) : [each]))
      // Renumbered, so a gap in "act-01, act-03" cannot point at the
      // omission.
      .map((each, index) => ({ ...each, id: `act-${String(index + 1).padStart(2, "0")}` })),
  });

  switch (error.operator) {
    case "misquoted-deadline": {
      const false_ = misquote(phrase, letter.source.text);
      if (false_ === null) {
        throw new Error(`cannot seed: "${phrase}" carries no value to misquote`);
      }
      const shown = {
        phrase: false_.phrase,
        due: due === null ? null : shifted(due, false_.days),
      };
      return {
        letter: { ...letter, actions: swap(withDeadline(action, { phrase, due }, shown)) },
        truth: { operator: "misquoted-deadline", target: error.target, gold: { phrase, due }, shown },
      };
    }
    case "misresolved-deadline": {
      if (due === null) throw new Error(`cannot seed: "${error.target}" has no worked-out date to move`);
      const shown = shifted(due, SHIFT_DAYS);
      return {
        letter: {
          ...letter,
          actions: swap(withDeadline(action, { phrase, due }, { phrase, due: shown })),
        },
        truth: { operator: "misresolved-deadline", target: error.target, gold: due, shown },
      };
    }
    case "dropped-obligation":
      return {
        letter: { ...letter, actions: swap(null) },
        truth: { operator: "dropped-obligation", target: error.target, gold: { phrase, due }, shown: null },
      };
  }
}
