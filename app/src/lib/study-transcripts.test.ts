// #431: the check a transcript passes before it is published.
//
// Two halves, and the order matters. The rules are driven on
// constructed transcripts, so each one is shown to bite — a directory
// with nothing in it yet cannot demonstrate that, and this project has
// been caught once by a test that passed vacuously in CI by taking a
// branch it was never written to test. Then the same rules are applied
// to whatever is actually committed, which today is nothing and after
// recruitment is twenty files.

import { describe, expect, it } from "vitest";
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { CONSENT, stamp } from "./study-consent";
import { begin, record, scores, type StudyCorpus, type Transcript } from "./study-record";
import type { StudyLetter } from "./study-letter";
import { planLetters } from "./study-letter-session";
import type { Response } from "./study-response";
import { closeReading, faults, manifestFaults } from "./study-transcripts";
import type { RunReport } from "./types";

const repoRoot = join(__dirname, "../../..");
const transcriptsDir = join(repoRoot, "fixtures/study/transcripts");

/**
 * What a transcript file is called, matched positively.
 *
 * The directory also holds manifests — `READ.json`, `RETIRED.json` —
 * and "every .json except the one I know about" breaks the day a second
 * one arrives, which is exactly how it broke: `RETIRED.json` was read
 * as a transcript and refused by the floor. The same shape had already
 * cost `scripts/study-pilot.sh` its corpus check that morning.
 */
const TRANSCRIPT = /^[pa]\d{2}\.json$/;

function corpus(): RunReport[] {
  return Array.from({ length: 10 }, (_, i) => {
    const name = `report-${String(i + 1).padStart(2, "0")}.json`;
    return JSON.parse(
      readFileSync(join(repoRoot, "fixtures/study", name), "utf8"),
    ) as RunReport;
  });
}

function answer(overrides: Partial<Response> = {}): Response {
  return {
    verdict: "accept",
    flagged: [],
    offered: [],
    correction: null,
    confidence_before: 3,
    confidence_after: 4,
    opened: [],
    elapsed_ms: 45_000,
    ...overrides,
  };
}

/** A finished session, the way the harness writes one. */
function finished(participant = "p07"): Transcript {
  const started = begin(participant, corpus(), stamp(CONSENT, "2026-09-01T10:00:00Z"));
  return started.responses.reduce<Transcript>(
    (transcript, _, index) => record(transcript, index, answer()),
    started,
  );
}

/**
 * The letter corpus, which is deliberately bigger than the ten a session
 * draws: `begin` records the whole pool so `rebuild` can refuse a corpus
 * that is not the one the participant saw.
 */
/** Typed as the letters member, not the union: the caller reads
 * `.letters` off it, and a union hides that from the type checker. */
function letterCorpus(): Extract<StudyCorpus, { material: "letters" }> {
  const dir = join(repoRoot, "fixtures/study/letters");
  const letters = readdirSync(dir)
    .filter((name) => /^letter-\d{2}\.json$/.test(name))
    .sort()
    .map((name) => JSON.parse(readFileSync(join(dir, name), "utf8")) as StudyLetter);
  return { material: "letters", letters, unclean: uncleanLetters() };
}

/** The audit's signed read, as the harness and the scorer both use it. */
function uncleanLetters(): ReadonlySet<string> {
  const audit = JSON.parse(
    readFileSync(join(repoRoot, "fixtures/study/audit-letters.json"), "utf8"),
  ) as { letters: Record<string, { clean: boolean | null }> };
  return new Set(
    Object.entries(audit.letters)
      .filter(([, entry]) => entry.clean === false)
      .map(([id]) => id),
  );
}

function finishedLetters(participant = "p07"): Transcript {
  const started = begin(participant, letterCorpus(), stamp(CONSENT, "2026-09-01T10:00:00Z"));
  return started.responses.reduce<Transcript>(
    (transcript, _, index) => record(transcript, index, answer()),
    started,
  );
}

/** What a transcript looks like once it has been written and read back. */
function written(transcript: Transcript): unknown {
  return JSON.parse(JSON.stringify(transcript));
}

describe("the floor a transcript passes before publication", () => {
  it("passes what the harness itself produces", () => {
    // If the check refused the harness's own output, every later
    // example would be measuring the check against a fiction.
    expect(faults(written(finished()))).toEqual([]);
  });

  it("passes a letters session, whose corpus is larger than the ten it draws", () => {
    // The rule used to be "exactly ten", which was true of the
    // statements by coincidence — there are ten reports — and false of
    // every letters session, because `corpus` is the pool the plan was
    // drawn from and there are eighteen letters. The floor refused
    // every file the letter harness could produce, and the first pilot
    // session is what found it.
    expect(faults(written(finishedLetters()))).toEqual([]);
  });

  it("refuses a corpus too small to draw a session from", () => {
    const transcript = written(finished()) as Record<string, unknown>;
    transcript.corpus = (transcript.corpus as string[]).slice(0, 9);
    expect(faults(transcript).join(" ")).toMatch(/ten/);
  });

  it("refuses a file holding anything the consent form does not list", () => {
    const extra = { ...written(finished()) as Record<string, unknown>, email: "someone@example.com" };
    expect(faults(extra).join(" ")).toMatch(/consent form says/);

    const short = written(finished()) as Record<string, unknown>;
    delete short.corpus;
    expect(faults(short).join(" ")).toMatch(/consent form says/);
  });

  it("refuses a participant number that is not one", () => {
    // Not fussiness. `p01` is the whole of what connects a file to a
    // person, and it connects to nobody by design; a name in that field
    // would be the one thing the arrangement promises cannot happen.
    for (const participant of ["Rich", "p1", "p07-rich", ""]) {
      const transcript = { ...(written(finished()) as Record<string, unknown>), participant };
      expect(faults(transcript).join(" "), participant).toMatch(/not a number of the form/);
    }
  });

  it("refuses a consent stamp that does not match the words in force", () => {
    const transcript = written(finished()) as Record<string, unknown>;
    transcript.consent = { ...(transcript.consent as object), digest: "deadbeef" };
    expect(faults(transcript).join(" ")).toMatch(/digest/);

    // An older version is not a fault: it names words that were in
    // force then, and the analysis needs to be able to read it.
    const older = written(finished()) as Record<string, unknown>;
    older.consent = { version: "2026-08-26", digest: "ac6bc79c", given_at: "2026-08-26T09:00:00Z" };
    expect(faults(older)).toEqual([]);
  });

  it("refuses answers that could not have come from the harness", () => {
    // Deliberately off-type: every case here is a value the harness's
    // own types make unreachable, which is the point — the floor reads
    // a file somebody could have hand-edited, where no type survives.
    const cases: Array<[string, Record<string, unknown>, RegExp]> = [
      ["a verdict nobody was offered", { verdict: "maybe" }, /verdict/],
      ["a confidence off the scale", { confidence_before: 9 }, /confidence_before/],
      ["a negative duration", { elapsed_ms: -1 }, /elapsed_ms/],
    ];
    for (const [name, override, pattern] of cases) {
      const transcript = written(finished()) as Record<string, unknown>;
      (transcript.responses as unknown[])[0] = { ...answer(), ...override };
      expect(faults(transcript).join(" "), name).toMatch(pattern);
    }

    // An unanswered slot is not a fault: somebody stopped early, and
    // that is an outcome rather than a broken file.
    const stopped = written(finished()) as Record<string, unknown>;
    (stopped.responses as unknown[])[7] = null;
    expect(faults(stopped)).toEqual([]);
  });

  it("points a long free-text answer at a person, and does not refuse it", () => {
    // Length was a refusal and should never have been. A participant
    // who explains themselves at length has done nothing wrong, and
    // their file may not be edited after they hand it over — so the
    // rule made a good answer unpublishable. Its own description said
    // such an answer "deserves a person's eye", which is a routing
    // decision, not a validity one.
    const transcript = written(finished()) as Record<string, unknown>;
    (transcript.responses as unknown[])[2] = answer({
      verdict: "reject",
      flagged: ["NIMBUS FITNESS"],
      offered: ["NIMBUS FITNESS"],
      correction: "x".repeat(301),
    });
    expect(faults(transcript), "a long answer is still a valid answer").toEqual([]);
    expect(closeReading(transcript).join(" ")).toMatch(/301-character/);

    // And an ordinary one asks for nothing.
    expect(closeReading(written(finished()))).toEqual([]);

    const ordinary = written(finished()) as Record<string, unknown>;
    (ordinary.responses as unknown[])[2] = answer({
      verdict: "reject",
      flagged: ["NIMBUS FITNESS"],
      offered: ["NIMBUS FITNESS"],
      correction: "£12.99 a month, not quarterly",
    });
    expect(faults(ordinary)).toEqual([]);
  });
});

describe("the manifest of who read what", () => {
  it("refuses a transcript nobody has signed for, and a signature for nothing", () => {
    const manifest = {
      schema: "kettle/study-read@0",
      read: [{ file: "p01.json", by: "Rich Holman", at: "2026-09-01" }],
    };
    expect(manifestFaults(manifest, ["p01.json"])).toEqual([]);
    expect(manifestFaults(manifest, ["p01.json", "p02.json"]).join(" ")).toMatch(
      /p02\.json is here and nobody has read it/,
    );
    expect(manifestFaults(manifest, []).join(" ")).toMatch(/is listed as read and is not here/);
  });

  it("refuses a signature with no name or no date", () => {
    const nameless = {
      schema: "kettle/study-read@0",
      read: [{ file: "p01.json", by: "  ", at: "2026-09-01" }],
    };
    expect(manifestFaults(nameless, ["p01.json"]).join(" ")).toMatch(/signed by nobody/);

    const undated = {
      schema: "kettle/study-read@0",
      read: [{ file: "p01.json", by: "Rich Holman", at: "soon" }],
    };
    expect(manifestFaults(undated, ["p01.json"]).join(" ")).toMatch(/was read on/);
  });
});

describe("what is actually committed", () => {
  it("is a directory with a manifest, whether or not anybody has been recruited", () => {
    // The empty state is real and will hold until recruitment. What
    // must not be possible is for it to *become* the populated state
    // without the manifest noticing, so the manifest is committed now,
    // empty, rather than written on the day the files arrive.
    expect(existsSync(transcriptsDir)).toBe(true);
    expect(existsSync(join(transcriptsDir, "READ.json"))).toBe(true);
  });

  it("holds only transcripts that pass the floor and that somebody has read", () => {
    const files = readdirSync(transcriptsDir)
      .filter((name) => TRANSCRIPT.test(name))
      .sort();
    const manifest = JSON.parse(readFileSync(join(transcriptsDir, "READ.json"), "utf8"));

    expect(manifestFaults(manifest, files)).toEqual([]);
    for (const file of files) {
      const parsed = JSON.parse(readFileSync(join(transcriptsDir, file), "utf8"));
      expect(faults(parsed), file).toEqual([]);
    }

    // Twenty is the pre-registered n. Over it means somebody added a
    // file the design did not plan for, which is worth a stop.
    expect(files.length).toBeLessThanOrEqual(20);
  });

  it("scores every committed transcript, and calls no catch on a natural error a false alarm", () => {
    // #577, held on the file that produced it: `a01`'s task 4 drew a
    // letter as a clean control that the pipeline had already dropped
    // an obligation from, and the answer named it in full.
    //
    // This also holds the corpus and the transcripts together. A
    // transcript rebuilds from its participant id against the corpus it
    // names, so regenerating the letters under a committed transcript
    // breaks it — loudly, here, rather than silently in an analysis.
    const unclean = uncleanLetters();

    const files = readdirSync(transcriptsDir)
      .filter((name) => TRANSCRIPT.test(name))
      .sort();
    for (const file of files) {
      const transcript = JSON.parse(
        readFileSync(join(transcriptsDir, file), "utf8"),
      ) as Transcript;
      if (transcript.material !== "letters") continue;
      const session = planLetters(transcript.participant, letterCorpus().letters, unclean);
      const table = scores(transcript, letterCorpus());
      expect(table.length, file).toBe(transcript.responses.filter((r) => r !== null).length);
      for (const [index, row] of table.entries()) {
        if (row.outcome !== "false-alarm") continue;
        expect(unclean.has(session.tasks[index]!.document), `${file} task ${index + 1}`).toBe(
          false,
        );
      }
    }
  });
});
