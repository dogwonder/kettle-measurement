// #431: a finished session, and the two things it must not be.
//
// The issue's "done when" asks for two properties that pull against
// convenience. Raw participant data is separated from product
// telemetry, and the study reproduces from committed fixtures. Both are
// properties of what gets written down, so they are tested on the
// written form rather than promised in a README.

import { describe, expect, it } from "vitest";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { CONSENT, stamp } from "./study-consent";
import { begin, record, rebuild, scores, type StudyCorpus } from "./study-record";
import type { StudyLetter } from "./study-letter";
import type { Response } from "./study-response";
import { plan } from "./study-session";
import type { RunReport } from "./types";

const repoRoot = join(__dirname, "../../..");

function corpus(): RunReport[] {
  return Array.from({ length: 10 }, (_, i) => {
    const name = `report-${String(i + 1).padStart(2, "0")}.json`;
    return JSON.parse(
      readFileSync(join(repoRoot, "fixtures/study", name), "utf8"),
    ) as RunReport;
  });
}

const consent = stamp(CONSENT, "2026-09-01T10:00:00Z");

/** The letters, with the audit's signed read of which the pipeline got wrong. */
function letterCorpus(): StudyCorpus {
  const dir = join(repoRoot, "fixtures/study/letters");
  const letters = readdirSync(dir)
    .filter((name) => /^letter-\d{2}\.json$/.test(name))
    .sort()
    .map((name) => JSON.parse(readFileSync(join(dir, name), "utf8")) as StudyLetter);
  const audit = JSON.parse(
    readFileSync(join(repoRoot, "fixtures/study/audit-letters.json"), "utf8"),
  ) as { letters: Record<string, { clean: boolean | null }> };
  return {
    material: "letters",
    letters,
    unclean: new Set(
      Object.entries(audit.letters)
        .filter(([, entry]) => entry.clean === false)
        .map(([id]) => id),
    ),
  };
}

function answer(overrides: Partial<Response> = {}): Response {
  return {
    verdict: "accept",
    flagged: [],
    offered: [],
    correction: null,
    confidence_before: 3,
    confidence_after: 3,
    opened: [],
    elapsed_ms: 45_000,
    ...overrides,
  };
}

describe("a participant's transcript", () => {
  it("holds answers and no report, so participant data never carries the product's", () => {
    const started = begin("p07", corpus(), consent);
    const filled = started.responses.reduce<typeof started>(
      (transcript, _, index) => record(transcript, index, answer()),
      started,
    );

    const written = JSON.stringify(filled);
    // Not "no merchant names" — a participant may well type one, and
    // that is their answer. What must not be here is the run: a
    // transcript carrying findings would put the product's output in
    // the file consent was given for, and the two have different
    // handling, different retention and different owners.
    expect(written).not.toContain("kettle/run-report@0");
    expect(written).not.toContain("recurring");
    expect(written).not.toContain("blake3:");

    // Everything needed to say what this person saw, and nothing more:
    // the session rebuilds from the id, so the reports do not travel.
    expect(filled.participant).toBe("p07");
    expect(filled.arm).toBe(plan("p07", corpus()).arm);
    expect(filled.corpus).toEqual(corpus().map((report) => report.run.id));
  });

  it("refuses a session that cannot be reconstructed, rather than scoring a guess", () => {
    const transcript = begin("p07", corpus(), consent);

    // The same ten reports: the plan is deterministic, so the analysis
    // sees exactly what the participant saw.
    expect(rebuild(transcript, corpus()).tasks.map((t) => t.document)).toEqual(
      plan("p07", corpus()).tasks.map((t) => t.document),
    );

    // A different corpus draws a different session. Scoring answers
    // against reports nobody was shown would produce a table with no
    // sign that anything had gone wrong.
    const swapped = corpus();
    swapped[3] = { ...(swapped[3] as RunReport), run: { ...(swapped[3] as RunReport).run, id: "study-99" } };
    expect(() => rebuild(transcript, swapped)).toThrow(/corpus/);
  });

  it("refuses a session whose draw has moved under it, rather than re-drawing", () => {
    // The corpus check above catches a corpus that changed. It cannot
    // catch a *judgement* that changed: `planLetters` draws its clean
    // controls only from letters the audit calls clean (#577), so
    // correcting the audit moves the draw while the corpus stays
    // identical — and the answers then score against documents the
    // participant never saw.
    //
    // That is not hypothetical. It happened to `a01` the same day it
    // was sat, and the table it produced looked perfectly ordinary:
    // three of the ten rows were simply about other letters. Nothing
    // said so. The transcript therefore records the ten documents it
    // showed, and a rebuild that disagrees refuses.
    const letters = letterCorpus();
    const transcript = begin("a09", letters, consent);
    expect(transcript.drawn).toHaveLength(10);
    expect(rebuild(transcript, letters).tasks.map((t) => t.document)).toEqual(transcript.drawn);

    // The same corpus, with the letter this session used as a clean
    // control now judged unclean: the pool is identical and only the
    // eligibility moved, which is exactly what correcting an audit
    // does.
    const control = rebuild(transcript, letters).tasks.find((t) => t.class === "clean");
    expect(control).toBeDefined();
    const stricter = {
      ...letters,
      unclean: new Set([...letters.unclean, control!.document]),
    };
    expect(() => rebuild(transcript, stricter)).toThrow(/showed|drew|drawn/i);
  });

  it("refuses to open a session without consent recorded against its words", () => {
    // Consent is not a boolean: what somebody agreed to is the text
    // they were shown, so a transcript names its version and the digest
    // of the words rendered under that version, or the agreement cannot
    // be checked afterwards. See `study-consent.test.ts` for why the
    // version alone was not enough.
    expect(() => begin("p07", corpus(), { ...consent, version: "" })).toThrow(
      /consent/,
    );
    expect(() => begin("p07", corpus(), { ...consent, digest: "" })).toThrow(
      /consent/,
    );
    expect(() => begin("p07", corpus(), { ...consent, given_at: "" })).toThrow(
      /consent/,
    );
  });

  it("derives scores rather than storing them, so a scoring change re-reads the same file", () => {
    const started = begin("p07", corpus(), consent);
    const session = rebuild(started, corpus());
    const seeded = session.tasks.find((task) => task.class === "invention");
    const at = seeded?.index ?? 0;

    const filled = record(
      started,
      at,
      answer({ verdict: "reject", flagged: seeded?.truth?.target ? [seeded.truth.target] : [] }),
    );
    expect(JSON.stringify(filled)).not.toContain("correct-detection");

    const table = scores(filled, corpus());
    // One row per answered task, and unanswered tasks are absent
    // rather than defaulted — a participant who stopped early has not
    // accepted the rest.
    expect(table).toHaveLength(1);
    expect(table[0]?.outcome).toBe("correct-detection");
    expect(table[0]?.class).toBe("invention");
  });
});
