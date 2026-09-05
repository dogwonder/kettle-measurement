// The runner's JSON shapes, mirrored in exactly one place (#42).
// Sources of truth: fixtures/run-01/results.json (kettle/run-report@0),
// fixtures/run-01/actions.json (kettle/proposed-actions@0), and the
// command/event surface in brief §7b. When the runner's emitters land
// (E5/E6) they become authoritative and this file follows them.
//
// Money is strings, never numbers: JSON numbers become floats in
// JavaScript; amounts stay exact by staying strings end to end.

/** An exact decimal amount as text, e.g. "10.99". Never parseFloat it. */
export type Money = string;

/** A calendar date as "YYYY-MM-DD". */
export type IsoDate = string;

/** A month as "YYYY-MM". */
export type IsoMonth = string;

export type Confidence = "high" | "medium" | "low";

export type Period = "weekly" | "monthly" | "quarterly" | "yearly";

export interface Transaction {
  date: IsoDate;
  amount: Money;
}

// ---------------------------------------------------------------------------
// kettle/run-report@0

export interface RunReport {
  schema: "kettle/run-report@0";
  run: RunInfo;
  summary: RunSummary;
  recurring: RecurringFinding[];
  regular_spend: RegularSpend[];
  income: Income[];
  needs_review: NeedsReview[];
  check_yourself: CheckYourself[];
}

export interface RunInfo {
  id: string;
  pack: { id: string; version: string; title: string };
  input: {
    file: string;
    rows: number;
    period: { from: IsoDate; to: IsoDate };
    hash: string;
  };
  model: { tier: string; id: string };
  started: string;
  finished: string;
  currency: string;
}

export interface RunSummary {
  recurring_count: number;
  annualised_total: Money;
  monthly_equivalent: Money;
  price_rises: number;
  needs_review_count: number;
  note: string;
}

export interface RecurringFinding {
  merchant: string;
  raw_merchant: string;
  /** e.g. "subscription", "utility" — open set, the pack's schema owns it. */
  kind: string;
  /** e.g. "streaming", "energy" — open set, the pack's schema owns it. */
  category: string;
  period: Period;
  amount_current: Money;
  annualised: Money;
  confidence: Confidence;
  price_rise: PriceRise | null;
  evidence: Evidence;
}

export interface PriceRise {
  from: Money;
  to: Money;
  month: IsoMonth;
  extra_per_year: Money;
}

export interface Evidence {
  /** One plain-language sentence: why this finding exists. */
  reason: string;
  /** Null when there aren't two payments to measure between. */
  interval_days: { median: number; spread: number } | null;
  transactions: Transaction[];
}

export interface RegularSpend {
  merchant: string;
  raw_merchant: string;
  kind: string;
  category: string;
  visits: number;
  total: Money;
  typical_visit: Money;
  confidence: Confidence;
}

export interface Income {
  merchant: string;
  raw_merchant: string;
  period: Period;
  amount: Money;
  confidence: Confidence;
}

export interface NeedsReview {
  raw_merchant: string;
  /** One plain-language sentence; reaches people, not logs. */
  reason: string;
  transactions: Transaction[];
  note: string;
}

export interface CheckYourself {
  about: string;
  why: string;
}

// ---------------------------------------------------------------------------
// kettle/proposed-actions@0

export interface ProposedActions {
  schema: "kettle/proposed-actions@0";
  run_id: string;
  /** The honesty note: approving only prepares, nothing happens by itself. */
  note: string;
  actions: ProposedAction[];
}

export type ActionKind =
  | "review_price_rise"
  | "review_subscription"
  | "check_renewal"
  | "calendar_reminder";

/** The runner-owned mark for a photographed line which did not read
 * the same way twice. Copy travels with the readings so every surface
 * asks the same question. */
export interface DisputedReading {
  read: string;
  also_read: string;
  message: string;
  instruction: string;
}

export interface ProposedAction {
  id: string;
  kind: ActionKind;
  title: string;
  detail: string;
  /** Shape varies by kind; screens read named fields defensively. */
  evidence: Record<string, string>;
  disputed: DisputedReading[];
  export: {
    /** Absent when the letter gave no date: text only, never an event
     * dated by Kettle. */
    ics?: { summary: string; date: IsoDate };
    text: string;
  };
  /** The runner only ever proposes; approved/dismissed is screen state. */
  status: "proposed";
}

/** What a person has decided about a proposed action — never persisted
 * by the runner, only by the review screen. */
export type ActionDecision = "proposed" | "approved" | "dismissed";

// ---------------------------------------------------------------------------
// Command + event surface (brief §7b) — implemented by app/src-tauri (#43).

export type RunId = string;

export interface PackSummary {
  id: string;
  version: string;
  name: string;
  /** Plain-language promise line for the task card. */
  description: string;
  /** Every MIME type this pack can read, across all its roles — what
   * the one OS file dialog filters on, and what tells someone early
   * that a dropped file is the wrong kind. Never use it to decide
   * which role a file satisfies; that is what `inputs` is for. */
  accepts: string[];
  /** The roles this pack declares, in manifest order. A union cannot
   * say which document is which, and a two-role pack needs one
   * labelled drop zone per role (#334). */
  inputs: PackInput[];
  /** The progress labels a run will report, in order — derived from
   * the pipeline by the runner (#244), never authored per pack here. */
  steps: string[];
  /** What the pack says for itself (#244): how long a run takes, what
   * it will do, and the words on the Run button. Always present — a
   * pack without one refused to load. */
  copy: PackCopyBlock;
  /** Whether a run proposes reminders to approve and export, read from
   * the manifest's `outputs` rather than assumed of every pack. */
  produces_actions: boolean;
}

/** Mirrors `CopyBlock` in crates/runner/src/packs.rs (#244). */
export interface PackCopyBlock {
  time: {
    kind: "varies";
    /** Tag-sized, beside the time class: "with statement size". */
    estimate: string;
    /** Explains why duration varies and how progress remains visible.
     * Never an absolute runtime carried from an earlier sitting. */
    note: string;
  };
  /** "What this run will do", numbered, in the pack's own words. */
  will: PackWillEntry[];
  /** The words on the Run button. */
  run_verb: string;
}

/** One numbered line of "what this run will do". */
export interface PackWillEntry {
  doing: string;
  detail: string;
  /** The progress-step labels this line covers, when it says. */
  steps?: string[];
}

/** One declared input role. Mirrors `PackInputSummary` in
 * app/src-tauri/src/core.rs. */
export interface PackInput {
  /** The binding key the runner takes. Never shown to anyone. */
  role: string;
  /** What a person is shown when asked for this document. */
  label: string;
  /** MIME types this role accepts, already filtered to what this
   * install can actually read. */
  accept: string[];
  /** Whether this role takes more than one file. */
  multiple: boolean;
  /** Several chosen files are ordered pages of one document. */
  pages: boolean;
}

export type RunStatus = "running" | "complete" | "error" | "cancelled";

export interface RunRecord {
  id: RunId;
  pack_id: string;
  status: RunStatus;
  input_files: string[];
  started: string;
  finished?: string;
  /** Present once status is "complete". */
  results?: RunReport;
  actions?: ProposedActions;
  /** Path to the run's self-contained report.html, for the iframe. */
  report_path?: string;
  /** Present once status is "error"; plain language. */
  error?: string;
  /**
   * Plain language, about the record rather than the run: an earlier
   * version of Kettle made this report, so it opens but is inert (#419).
   */
  notice?: string;
}

/** run://progress — the only thing that drives the step list. */
export interface RunProgressEvent {
  run_id: RunId;
  step_label: string;
  current: number;
  total: number;
}

export interface RunCompleteEvent {
  run_id: RunId;
}

/** run://cancelled — the confirmation that a run has stopped (#115).
 * `kept_data` is null when nothing was kept, and a sentence when
 * cleanup failed and something private may remain on the disk. */
export interface RunCancelledEvent {
  run_id: RunId;
  kept_data: string | null;
}

export interface RunErrorEvent {
  run_id: RunId;
  step: string;
  message: string;
}

export type ExportFormat = "ics" | "text";

// ---------------------------------------------------------------------------
// Parsing: the two runner documents cross a boundary (files on disk),
// so they're checked at the edge. Commands/events come from our own
// Rust over Tauri's typed IPC and are trusted as declared.

class ShapeError extends Error {}

function fail(path: string, want: string): never {
  throw new ShapeError(`${path}: expected ${want}`);
}

function obj(value: unknown, path: string): Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    fail(path, "an object");
  }
  return value as Record<string, unknown>;
}

function arr(value: unknown, path: string): unknown[] {
  if (!Array.isArray(value)) fail(path, "an array");
  return value;
}

function str(value: unknown, path: string): string {
  if (typeof value !== "string") fail(path, "a string");
  return value;
}

function num(value: unknown, path: string): number {
  if (typeof value !== "number") fail(path, "a number");
  return value;
}

/** Amounts must arrive as strings — a number here is a runner bug. */
function money(value: unknown, path: string): Money {
  if (typeof value !== "string") fail(path, "a money string, not a number");
  return value;
}

function transactions(value: unknown, path: string): Transaction[] {
  return arr(value, path).map((t, i) => {
    const o = obj(t, `${path}[${i}]`);
    return {
      date: str(o.date, `${path}[${i}].date`),
      amount: money(o.amount, `${path}[${i}].amount`),
    };
  });
}

export function parseRunReport(value: unknown): RunReport {
  const root = obj(value, "report");
  if (root.schema !== "kettle/run-report@0") {
    fail("report.schema", '"kettle/run-report@0"');
  }

  const run = obj(root.run, "run");
  num(obj(run.input, "run.input").rows, "run.input.rows");

  const summary = obj(root.summary, "summary");
  money(summary.annualised_total, "summary.annualised_total");
  money(summary.monthly_equivalent, "summary.monthly_equivalent");

  arr(root.recurring, "recurring").forEach((r, i) => {
    const o = obj(r, `recurring[${i}]`);
    const evidence = obj(o.evidence, `recurring[${i}].evidence`);
    money(o.amount_current, `recurring[${i}].amount_current`);
    money(o.annualised, `recurring[${i}].annualised`);
    transactions(evidence.transactions, `recurring[${i}].evidence.transactions`);
    if (o.price_rise !== null && o.price_rise !== undefined) {
      const rise = obj(o.price_rise, `recurring[${i}].price_rise`);
      money(rise.from, `recurring[${i}].price_rise.from`);
      money(rise.to, `recurring[${i}].price_rise.to`);
    }
  });

  arr(root.regular_spend, "regular_spend").forEach((r, i) => {
    const o = obj(r, `regular_spend[${i}]`);
    money(o.total, `regular_spend[${i}].total`);
    money(o.typical_visit, `regular_spend[${i}].typical_visit`);
  });

  arr(root.income, "income").forEach((r, i) => {
    money(obj(r, `income[${i}]`).amount, `income[${i}].amount`);
  });

  arr(root.needs_review, "needs_review").forEach((r, i) => {
    const o = obj(r, `needs_review[${i}]`);
    str(o.reason, `needs_review[${i}].reason`);
    transactions(o.transactions, `needs_review[${i}].transactions`);
  });

  arr(root.check_yourself, "check_yourself").forEach((r, i) => {
    const o = obj(r, `check_yourself[${i}]`);
    str(o.about, `check_yourself[${i}].about`);
    str(o.why, `check_yourself[${i}].why`);
  });

  return root as unknown as RunReport;
}

export function parseProposedActions(value: unknown): ProposedActions {
  const root = obj(value, "actions");
  if (root.schema !== "kettle/proposed-actions@0") {
    fail("actions.schema", '"kettle/proposed-actions@0"');
  }
  str(root.run_id, "actions.run_id");

  arr(root.actions, "actions").forEach((a, i) => {
    const o = obj(a, `actions[${i}]`);
    str(o.id, `actions[${i}].id`);
    str(o.title, `actions[${i}].title`);
    if (o.status !== "proposed") fail(`actions[${i}].status`, '"proposed"');
    // Older and ordinary actions omit an empty dispute list. Normalise
    // that compatible wire form before it reaches a screen.
    o.disputed ??= [];
    arr(o.disputed, `actions[${i}].disputed`).forEach((d, j) => {
      const dispute = obj(d, `actions[${i}].disputed[${j}]`);
      str(dispute.read, `actions[${i}].disputed[${j}].read`);
      str(dispute.also_read, `actions[${i}].disputed[${j}].also_read`);
      str(dispute.message, `actions[${i}].disputed[${j}].message`);
      str(dispute.instruction, `actions[${i}].disputed[${j}].instruction`);
    });
    const exported = obj(o.export, `actions[${i}].export`);
    if (exported.ics !== undefined) {
      const ics = obj(exported.ics, `actions[${i}].export.ics`);
      str(ics.summary, `actions[${i}].export.ics.summary`);
      str(ics.date, `actions[${i}].export.ics.date`);
    }
    str(exported.text, `actions[${i}].export.text`);
  });

  return root as unknown as ProposedActions;
}
