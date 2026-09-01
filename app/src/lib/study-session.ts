// #431: what one participant sees, decided before they arrive.
//
// The protocol frozen on #431 promises three things at once — that task
// order and seed assignment are randomised, that the two arms come out
// balanced at a fixed n of 20, and that the analysis can reconstruct
// exactly what any participant was shown. `Math.random` gives the first
// and takes away the third, so every draw here comes from a generator
// seeded by the participant's own id.
//
// ## Why the mix is what it is
//
// #568's rung 1 measured the invention floor over filed charity
// accounts at the schema-only rung: 1 in 470 judgeable claims, 0.21%,
// Wilson [0.04%, 1.20%]. A study seeded mostly with invented figures
// would measure detection of an error class that barely occurs.
//
// What that run's prose arm produced instead was a real figure read
// under the wrong relation — funds brought forward presented as income,
// turning a £62,556 gain into a £49k loss. Every figure genuine, every
// quote verbatim, the reading wrong. So the classes here are separated
// by what *checking the evidence does*: it refutes an invention, it
// confirms a mis-relation, and there is nothing to check for an
// omission. Three, three, two, and two clean.

import { seed, type SeededError, type SeededTruth } from "./study-fixtures";
import type { RunReport } from "./types";

/**
 * Which presentation a participant sees for their whole session.
 *
 * Condition 3 and condition 4 of the issue's four. The arm holds for
 * every task rather than switching mid-session: a participant who saw
 * emphasised evidence on some reports and not others would carry what
 * they learned from one into the next, and the between-arm comparison
 * would be measuring the switch.
 */
export type Arm = "evidence" | "emphasised";

export type TaskClass = "invention" | "mis-relation" | "omission" | "clean";

export interface Task {
  /** Position in the session, from zero. */
  index: number;
  /** Which clean report this task was built from. */
  document: string;
  class: TaskClass;
  /** What the participant sees. */
  report: RunReport;
  /** The gold answer, or `null` for a clean report. */
  truth: SeededTruth | null;
  /**
   * The claim whose evidence the emphasised arm points at, or `null`
   * for a report with no claims at all.
   *
   * Chosen from the *clean* report, which is the whole point — see
   * `emphasisFor`.
   */
  emphasis: string | null;
}

export interface SessionPlan {
  participant: string;
  /**
   * Where this participant sits in the enrolment order.
   *
   * Carried rather than left implicit in the id, because it is part of
   * the protocol: the design is fixed-n at twenty with no interim look
   * and one declared futility stop at n = 10, and an analysis that
   * cannot place a session in the order cannot apply either rule to it.
   */
  enrolment: number;
  arm: Arm;
  tasks: Task[];
}

/** Ten tasks: three inventions, three mis-relations, two omissions, two clean. */
export const MIX: TaskClass[] = [
  "invention",
  "invention",
  "invention",
  "mis-relation",
  "mis-relation",
  "mis-relation",
  "omission",
  "omission",
  "clean",
  "clean",
];

/** FNV-1a, so a participant id becomes the same seed on every machine. */
export function hash(text: string): number {
  let value = 0x811c9dc5;
  for (let i = 0; i < text.length; i += 1) {
    value ^= text.charCodeAt(i);
    value = Math.imul(value, 0x01000193);
  }
  return value >>> 0;
}

/** mulberry32: small, seeded, and the same sequence everywhere. */
export function generator(seedValue: number): () => number {
  let state = seedValue >>> 0;
  return () => {
    state = (state + 0x6d2b79f5) >>> 0;
    let t = state;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/** Fisher–Yates against a seeded generator, never in place. */
export function shuffled<T>(items: T[], random: () => number): T[] {
  const out = [...items];
  for (let i = out.length - 1; i > 0; i -= 1) {
    const j = Math.floor(random() * (i + 1));
    [out[i], out[j]] = [out[j] as T, out[i] as T];
  }
  return out;
}

/**
 * The participant's place in the enrolment order, from their id.
 *
 * Required rather than optional. The design is fixed-n with no interim
 * look and one declared futility stop at n = 10, so where a participant
 * sits in the order is part of the protocol rather than bookkeeping — a
 * session that cannot be placed against the stopping rule cannot be
 * analysed under it. Refusing here is the difference between finding
 * that out now and finding it out after the twentieth person has gone
 * home.
 */
export function enrolment(participant: string): number {
  const digits = /(\d+)/.exec(participant);
  if (digits === null) {
    throw new Error(
      `participant ${participant} carries no enrolment number: the design is fixed-n with a futility stop at n = 10, and a session with no place in that order cannot be analysed under it`,
    );
  }
  return Number(digits[1]);
}

/**
 * Every participant is in condition 3.
 *
 * Decided 26 August 2026, on an inconsistency in #431's own frozen
 * pre-registration. It promises the primary sixty pairs — invention
 * against mis-relation, paired, within participant, in condition 3 —
 * and separately splits twenty participants across two arms at ten
 * each. Both cannot hold: ten participants in condition 3 give thirty
 * pairs, and the minimum detectable difference moves from thirty points
 * to about forty, which is past the gap the study exists to act on.
 *
 * So condition 4 is not run here. It was already declared exploratory,
 * predicted to show no movement, and admitted to seeing only a
 * ~40-point difference; spending half the sample to buy that costs the
 * primary the power it was designed around. `Arm` keeps both values and
 * the emphasised presentation stays built and tested, so condition 4
 * becomes its own study if the primary result makes it worth one —
 * which is a different thing from it being half-run here.
 *
 * Block randomisation in pairs is gone with it. It was balancing an
 * assignment nobody is making now, and git holds it for whoever needs
 * it back.
 */
export function armFor(_participant: string): Arm {
  return "evidence";
}

/** The merchants in a report that can carry a given operator. */
function eligible(report: RunReport, operator: SeededError["operator"]): string[] {
  return (report.recurring ?? [])
    .filter((finding) => {
      if (operator === "wrong-period") return finding.period !== "yearly";
      if (operator === "wrong-rise-month") {
        if (finding.price_rise === null) return false;
        const [year, month] = finding.price_rise.month.split("-").map(Number);
        const before =
          month === 1
            ? `${(year ?? 0) - 1}-12`
            : `${year}-${String((month ?? 1) - 1).padStart(2, "0")}`;
        return finding.evidence.transactions.some((txn) => txn.date.startsWith(before));
      }
      return true;
    })
    .map((finding) => finding.merchant);
}

/**
 * Both mis-relation operators appear in a session, never one three
 * times.
 *
 * Three identical seeds would let a participant learn the pattern on
 * the first and answer the rest without reading — which scores as
 * detection and is not.
 */
function misRelationOperators(
  documents: RunReport[],
  random: () => number,
): SeededError["operator"][] {
  const canMisdate = documents.map(
    (report) => eligible(report, "wrong-rise-month").length > 0,
  );
  const index = canMisdate.indexOf(true);
  if (index === -1) {
    throw new Error(
      "no mis-relation report carries a datable price rise: a session would seed one operator three times",
    );
  }
  // Exactly one of each, and the remaining task decided by the draw.
  // Both operators are guaranteed present, and which one appears twice
  // varies between participants rather than being fixed by the code.
  const operators: SeededError["operator"][] = documents.map(() => "wrong-period");
  operators[index] = "wrong-rise-month";
  const spare = operators.findIndex(
    (operator, at) => operator === "wrong-period" && at !== index,
  );
  if (spare !== -1 && canMisdate[spare] === true && random() < 0.5) {
    operators[spare] = "wrong-rise-month";
  }
  return operators;
}

/**
 * The claim the emphasised presentation points at, drawn rather than
 * ranked.
 *
 * The first rule here was the claim that costs most if it is wrong —
 * the largest annualised figure — ranked on the *clean* report so the
 * seed could not choose it. That was not enough. Ranking on the clean
 * report stops the seed choosing the emphasis; it does not stop the
 * seed changing which row *looks* biggest. An invention multiplies its
 * row by ten, so on those three tasks the emphasised row is no longer
 * the largest figure on the page, while on the other seven it is. Over
 * ten reports that is learnable, and it marks exactly one of the two
 * classes the primary compares.
 *
 * Drawing removes the rule instead of muffling it. Nothing about the
 * emphasis can be predicted from the page, so there is no invariant for
 * a seed to break. The cost is "high-risk", and it is worth paying: at
 * ten per arm the question condition 4 can actually answer is whether
 * one already-open disclosure changes what a person does, not which
 * claim deserves one.
 *
 * Drawn from what the report still shows, so the emphasis always
 * exists. A dropped claim simply is not a candidate, and since any row
 * may be marked there is nothing in that for a participant to read.
 */
function emphasisFor(shown: RunReport, random: () => number): string | null {
  const claims = (shown.recurring ?? []).map((finding) => finding.merchant);
  if (claims.length === 0) return null;
  return claims[Math.floor(random() * claims.length)] as string;
}

const OPERATOR_FOR: Record<Exclude<TaskClass, "clean" | "mis-relation">, SeededError["operator"]> =
  {
    invention: "wrong-amount",
    omission: "dropped-claim",
  };

/**
 * One participant's session: which report carries which error, in which
 * order, under which arm.
 *
 * Deterministic given the participant and the corpus, so a finished
 * session can be rebuilt from the id alone and its randomisation
 * checked by somebody who was not there.
 */
export function plan(participant: string, corpus: RunReport[]): SessionPlan {
  if (corpus.length < MIX.length) {
    // Ten seeds spread over fewer reports would show the same merchants
    // again and again, and a participant would end up studying the
    // harness rather than reading a report.
    throw new Error(
      `a session needs ${MIX.length} reports, one per task; the corpus holds ${corpus.length} reports`,
    );
  }

  const random = generator(hash(participant));
  const documents = shuffled(corpus, random).slice(0, MIX.length);
  const classes = shuffled(MIX, random);

  const misRelationDocuments = documents.filter(
    (_, at) => classes[at] === "mis-relation",
  );
  const misRelation = misRelationOperators(misRelationDocuments, random);
  let misRelationSeen = 0;
  /** Operator-and-merchant pairs already used in this session. */
  const used = new Set<string>();

  const tasks: Task[] = classes.map((taskClass, index) => {
    const report = documents[index] as RunReport;
    const document = report.run.id;
    if (taskClass === "clean") {
      return {
        index,
        document,
        class: taskClass,
        report,
        truth: null,
        emphasis: emphasisFor(report, random),
      };
    }

    const assigned =
      taskClass === "mis-relation"
        ? (misRelation[misRelationSeen++] as SeededError["operator"])
        : OPERATOR_FOR[taskClass];
    // A mis-relation can be told either way, so if the report has no
    // merchant left for the operator it drew, its sibling gets the
    // task rather than the session failing. The guarantee that matters
    // — both operators appear — was settled when they were assigned.
    const siblings: SeededError["operator"][] =
      taskClass === "mis-relation"
        ? assigned === "wrong-period"
          ? ["wrong-period", "wrong-rise-month"]
          : ["wrong-rise-month", "wrong-period"]
        : [assigned];
    // A merchant this session has already carried under this operator
    // is not a candidate again. A participant who has just caught
    // Netflix's rise dated a month early does not read the second one,
    // they recognise it — and a recognition scores as a detection it
    // is not.
    const open = siblings
      .map((candidate) => ({
        operator: candidate,
        targets: eligible(report, candidate).filter(
          (merchant) => !used.has(`${candidate}:${merchant}`),
        ),
      }))
      .filter((candidate) => candidate.targets.length > 0);
    const choice = open[0];
    if (choice === undefined) {
      throw new Error(
        `report ${document} has no merchant left for ${siblings.join(" or ")}: every one it can take is already seeded in this session`,
      );
    }
    const operator = choice.operator;
    const target = choice.targets[
      Math.floor(random() * choice.targets.length)
    ] as string;
    used.add(`${operator}:${target}`);
    const seeded = seed(report, { operator, target } as SeededError);
    return {
      index,
      document,
      class: taskClass,
      report: seeded.report,
      truth: seeded.truth,
      emphasis: emphasisFor(seeded.report, random),
    };
  });

  return { participant, enrolment: enrolment(participant), arm: armFor(participant), tasks };
}
