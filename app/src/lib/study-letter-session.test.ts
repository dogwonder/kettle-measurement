// #431, letter track: a session is the frozen mix, drawn from the
// participant's own id, and rebuilds from it.

import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { planLetters } from "./study-letter-session";
import type { StudyLetter } from "./study-letter";

const dir = join(import.meta.dirname, "../../../fixtures/study/letters");

function corpus(): StudyLetter[] {
  return readdirSync(dir)
    .filter((name) => /^letter-\d{2}\.json$/.test(name))
    .sort()
    .map((name) => JSON.parse(readFileSync(join(dir, name), "utf8")) as StudyLetter);
}

/** The letters a person's read of the audit records as unclean. */
function unclean(): ReadonlySet<string> {
  const audit = JSON.parse(
    readFileSync(join(dir, "../audit-letters.json"), "utf8"),
  ) as { letters: Record<string, { clean: boolean | null }> };
  return new Set(
    Object.entries(audit.letters)
      .filter(([, entry]) => entry.clean === false)
      .map(([id]) => id),
  );
}

describe("a letter session", () => {
  it("draws its clean controls only from letters the audit calls clean", () => {
    // #577's other half. The two clean tasks are what the false-alarm
    // rate is measured on, and a control the pipeline already got
    // wrong cannot measure it: the participant is right to reject it,
    // and the cell is contaminated at source.
    //
    // Scoring can now say so after the fact, which is worth having —
    // but a control that was never contaminated is worth more, and it
    // costs nothing here, because the corpus holds more clean letters
    // than a session needs.
    const dirty = unclean();
    expect(dirty.size, "the audit records some natural errors, or this proves nothing").toBeGreaterThan(0);

    for (const participant of ["a01", "a02", "p01", "p07", "p20"]) {
      const session = planLetters(participant, corpus(), dirty);
      const controls = session.tasks.filter((task) => task.class === "clean");
      expect(controls).toHaveLength(2);
      for (const control of controls) {
        expect(dirty.has(control.document), `${participant}: ${control.document}`).toBe(false);
      }
    }
  });

  it("is the frozen mix over letters the participant has not seen twice", () => {
    const session = planLetters("a01", corpus());
    const classes = session.tasks.map((task) => task.class).sort();
    expect(classes).toEqual([
      "clean", "clean",
      "invention", "invention", "invention",
      "mis-relation", "mis-relation", "mis-relation",
      "omission", "omission",
    ]);
    expect(new Set(session.tasks.map((task) => task.document)).size).toBe(10);
    expect(session.enrolment).toBe(1);
  });

  it("rebuilds from the id alone, and differs between ids", () => {
    const one = planLetters("a01", corpus());
    const again = planLetters("a01", corpus());
    expect(again).toEqual(one);
    const other = planLetters("a02", corpus());
    expect(other.tasks.map((t) => `${t.class}:${t.document}`)).not.toEqual(
      one.tasks.map((t) => `${t.class}:${t.document}`),
    );
  });

  it("seeds what it says it seeds", () => {
    for (const participant of ["a01", "a02", "a03", "p01", "p02"]) {
      for (const task of planLetters(participant, corpus()).tasks) {
        if (task.class === "clean") {
          expect(task.truth).toBeNull();
          continue;
        }
        expect(task.truth?.operator).toBe(
          { invention: "misquoted-deadline", "mis-relation": "misresolved-deadline", omission: "dropped-obligation" }[task.class],
        );
        const titles = task.letter.actions.actions.map((a) => a.title);
        if (task.class === "omission") expect(titles).not.toContain(task.truth?.target);
        else expect(titles).toContain(task.truth?.target);
      }
    }
  });

  it("refuses a corpus smaller than a session", () => {
    expect(() => planLetters("a01", corpus().slice(0, 9))).toThrow(/needs 10 letters/);
  });
});
