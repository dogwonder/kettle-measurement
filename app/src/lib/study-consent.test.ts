// #431: the words a participant agreed to, and the four promises they make.
//
// A consent form is a document somebody will be asked about later:
// "what did they agree to?" The harness already refused to open a
// session without a version, which answered "which text" and not "what
// did it say" — the constant and the words on the screen were two
// unrelated things, and editing the copy moved neither.
//
// So the text is data, the stamp is derived from it, and each promise
// the text makes is held by something here rather than by intent.

import { describe, expect, it } from "vitest";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { CONSENT, digestOf, stamp, unsettledIn } from "./study-consent";
import { begin, record } from "./study-record";
import type { Response } from "./study-response";
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

describe("the consent text in force", () => {
  it("is stamped by its words, so a transcript names what was on the screen", () => {
    const given = stamp(CONSENT, "2026-09-01T10:00:00Z");
    expect(given.version).toBe(CONSENT.version);
    expect(given.digest).toBe(digestOf(CONSENT));
    expect(given.given_at).toBe("2026-09-01T10:00:00Z");

    // One word, anywhere in the document, and the stamp is a different
    // stamp. Two participants a fortnight apart cannot come out of the
    // analysis looking like they read the same page when they did not.
    const edited = {
      ...CONSENT,
      sections: CONSENT.sections.map((section, index) =>
        index === 0
          ? { ...section, paragraphs: [...section.paragraphs, "And one more thing."] }
          : section,
      ),
    };
    expect(digestOf(edited)).not.toBe(digestOf(CONSENT));
  });

  it("is pinned, so the words cannot move without the version moving", () => {
    // This is the guard, and it is meant to fail. If you edited the
    // text, that is the version bumping: change the date to the day it
    // came into force and paste the new digest here, in the same
    // commit. A version that names one document and describes another
    // is worse than no version.
    expect(CONSENT.version).toBe("2026-08-28");
    expect(digestOf(CONSENT)).toBe("bb057ccb");
  });

  it("is finished, so a session can be opened over it", () => {
    // The ratchet, and the reason it is a separate example from the one
    // below: while a gap was declared this failed, which is what
    // "not ready to recruit" looked like from the test suite. Now that
    // both gaps are filled, adding a new one fails here until it is
    // filled too — declaring it is no longer enough, because people are
    // being asked to agree to this.
    expect([...unsettledIn(CONSENT)]).toEqual([]);
    expect(CONSENT.unsettled).toEqual([]);
  });

  it("declares what nobody has settled instead of leaving a blank", () => {
    // Empty today, and the example is still the one that matters. Two
    // facts were open until 27 August — who held the files afterwards,
    // and who a participant asked about them — and the next question
    // about how these files are handled arrives the same way. A form
    // with a silent gap is a form somebody hands over anyway, so each
    // gap is a marker in the text and an entry beside it with a reason
    // and a date, the staged-exception habit this repo already keeps
    // for a govuk component or a tools mixin. Neither half may exist
    // without the other.
    const marked = unsettledIn(CONSENT);
    const declared = CONSENT.unsettled.map((gap) => gap.field);
    expect([...marked].sort()).toEqual([...declared].sort());
    for (const gap of CONSENT.unsettled) {
      expect(gap.why.length).toBeGreaterThan(0);
      expect(gap.since).toMatch(/^\d{4}-\d{2}-\d{2}$/);
    }
  });
});

describe("each promise the text makes", () => {
  it("records what the 'what gets written down' section says and nothing else", () => {
    // The section is a list of fields. The transcript is a list of
    // fields. A form that promised one set while the file held another
    // would be the same defect as a report whose disclosure did not
    // match its claim, and this project checks that one on the
    // artefact rather than in prose.
    const started = begin("p07", corpus(), stamp(CONSENT, "2026-09-01T10:00:00Z"));
    const answer: Response = {
      verdict: "accept",
      flagged: [],
      offered: [],
      correction: null,
      confidence_before: 3,
      confidence_after: 4,
      opened: [],
      elapsed_ms: 45_000,
    };
    const filled = record(started, 0, answer);
    const written = JSON.parse(JSON.stringify(filled)) as Record<string, unknown>;

    expect(Object.keys(written).sort()).toEqual(
      CONSENT.records.map((field) => field.key).sort(),
    );
  });

  it("cannot send anything anywhere, because nothing in the harness knows how", () => {
    // "Nothing is uploaded" is the promise the whole harness rests on
    // and the one a participant cannot check. The product's boundary is
    // guarded by a build-time source scan (`crates/privacy-audit`); the
    // study gets the same treatment at its own scale, over the harness
    // and every module it imports.
    const paths = [
      ...sources(join(repoRoot, "app/study")),
      ...sources(join(repoRoot, "app/src/lib")),
      ...sources(join(repoRoot, "app/src/lib/components")),
    ];
    expect(paths.length).toBeGreaterThan(20);

    const network = /\bfetch\s*\(|XMLHttpRequest|sendBeacon|new WebSocket|new EventSource/;
    const offenders = paths.filter((path) =>
      network.test(readFileSync(path, "utf8")),
    );
    expect(offenders).toEqual([]);
  });
});

/** Every source file in one directory, tests excluded — they say what
 * must not happen and would match their own pattern. */
function sources(dir: string): string[] {
  return readdirSync(dir, { withFileTypes: true })
    .filter((entry) => entry.isFile())
    .map((entry) => entry.name)
    .filter((name) => /\.(ts|svelte)$/.test(name) && !name.includes(".test."))
    .map((name) => join(dir, name));
}
