// #431, letter track: scoring one answer.

import { describe, expect, it } from "vitest";
import { datesIn, daysIn, scoreLetter } from "./study-letter-response";
import type { LetterTask } from "./study-letter-session";
import { ABSENT, type Response } from "./study-response";

function task(truth: LetterTask["truth"]): LetterTask {
  return {
    index: 0,
    document: "letter-01",
    class: truth === null ? "clean" : truth.operator === "dropped-obligation" ? "omission" : truth.operator === "misquoted-deadline" ? "invention" : "mis-relation",
    letter: {} as LetterTask["letter"],
    truth,
    emphasis: null,
  };
}

function answer(over: Partial<Response>): Response {
  return {
    verdict: "accept",
    flagged: [],
    offered: [],
    correction: null,
    confidence_before: 3,
    confidence_after: 3,
    opened: [],
    elapsed_ms: 1000,
    ...over,
  };
}

describe("scoring a letter answer", () => {
  it("reads dates however a person writes them", () => {
    expect(datesIn("it should be 23 May 2026, not the 30th")).toEqual(["2026-05-23"]);
    expect(datesIn("2026-05-23 or 23/05/2026")).toEqual(["2026-05-23"]);
    expect(daysIn("the letter says within 14 days")).toEqual([14]);
  });

  it("judges a mis-resolution on the one date offered", () => {
    const t = task({ operator: "misresolved-deadline", target: "Pay the total", gold: "2026-05-23", shown: "2026-05-30" });
    const detected = scoreLetter(t, answer({ verdict: "reject", flagged: ["Pay the total"], correction: "23 May 2026", opened: ["Pay the total"] }));
    expect(detected.outcome).toBe("correct-detection");
    expect(detected.corrected).toBe("right");
    expect(detected.opened_target).toBe(true);
    expect(scoreLetter(t, answer({ verdict: "reject", flagged: ["Pay the total"], correction: "not 30 May" })).corrected).toBe("needs-judging");
    expect(scoreLetter(t, answer({ verdict: "reject", flagged: ["Pay the total"], correction: "1 June 2026" })).corrected).toBe("wrong");
  });

  it("accepts the true days or the true date for a misquote", () => {
    const t = task({
      operator: "misquoted-deadline",
      target: "Pay the total",
      gold: { phrase: "within 14 days", due: "2026-05-23" },
      shown: { phrase: "within 28 days", due: "2026-06-06" },
    });
    expect(scoreLetter(t, answer({ verdict: "reject", flagged: ["pay the total"], correction: "14 days" })).corrected).toBe("right");
    expect(scoreLetter(t, answer({ verdict: "reject", flagged: ["Pay the total"], correction: "23 May 2026" })).corrected).toBe("right");
    expect(scoreLetter(t, answer({ verdict: "reject", flagged: ["Pay the total"], correction: "" })).corrected).toBe("not-offered");
  });

  it("scores every claim a participant ticked, not just the first", () => {
    // The box asked "which one?" and the reports do not always have one
    // answer. Three of the author's thirty answers smuggled a second
    // complaint into the correction field because there was nowhere
    // else to put it — "Wrong date: 6 May 2026. And missing payment
    // ...". A participant who sees the seed *and* something else had to
    // choose, and choosing the other one scored a false alarm on
    // somebody who had seen the seed.
    //
    // So the answer is a set. Which needs a scoring rule, or ticking
    // everything would score a perfect detection: each ticked claim
    // that carries no seed is a false alarm of its own, against the
    // claims the report actually offered.
    const seeded = task({
      operator: "misresolved-deadline",
      target: "Pay the total",
      gold: "2026-05-23",
      shown: "2026-05-30",
    });
    const offered = ["Pay the total", "Return the form", "Quote the reference"];

    const both = scoreLetter(
      seeded,
      answer({ verdict: "reject", flagged: ["Pay the total", "Return the form"], offered }),
    );
    expect(both.outcome).toBe("correct-detection");
    expect(both.false_alarms).toBe(1);
    expect(both.claims_offered).toBe(3);

    // Ticking everything detects the seed and says nothing: a hit rate
    // of one alongside a false-alarm rate of one is visibly
    // uninformative, where "correct-detection" alone would have looked
    // like a perfect score.
    const everything = scoreLetter(seeded, answer({ verdict: "reject", flagged: offered, offered }));
    expect(everything.outcome).toBe("correct-detection");
    expect(everything.false_alarms).toBe(2);

    // Ticking nothing is doubt they could not place, as before.
    expect(scoreLetter(seeded, answer({ verdict: "reject", flagged: [], offered })).outcome).toBe(
      "unattributed-doubt",
    );

    // And on a clean report every tick is a false alarm.
    const clean = scoreLetter(
      task(null),
      answer({ verdict: "reject", flagged: ["Pay the total"], offered }),
    );
    expect(clean.outcome).toBe("false-alarm");
    expect(clean.false_alarms).toBe(1);
  });

  it("does not call a catch on a natural pipeline error a false alarm", () => {
    // #577. `a01`'s task 4 drew a letter as a clean control that was
    // not clean — the pipeline had dropped the appointment itself —
    // and the answer named the missing obligation in full. It scored
    // `false-alarm`, which is the one direction that matters: it makes
    // the most careful participant look like the noisiest, and the
    // false-alarm rate is half the instrument.
    //
    // The audit already knows. It is a person's signed read, so the
    // scorer reads it rather than deciding for itself.
    const control = task(null);
    const rejected = answer({ verdict: "reject", flagged: [ABSENT] });
    expect(scoreLetter(control, rejected).outcome).toBe("false-alarm");
    expect(scoreLetter(control, rejected, { unclean: true }).outcome).toBe(
      "caught-a-natural-error",
    );

    // Only where the alternative was a false alarm. A detection of the
    // seed is still a detection, and an acceptance is still an
    // acceptance, whatever else the document got wrong.
    const seeded = task({
      operator: "misresolved-deadline",
      target: "Pay the total",
      gold: "2026-05-23",
      shown: "2026-05-30",
    });
    expect(
      scoreLetter(seeded, answer({ verdict: "reject", flagged: ["Pay the total"] }), {
        unclean: true,
      }).outcome,
    ).toBe("correct-detection");
    expect(scoreLetter(seeded, answer({}), { unclean: true }).outcome).toBe("false-acceptance");
  });

  it("attributes an absence to the absence, and a card to the card", () => {
    // The rule that had every rejection scoring a false alarm. A
    // dropped obligation is not on the page, so pointing at a card is
    // wrong on it and `ABSENT` is right; on an operator that moved a
    // claim rather than removing one, it is the other way round.
    const dropped = task({
      operator: "dropped-obligation",
      target: "Return the form",
      gold: { phrase: "within 14 days", due: "2026-05-23" },
      shown: null,
    });
    expect(scoreLetter(dropped, answer({ verdict: "reject", flagged: [ABSENT] })).outcome).toBe(
      "correct-detection",
    );
    expect(
      scoreLetter(dropped, answer({ verdict: "reject", flagged: [ABSENT], correction: "23 May 2026" }))
        .corrected,
    ).toBe("right");
    expect(
      scoreLetter(dropped, answer({ verdict: "reject", flagged: ["Return the form"] })).outcome,
    ).toBe("false-alarm");

    const moved = task({
      operator: "misresolved-deadline",
      target: "Pay the total",
      gold: "2026-05-23",
      shown: "2026-05-30",
    });
    expect(scoreLetter(moved, answer({ verdict: "reject", flagged: [ABSENT] })).outcome).toBe(
      "false-alarm",
    );
    expect(
      scoreLetter(task(null), answer({ verdict: "reject", flagged: [ABSENT] })).outcome,
    ).toBe("false-alarm");
  });

  it("scores the study's outcomes the same way as the statement track", () => {
    const seeded = task({ operator: "dropped-obligation", target: "Return the form", gold: { phrase: "by 30 June 2026", due: "2026-06-30" }, shown: null });
    expect(scoreLetter(seeded, answer({})).outcome).toBe("false-acceptance");
    expect(scoreLetter(seeded, answer({ verdict: "reject" })).outcome).toBe("unattributed-doubt");
    expect(scoreLetter(seeded, answer({ verdict: "reject", flagged: ["Pay the total"] })).outcome).toBe("false-alarm");
    expect(scoreLetter(seeded, answer({ verdict: "reject", flagged: ["Return the form"] })).opened_target).toBeNull();
    const clean = task(null);
    expect(scoreLetter(clean, answer({})).outcome).toBe("correct-acceptance");
    expect(scoreLetter(clean, answer({ verdict: "reject", flagged: ["anything"] })).outcome).toBe("false-alarm");
    expect(scoreLetter(clean, answer({ confidence_before: 2, confidence_after: 5 })).confidence_shift).toBe(3);
  });
});
