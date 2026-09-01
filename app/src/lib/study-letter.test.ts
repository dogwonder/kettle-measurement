// #431, letter track: one named error changes only its target action
// and keeps the gold answer — the same first example the statement
// track was built from, over a letter's proposed actions.

import { describe, expect, it } from "vitest";
import {
  eligible,
  longDate,
  misquote,
  seedLetter,
  shifted,
  SHIFT_DAYS,
  type StudyLetter,
} from "./study-letter";
import type { ProposedAction } from "./types";

function action(title: string, phrase: string, due: string | null, party = "Kestrel Plumbing & Heating Ltd"): ProposedAction {
  const detail =
    due === null
      ? `${party} asked for this, but the letter does not give a date Kettle could work out — it says "${phrase}". Choose a date that suits you.`
      : `${party} asked for this by ${longDate(due)}. The letter says "${phrase}".`;
  return {
    id: "act-00",
    kind: "calendar_reminder",
    title,
    detail,
    evidence: {
      asked_by: party,
      in_the_letter: phrase,
      passage_1: `Please ${title.toLowerCase()} ${phrase}.`,
    },
    disputed: [],
    export: {
      ...(due === null ? {} : { ics: { summary: `${title} — ${party}`, date: due } }),
      text: `${title} (${party}) — ${phrase}`,
    },
    status: "proposed",
  };
}

/** A letter with a relative ask, an absolute ask and an undated one. */
function letter(): StudyLetter {
  const actions = [
    action("Pay the total", "within 14 days", "2026-05-23"),
    action("Return the form", "by 30 June 2026", "2026-06-30"),
    action("Call to rearrange", "as soon as possible", null),
  ].map((each, index) => ({ ...each, id: `act-${String(index + 1).padStart(2, "0")}` }));
  return {
    schema: "kettle/study-letter@0",
    id: "letter-t1",
    source: {
      file: "invoice-t1.txt",
      hash: "blake3:test",
      text: [
        "Kestrel Plumbing & Heating Ltd",
        "Please pay the total within 14 days of the date of this letter.",
        "Please return the form by 30 June 2026.",
        "Please call to rearrange as soon as possible.",
      ].join("\n\n"),
    },
    pack: { id: "app.kttl.letter-to-actions", version: "0.2.0" },
    model: "test",
    actions: { schema: "kettle/proposed-actions@0", run_id: "letter-t1", note: "", actions },
    expected: [],
  };
}

describe("seeding a letter", () => {
  it("one_named_mutation_changes_only_its_target_action_and_keeps_the_gold_answer", () => {
    const clean = letter();
    const { letter: shown, truth } = seedLetter(clean, {
      operator: "misquoted-deadline",
      target: "Pay the total",
    });
    expect(truth).toEqual({
      operator: "misquoted-deadline",
      target: "Pay the total",
      gold: { phrase: "within 14 days", due: "2026-05-23" },
      shown: { phrase: "within 28 days", due: shifted("2026-05-23", 14) },
    });
    const target = shown.actions.actions.find((a) => a.title === "Pay the total")!;
    // Every field that carries the deadline moved together.
    expect(target.evidence.in_the_letter).toBe("within 28 days");
    expect(target.detail).toContain('"within 28 days"');
    expect(target.detail).toContain(longDate("2026-06-06"));
    expect(target.export.ics?.date).toBe("2026-06-06");
    expect(target.export.text).toContain("within 28 days");
    // The passage is untouched: it is what refutes the invention.
    expect(target.evidence.passage_1).toBe(clean.actions.actions[0]!.evidence.passage_1);
    // Nothing else moved.
    expect(shown.actions.actions.filter((a) => a.title !== "Pay the total")).toEqual(
      clean.actions.actions.filter((a) => a.title !== "Pay the total"),
    );
    expect(shown.source).toEqual(clean.source);
    // The false quote is not in the letter — or it would be true.
    expect(shown.source.text.toLowerCase()).not.toContain("within 28 days");
  });

  it("a mis-resolution keeps every word and moves only the date worked out", () => {
    const clean = letter();
    const { letter: shown, truth } = seedLetter(clean, {
      operator: "misresolved-deadline",
      target: "Pay the total",
    });
    expect(truth).toEqual({
      operator: "misresolved-deadline",
      target: "Pay the total",
      gold: "2026-05-23",
      shown: "2026-05-30",
    });
    const target = shown.actions.actions[0]!;
    expect(target.evidence.in_the_letter).toBe("within 14 days");
    expect(target.detail).toContain(`by ${longDate("2026-05-30")}`);
    expect(target.export.ics?.date).toBe("2026-05-30");
  });

  it("an omission removes the ask and renumbers, so no gap points at it", () => {
    const { letter: shown, truth } = seedLetter(letter(), {
      operator: "dropped-obligation",
      target: "Return the form",
    });
    expect(truth.shown).toBeNull();
    expect(truth.gold).toEqual({ phrase: "by 30 June 2026", due: "2026-06-30" });
    expect(shown.actions.actions.map((a) => a.title)).toEqual(["Pay the total", "Call to rearrange"]);
    expect(shown.actions.actions.map((a) => a.id)).toEqual(["act-01", "act-02"]);
  });

  it("only offers an operator where the seed would be an honest one", () => {
    const clean = letter();
    // A written date can be misquoted; it cannot be mis-resolved,
    // because it was read rather than worked out.
    expect(eligible(clean, "misquoted-deadline")).toEqual(["Pay the total", "Return the form"]);
    expect(eligible(clean, "misresolved-deadline")).toEqual(["Pay the total"]);
    expect(eligible(clean, "dropped-obligation")).toHaveLength(3);
    // "as soon as possible" carries no value to misstate.
    expect(misquote("as soon as possible", clean.source.text)).toBeNull();
    // A misquote that the letter happens to say elsewhere is not false.
    expect(misquote("within 14 days", "pay within 14 days or within 28 days")).toBeNull();
  });

  it("refuses a target it cannot name exactly once", () => {
    expect(() => seedLetter(letter(), { operator: "dropped-obligation", target: "Nothing" })).toThrow(
      /0 actions/,
    );
  });

  it("moves dates in the calendar, not in the string", () => {
    expect(shifted("2026-12-28", SHIFT_DAYS)).toBe("2027-01-04");
    expect(shifted("2026-02-25", 7)).toBe("2026-03-04");
    expect(longDate("2026-04-05")).toBe("5 April 2026");
  });
});
