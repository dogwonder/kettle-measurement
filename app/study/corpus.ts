// #431: the committed study corpus, as the harness sees it.
//
// Ten synthetic statements and the ten reports Kettle produced from
// them, imported from `fixtures/study/` rather than copied here. The
// order is the filename order, and it matters: a transcript pins the
// corpus by run id, so a corpus assembled in a different order would be
// refused rather than silently scored against a different session.

import type { StudyLetter } from "../src/lib/study-letter";
import type { RunReport } from "../src/lib/types";

const reportFiles = import.meta.glob("../../fixtures/study/report-*.json", {
  eager: true,
  import: "default",
}) as Record<string, RunReport>;

const statementFiles = import.meta.glob("../../fixtures/study/statement-*.csv", {
  eager: true,
  query: "?raw",
  import: "default",
}) as Record<string, string>;

/** The ten reports, in filename order. */
export const corpus: RunReport[] = Object.keys(reportFiles)
  .sort()
  .map((path) => reportFiles[path] as RunReport);

/**
 * The letters a person's read of `fixtures/study/audit-letters.json`
 * records as carrying an error the pipeline made on its own (#577).
 *
 * Imported rather than restated: the file is the signed judgement, and
 * a second list here is how a harness comes to disagree with the audit
 * describing its own corpus.
 */
const auditFile = (await import("../../fixtures/study/audit-letters.json")) as unknown as {
  default: { letters: Record<string, { clean: boolean | null }> };
};

export const uncleanLetters: ReadonlySet<string> = new Set(
  Object.entries(auditFile.default.letters)
    .filter(([, entry]) => entry.clean === false)
    .map(([id]) => id),
);

const letterFiles = import.meta.glob("../../fixtures/study/letters/letter-*.json", {
  eager: true,
  import: "default",
}) as Record<string, StudyLetter>;

/**
 * The letter corpus (27 August 2026), in filename order: synthetic
 * letters from the `kettle-examples` generator, run through the letter
 * pack for real by `crates/runner/examples/study_letters.rs`. See
 * `fixtures/study/README.md`.
 */
export const letters: StudyLetter[] = Object.keys(letterFiles)
  .sort()
  .map((path) => letterFiles[path] as StudyLetter);

const statements = new Map(
  Object.entries(statementFiles).map(([path, text]) => [
    path.slice(path.lastIndexOf("/") + 1),
    text,
  ]),
);

/**
 * The statement a report was made from.
 *
 * A participant cannot notice that a commitment is missing from a
 * report by reading the report — there is no row to inspect. So the
 * source document is part of the task, and an omission that could only
 * be caught by second sight would measure nothing. Refused rather than
 * defaulted to empty: a task showing an empty statement would score
 * every omission as undetected and look exactly like a finding.
 */
export function statementFor(report: RunReport): string {
  const text = statements.get(report.run.input.file);
  if (text === undefined) {
    throw new Error(
      `no statement ${report.run.input.file} beside report ${report.run.id}`,
    );
  }
  return text;
}

export interface StatementRow {
  date: string;
  description: string;
  amount: string;
}

/**
 * The statement's rows. Split on commas without quote handling, which
 * is honest for this corpus — `make-statements.py` writes three plain
 * fields and `audit.py` reads them the same way — and refuses a row
 * that is not three fields rather than shifting the columns along.
 */
export function rowsOf(csv: string): StatementRow[] {
  const [header, ...lines] = csv.trim().split("\n");
  if (header?.trim() !== "Date,Description,Amount") {
    throw new Error(`unexpected statement header: ${header}`);
  }
  return lines.map((line) => {
    const fields = line.split(",");
    if (fields.length !== 3) {
      throw new Error(`statement row is not three fields: ${line}`);
    }
    const [date = "", description = "", amount = ""] = fields;
    return { date, description, amount };
  });
}
