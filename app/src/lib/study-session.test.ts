// #431: what one participant sees, decided before they arrive.
//
// Randomisation is a pre-registration commitment, not an implementation
// detail: the protocol on the issue promises that task order and which
// reports carry a seed are randomised, and that the analysis can say
// afterwards exactly what each participant was shown. Both halves have
// to hold at once, so the plan is *deterministic given the participant*
// — drawn from a seeded generator rather than `Math.random`, which
// would leave the record unable to reconstruct a session that has
// already happened.
//
// The mix is the one frozen on #431 after #568's rung 1: three
// inventions, three mis-relations, two omissions, two clean. The clean
// pair is not padding — it gives the false-alarm rate and stops
// "reject everything" from scoring well.

import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { plan } from "./study-session";
import type { RunReport } from "./types";

const repoRoot = join(__dirname, "../../..");

function clean(): RunReport {
  return JSON.parse(
    readFileSync(join(repoRoot, "fixtures/run-01/results.json"), "utf8"),
  ) as RunReport;
}

/** Integer pence, because money is never floated — including in a test
 * whose whole job is to rank amounts. */
function pence(amount: string): number {
  const [pounds = "0", p = "00"] = amount.split(".");
  return Number(pounds) * 100 + Number(p);
}

/** The committed study corpus: the ten reports the study actually
 * shows. Used where a test needs real figures rather than ten clones of
 * one report. */
function studyCorpus(): RunReport[] {
  return Array.from({ length: 10 }, (_, i) => {
    const name = `report-${String(i + 1).padStart(2, "0")}.json`;
    return JSON.parse(
      readFileSync(join(repoRoot, "fixtures/study", name), "utf8"),
    ) as RunReport;
  });
}

/** A corpus of distinguishable reports, one per task in a session. */
function corpus(size = 10): RunReport[] {
  return Array.from({ length: size }, (_, i) => {
    const report = clean();
    report.run.id = `study-${String(i).padStart(2, "0")}`;
    return report;
  });
}

describe("study session", () => {
  it("a_session_is_ten_tasks_in_the_declared_mix_and_the_same_participant_sees_it_again", () => {
    const first = plan("p01", corpus());

    // The mix frozen on #431: the primary comparison needs three of
    // each displayed class, and a null on either arm has to mean
    // "not detected" rather than "not shown".
    expect(first.tasks).toHaveLength(10);
    const classes = first.tasks.map((task) => task.class).sort();
    expect(classes).toEqual([
      "clean",
      "clean",
      "invention",
      "invention",
      "invention",
      "mis-relation",
      "mis-relation",
      "mis-relation",
      "omission",
      "omission",
    ]);

    // Reproducible: the analysis reconstructs a finished session from
    // the participant id alone. A plan nobody can rebuild is a plan
    // whose randomisation cannot be checked.
    const again = plan("p01", corpus());
    expect(again.tasks.map((t) => `${t.document}:${t.class}`)).toEqual(
      first.tasks.map((t) => `${t.document}:${t.class}`),
    );

    // Every seeded task carries the gold answer its response is scored
    // against, and a clean task carries none — the two must be
    // distinguishable in the record, never inferred from the report.
    for (const task of first.tasks) {
      if (task.class === "clean") expect(task.truth).toBeNull();
      else expect(task.truth).not.toBeNull();
    }
  });

  it("randomises order and assignment across participants", () => {
    const one = plan("p01", corpus());
    const two = plan("p02", corpus());

    const shape = (p: typeof one) => p.tasks.map((t) => `${t.document}:${t.class}`).join("|");
    expect(shape(one)).not.toEqual(shape(two));
  });

  it("gives every task its own report so a participant cannot learn one document", () => {
    const session = plan("p01", corpus());
    const documents = session.tasks.map((task) => task.document);

    expect(new Set(documents).size).toBe(10);
  });

  it("refuses a corpus too small to give each task its own report", () => {
    // Ten seeds spread over five reports would show the same five
    // merchants ten times, and by the fourth report the participant is
    // studying the harness rather than reading a report.
    expect(() => plan("p01", corpus(6))).toThrow(/6 reports/);
  });

  it("never repeats the same error on the same merchant inside one session", () => {
    // Found by looking at a plan rather than by a test: p01 drew
    // `wrong-rise-month` on Netflix twice, and p03 drew `wrong-period`
    // on Spotify twice. Every assertion in this file passed while it
    // happened, because none of them looked at the pairs.
    //
    // It matters for the same reason the mix does. A participant who
    // has just caught Netflix's rise dated to the wrong month does not
    // read the second one — they recognise it, and a recognition scores
    // as a detection it is not.
    for (const participant of ["p01", "p02", "p03", "p04", "p05"]) {
      const seeded = plan(participant, corpus())
        .tasks.filter((task) => task.truth !== null)
        .map((task) => `${task.truth?.operator}:${task.truth?.target}`);

      expect(new Set(seeded).size, `${participant} repeats a seed`).toBe(
        seeded.length,
      );
    }
  });

  it("every_participant_is_in_condition_three_so_the_primary_gets_the_pairs_it_was_sized_for", () => {
    // Decided 26 August 2026, on an inconsistency in #431's own frozen
    // pre-registration: it promises the primary 60 pairs — invention
    // against mis-relation, paired, within participant, in condition 3
    // — and separately splits twenty participants across two arms at
    // ten each. Both cannot hold. Ten participants in condition 3 give
    // thirty pairs, and the minimum detectable difference goes from
    // thirty points to about forty, which is past the gap the study
    // exists to act on.
    //
    // So condition 4 is not run here. It was already declared
    // exploratory, predicted to show no movement, and admitted to
    // seeing only a ~40-point difference; spending half the sample on
    // it to buy that costs the primary the power it was designed
    // around. It becomes its own study if the primary result makes it
    // worth one.
    for (let n = 1; n <= 20; n += 1) {
      const participant = `p${String(n).padStart(2, "0")}`;
      expect(plan(participant, studyCorpus()).arm, participant).toBe("evidence");
    }
  });

  it("emphasis_is_drawn_rather_than_ranked_so_there_is_no_visible_rule_to_violate", () => {
    // The first rule here ranked by cost — the largest annualised
    // figure — on the clean report, so the seed could not *choose* the
    // emphasis. That was not enough. The seed still changes which row
    // *looks* biggest: an invention multiplies its row by ten, so on
    // those tasks the emphasised row is no longer the largest figure on
    // the page while on the other seven it is. Over ten reports that is
    // learnable, and it marks exactly one of the two classes the
    // primary compares.
    //
    // Drawing instead of ranking removes the rule rather than muffling
    // it. Nothing about the emphasis can be predicted from the page, so
    // there is no invariant for a seed to break. The cost is that
    // "high-risk" goes: the arm now asks whether one already-open
    // disclosure changes what a person does, which is the question ten
    // per arm could answer anyway.
    //
    // Asserted as: no magnitude rule predicts it. Within one class,
    // over enough participants, the emphasis is sometimes the largest
    // row shown and sometimes not — which "always the biggest" and
    // "never the biggest" both fail.
    const clean = studyCorpus();
    const seen: Record<string, Set<boolean>> = {};
    for (let n = 1; n <= 12; n += 1) {
      for (const task of plan(`p${String(n).padStart(2, "0")}`, clean).tasks) {
        const biggest = [...task.report.recurring].sort(
          (a, b) => pence(b.annualised) - pence(a.annualised),
        )[0]?.merchant;
        (seen[task.class] ??= new Set()).add(task.emphasis === biggest);
      }
    }
    for (const taskClass of ["invention", "mis-relation", "clean"]) {
      expect(
        [...(seen[taskClass] ?? [])].sort(),
        `${taskClass}: emphasis follows the figures`,
      ).toEqual([false, true]);
    }
  });

  it("emphasises a claim the report still shows, and the same one on a rebuild", () => {
    // Reproducibility is the whole reason the draw comes from the
    // seeded generator: the analysis has to be able to say what any
    // participant was pointed at without having been in the room.
    const first = plan("p07", studyCorpus());
    const again = plan("p07", studyCorpus());
    expect(again.tasks.map((t) => t.emphasis)).toEqual(
      first.tasks.map((t) => t.emphasis),
    );
    for (const task of first.tasks) {
      expect(
        task.report.recurring.map((f) => f.merchant),
        `task ${task.index}`,
      ).toContain(task.emphasis);
    }
  });

  it("always emphasises something, so an unemphasised report cannot signal an omission", () => {
    // The one place the seeded report has to be consulted: a dropped
    // claim cannot be pointed at. Falling through to the next claim
    // down keeps every report in the emphasised arm looking the same,
    // and a participant who noticed that "nothing is emphasised" only
    // happens on omissions would be detecting the harness.
    for (const participant of ["p01", "p02", "p03", "p04", "p05", "p06"]) {
      for (const task of plan(participant, studyCorpus()).tasks) {
        expect(task.emphasis, `${participant} task ${task.index}`).toBeTypeOf(
          "string",
        );
      }
    }
  });

  it("seeds a mis-relation with both of its operators, never one three times", () => {
    const session = plan("p01", corpus());
    const operators = session.tasks
      .filter((task) => task.class === "mis-relation")
      .map((task) => task.truth?.operator);

    expect(new Set(operators).size).toBeGreaterThan(1);
  });
});
