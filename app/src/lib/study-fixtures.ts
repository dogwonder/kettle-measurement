// #431: seed one known error into a clean report, and keep the truth.
//
// The study asks whether a person catches an error automated
// containment did not. That only means anything if the harness can say,
// afterwards and without re-deriving it, exactly what was wrong and
// what the right answer was — so seeding returns the faulty report and
// its gold answer together, as one value.
//
// ## Why an omission is a first-class operator here
//
// #432's reading across the whole v16 archive (#565) found `prevented`
// = 0 at every policy rung with a flat ladder. Decomposing into closed
// questions over short passages nearly eliminates invention, so the
// guards have almost nothing to catch — and the harm that remains is
// omission, which no guard can reach because there is no claim to
// inspect. Every one of the seven wrong answers on the letter pack's
// sealed set was a miss.
//
// That asymmetry runs against the product's central claim, which is why
// the study exists. An invention arrives with the quote that refutes
// it: the row is on the page and checking it is what the report is
// built for. An omission arrives with nothing — no row, no evidence to
// open, no claim to distrust — so to catch it a person has to notice
// that something they were never shown is absent.
//
// A seeding framework that could only corrupt displayed values would
// silently confine the study to the error class that barely happens.
// So `dropped-claim` is here beside `wrong-amount`, and both keep the
// same invariant: exactly one claim differs, and nothing else moves.

import type { IsoMonth, Money, Period, RecurringFinding, RunReport } from "./types";

/** One seeded mistake, named before the study runs. */
export type SeededError =
  /** A displayed value is wrong. The evidence is still on the page. */
  | { operator: "wrong-amount"; target: string }
  /**
   * A real figure read under the wrong relation: a monthly commitment
   * displayed as quarterly. Every figure on the page stays genuine —
   * checking the evidence *confirms* each one and the reading is still
   * wrong.
   */
  | { operator: "wrong-period"; target: string }
  /**
   * A real price rise dated to the month before it happened. Nothing
   * derives from the month, so this moves the relation and nothing
   * else: every figure on the page stays genuine.
   */
  | { operator: "wrong-rise-month"; target: string }
  /** A true finding is missing entirely. Nothing on the page is wrong. */
  | { operator: "dropped-claim"; target: string };

/**
 * What the participant saw, and what they should have seen.
 *
 * A union rather than one shape with optional fields, because what a
 * correct report says is a different *kind* of answer per operator —
 * an amount for a corrupted value, a period for a mis-read relation,
 * the missing claim's amount for an omission. One shape carrying all
 * three would let scoring compare an answer against a field the
 * operator never touched.
 */
export type SeededTruth =
  | { operator: "wrong-amount"; target: string; gold: Money; shown: Money }
  | { operator: "wrong-period"; target: string; gold: Period; shown: Period }
  | { operator: "wrong-rise-month"; target: string; gold: IsoMonth; shown: IsoMonth }
  /**
   * `shown: null` is not "no answer recorded" — it is the finding
   * itself, and the reason an omission cannot be checked the way a
   * wrong value can.
   */
  | { operator: "dropped-claim"; target: string; gold: Money; shown: null };

export interface Seeded {
  report: RunReport;
  truth: SeededTruth;
}

/**
 * Money as integer pence, never as a float. `Money` is a canonical
 * two-decimal string (types.ts); anything else is refused rather than
 * shifted, because a seed that wrote "NaN" into a report would be a
 * study of the harness and not of the person.
 */
export function toPence(amount: Money): number {
  const match = /^(-?)(\d+)\.(\d{2})$/.exec(amount);
  if (match === null) {
    throw new Error(`cannot shift non-canonical amount ${amount}`);
  }
  const [, sign, pounds, pence] = match;
  const value = Number(pounds) * 100 + Number(pence);
  return sign === "-" ? -value : value;
}

function fromPence(pence: number): Money {
  const sign = pence < 0 ? "-" : "";
  const abs = Math.abs(pence);
  return `${sign}${Math.floor(abs / 100)}.${String(abs % 100).padStart(2, "0")}`;
}

/** Integer division rounded half away from zero. */
function divide(pence: number, by: number): number {
  const sign = pence < 0 ? -1 : 1;
  return sign * Math.floor((Math.abs(pence) * 2 + by) / (2 * by));
}

const PERIODS_PER_YEAR: Record<Period, number> = {
  weekly: 52,
  monthly: 12,
  quarterly: 4,
  yearly: 1,
};

/**
 * A wrong amount that is wrong by an order of magnitude rather than a
 * digit.
 *
 * Detectability is the thing being measured, so the size of the error
 * is a design decision and not an implementation detail. A £12.99
 * subscription shown as £129.90 is checkable against the evidence
 * without arithmetic, which keeps the invention condition a fair test
 * of *reading the evidence* rather than of doing sums in one's head —
 * the latter would measure numeracy and call it detection.
 */
function tenfold(amount: Money): Money {
  return fromPence(toPence(amount) * 10);
}

/**
 * The period a real one is misread as: always the next coarser one.
 *
 * Coarser rather than finer on purpose. It makes the annualised figure
 * *smaller* — a report telling you that you spend less than you do —
 * which is the direction that costs someone money and the direction a
 * reader is least likely to challenge. It is also the shape #568's
 * rung 1 found in prose: a real figure read under a relation that
 * flatters the position.
 *
 * `yearly` has no coarser neighbour, so it is refused rather than
 * wrapped around to something arbitrary. A seed nobody can defend as
 * "a plausible misreading" is not measuring detection.
 */
function misread(period: Period): Period {
  const coarser: Partial<Record<Period, Period>> = {
    weekly: "monthly",
    monthly: "quarterly",
    quarterly: "yearly",
  };
  const next = coarser[period];
  if (next === undefined) {
    throw new Error(`cannot misread ${period} as a coarser period`);
  }
  return next;
}

/**
 * The month before this one, as `YYYY-MM`.
 *
 * Arithmetic on the string rather than a `Date`, because a `Date`
 * built from a month string is midnight UTC and a machine an hour west
 * of it reads the previous month back.
 */
function monthBefore(month: IsoMonth): IsoMonth {
  const match = /^(\d{4})-(\d{2})$/.exec(month);
  if (match === null) {
    throw new Error(`cannot step back from non-canonical month ${month}`);
  }
  const year = Number(match[1]);
  const index = Number(match[2]);
  const [before, y] = index === 1 ? [12, year - 1] : [index - 1, year];
  return `${y}-${String(before).padStart(2, "0")}`;
}

/**
 * The report's own totals, re-derived from its rows. A seeded report
 * whose summary still described the clean one would let a participant
 * "detect" the seed from the arithmetic, not from the evidence.
 */
function recomputeSummary(faulty: RunReport, findings: RecurringFinding[]): void {
  const annualised = findings.reduce(
    (total, finding) => total + toPence(finding.annualised),
    0,
  );
  faulty.summary.recurring_count = findings.length;
  faulty.summary.price_rises = findings.filter((f) => f.price_rise !== null).length;
  faulty.summary.annualised_total = fromPence(annualised);
  faulty.summary.monthly_equivalent = fromPence(divide(annualised, 12));
}

/** Seed one named error, returning the faulty report and the truth. */
export function seed(report: RunReport, error: SeededError): Seeded {
  // Deep copy first: a harness that mutated the caller's report would
  // seed its own control, and every participant would see the faulty
  // version while the record said half of them saw a clean one.
  const faulty: RunReport = JSON.parse(JSON.stringify(report)) as RunReport;
  const findings: RecurringFinding[] = faulty.recurring ?? [];
  const index = findings.findIndex((finding) => finding.merchant === error.target);
  const found = index === -1 ? undefined : findings[index];
  if (found === undefined) {
    // Never a silent no-op. A "seeded" report identical to its control
    // records every participant as having missed an error that was
    // never there, and nothing downstream could tell that from a real
    // miss.
    throw new Error(
      `no recurring finding for ${error.target}: cannot seed ${error.operator}`,
    );
  }

  const gold = found.amount_current;

  if (error.operator === "dropped-claim") {
    findings.splice(index, 1);
    recomputeSummary(faulty, findings);
    return {
      report: faulty,
      truth: { operator: error.operator, target: error.target, gold, shown: null },
    };
  }

  if (error.operator === "wrong-period") {
    // Nothing a participant can check moves: not the amount, not the
    // price rise, not one transaction, not the median interval. Only
    // the relation changes, and the figures the relation *implies*
    // follow it — an annualised total still reading £155.88 beside
    // "quarterly" would betray the seed as arithmetic rather than
    // being caught as a misreading.
    const goldPeriod = found.period;
    const shownPeriod = misread(goldPeriod);
    const periods = PERIODS_PER_YEAR[shownPeriod];
    found.period = shownPeriod;
    found.annualised = fromPence(toPence(found.amount_current) * periods);
    if (found.price_rise !== null) {
      found.price_rise.extra_per_year = fromPence(
        (toPence(found.price_rise.to) - toPence(found.price_rise.from)) * periods,
      );
    }
    recomputeSummary(faulty, findings);
    return {
      report: faulty,
      truth: {
        operator: error.operator,
        target: error.target,
        gold: goldPeriod,
        shown: shownPeriod,
      },
    };
  }

  if (error.operator === "wrong-rise-month") {
    // A finding with no rise has no relation to misread. Seeding one
    // would be inventing a claim under the name of a mis-relation, and
    // the truth record would describe an error the report does not
    // contain.
    if (found.price_rise === null) {
      throw new Error(
        `no price rise for ${error.target}: cannot seed ${error.operator}`,
      );
    }
    const goldMonth = found.price_rise.month;
    const shownMonth = monthBefore(goldMonth);
    // The seeded month must carry a payment, because EvidenceRow
    // highlights the chip whose date starts with it. A month with no
    // chip highlights nothing, and the participant is asked to catch a
    // claim the report never points at.
    const highlighted = found.evidence.transactions.some((txn) =>
      txn.date.startsWith(shownMonth),
    );
    if (!highlighted) {
      throw new Error(
        `no payment in ${shownMonth}: cannot misdate ${error.target}'s rise to a month with no chip`,
      );
    }
    found.price_rise.month = shownMonth;
    // Deliberately nothing else: not the amounts, not `extra_per_year`,
    // not the totals. Nothing derives from when a rise happened, which
    // is what makes this the purest mis-relation available — the only
    // wrong thing on the page is a relationship between right things.
    recomputeSummary(faulty, findings);
    return {
      report: faulty,
      truth: {
        operator: error.operator,
        target: error.target,
        gold: goldMonth,
        shown: shownMonth,
      },
    };
  }

  if (error.operator === "wrong-amount") {
    // Every field derived from the amount moves with it, or the row
    // contradicts itself on screen (£129.90 beside "£10.99 → £12.99")
    // and the seed is detectable without opening the evidence.
    const shown = tenfold(gold);
    const periods = PERIODS_PER_YEAR[found.period];
    found.amount_current = shown;
    found.annualised = fromPence(toPence(shown) * periods);
    if (found.price_rise !== null) {
      found.price_rise.to = shown;
      found.price_rise.extra_per_year = fromPence(
        (toPence(shown) - toPence(found.price_rise.from)) * periods,
      );
    }
    recomputeSummary(faulty, findings);
    return {
      report: faulty,
      truth: { operator: error.operator, target: error.target, gold, shown },
    };
  }

  // Exhaustive by construction: an operator nobody implemented must
  // not fall through to whichever branch happens to be last. A seed
  // that quietly corrupted an amount when asked for something else
  // would be recorded as the operator that was asked for, and the
  // study would score answers against the wrong truth.
  const unreachable: never = error;
  throw new Error(`no seeding rule for ${JSON.stringify(unreachable)}`);
}
