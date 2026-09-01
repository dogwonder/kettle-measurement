// #431: the study harness's first contract.
//
// The study asks whether a person can catch an error the pipeline did
// not. That needs three things at once, and this file holds them to it:
// a clean report, the same report with exactly one named error seeded,
// and machine-readable truth about what changed — because a
// participant's answer can only be scored against a gold answer nobody
// derived after the fact.
//
// The primary seeded error is an **omission**, and that is not one of
// six equals. #432's reading across the whole v16 archive (#565) found
// `prevented` = 0 at every rung with a flat ladder: decomposing into
// closed questions nearly eliminates invention, and the harm that
// remains is the thing the report never mentions. All seven wrong
// answers on the letter pack's sealed set were misses.
//
// So the harness must seed a *removal* as fluently as a corruption, and
// the "only its target changed" invariant has to hold for both. A
// mutation framework that can only corrupt what is displayed would
// quietly restrict this study to the error class that barely occurs —
// which is the failure mode the study exists to look for in the
// product.
//
// run-01 is the subscription pack rather than the letter pack, because
// it is the committed rendered report the app's tests already share.
// A dropped recurring payment is the same harm in that pack's language:
// something true about your money that the report does not mention.

import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { seed, type SeededError } from "./study-fixtures";
import type { RunReport } from "./types";

const repoRoot = join(__dirname, "../../..");

function clean(): RunReport {
  return JSON.parse(
    readFileSync(join(repoRoot, "fixtures/run-01/results.json"), "utf8"),
  ) as RunReport;
}

/** Every field of every finding, as one comparable list of strings. */
function claimLines(report: RunReport): string[] {
  return (report.recurring ?? []).map((finding) => JSON.stringify(finding));
}

describe("study fixtures", () => {
  it("one_named_mutation_changes_only_its_target_claim_and_keeps_the_gold_answer", () => {
    const before = clean();
    const error: SeededError = { operator: "wrong-amount", target: "Netflix" };

    const seeded = seed(before, error);

    // 1. The gold answer survives the seeding. Without this the study
    //    has nothing to score a participant's correction against, and
    //    re-deriving it from the clean file afterwards would let a
    //    harness bug and a participant error look identical.
    expect(seeded.truth.operator).toBe("wrong-amount");
    expect(seeded.truth.target).toBe("Netflix");
    expect(seeded.truth.gold).toBe("12.99");
    expect(seeded.truth.shown).toBe("129.90");

    // 2. Exactly one claim differs, and it is the named one.
    const cleanLines = claimLines(before);
    const changed = claimLines(seeded.report).filter(
      (line, index) => line !== cleanLines[index],
    );
    expect(changed).toHaveLength(1);
    expect(JSON.parse(changed[0] ?? "{}").merchant).toBe("Netflix");

    // 3. The clean report is not mutated in place. A study that seeded
    //    its own control would show every participant the faulty
    //    version and record the comparison as if it had happened.
    expect(before.recurring[0]?.amount_current).toBe("12.99");
  });

  it("an omission removes its claim and leaves every other claim untouched", () => {
    const before = clean();
    const seeded = seed(before, { operator: "dropped-claim", target: "Netflix" });

    expect(seeded.report.recurring).toHaveLength(before.recurring.length - 1);
    expect(
      seeded.report.recurring.some((finding) => finding.merchant === "Netflix"),
    ).toBe(false);
    // The survivors are byte-identical: a drop must not renumber or
    // re-round anything, or a participant could "detect" the seeded
    // error by noticing an artefact of the seeding.
    const survivors = claimLines(seeded.report);
    const expected = claimLines(before).filter(
      (line) => JSON.parse(line).merchant !== "Netflix",
    );
    expect(survivors).toEqual(expected);
  });

  it("an omission's gold answer is the claim that should have been there", () => {
    const before = clean();
    const seeded = seed(before, { operator: "dropped-claim", target: "Netflix" });

    // `shown` is null and that is the whole point: there is no row on
    // the page, no evidence to open and no claim to distrust. The gold
    // answer has to carry what the report *should* have said, or the
    // study cannot score whether the person noticed its absence.
    expect(seeded.truth.shown).toBeNull();
    expect(seeded.truth.gold).toBe("12.99");
  });

  it("a wrong amount moves the fields derived from it, so the row does not betray the seed", () => {
    // EvidenceRow renders the yearly figure and the price-rise mark
    // beside the amount. If only `amount_current` moved, £129.90 would
    // sit next to "£10.99 → £12.99" and a participant could detect the
    // seeding artefact instead of reading the evidence.
    const seeded = seed(clean(), { operator: "wrong-amount", target: "Netflix" });
    const netflix = seeded.report.recurring.find((f) => f.merchant === "Netflix");

    expect(netflix?.amount_current).toBe("129.90");
    expect(netflix?.annualised).toBe("1558.80");
    expect(netflix?.price_rise?.to).toBe("129.90");
    expect(netflix?.price_rise?.extra_per_year).toBe("1426.92");
    expect(seeded.report.summary.annualised_total).toBe("2449.56");
    expect(seeded.report.summary.monthly_equivalent).toBe("204.13");
    expect(seeded.report.summary.recurring_count).toBe(5);
  });

  it("an omission recomputes the report's own totals so they agree with its rows", () => {
    const seeded = seed(clean(), { operator: "dropped-claim", target: "Netflix" });

    expect(seeded.report.summary.recurring_count).toBe(4);
    expect(seeded.report.summary.price_rises).toBe(0);
    expect(seeded.report.summary.annualised_total).toBe("890.76");
    expect(seeded.report.summary.monthly_equivalent).toBe("74.23");
  });

  it("a_mis_relation_leaves_every_figure_genuine_and_moves_only_what_the_period_derives", () => {
    // #568's rung 1 is why this operator exists. Closed questions over
    // short passages invent at 0.21% (1 in 470, Wilson [0.04%, 1.20%]),
    // so a study seeded with invented figures measures an error class
    // that barely happens. What the prose arm produced instead, five
    // times in ten explanations, was a *real figure read under the
    // wrong relation*: funds brought forward presented as income,
    // turning a £62,556 gain into a £49k loss.
    //
    // A period seed is that harm in this pack's language. Every
    // transaction stays, the amount stays, the median interval stays —
    // so a participant who opens the evidence finds each figure
    // confirmed, and the reading is still wrong. The twelve payments
    // and the 30-day median are left deliberately untouched: they are
    // the affordance that makes this checkable rather than impossible,
    // exactly as the words "brought forward" were on the page in the
    // accounts.
    const seeded = seed(clean(), { operator: "wrong-period", target: "Netflix" });
    const netflix = seeded.report.recurring.find((f) => f.merchant === "Netflix");

    expect(seeded.truth.operator).toBe("wrong-period");
    expect(seeded.truth.gold).toBe("monthly");
    expect(seeded.truth.shown).toBe("quarterly");

    expect(netflix?.period).toBe("quarterly");
    // Genuine: the seed must not touch a figure a participant can check.
    expect(netflix?.amount_current).toBe("12.99");
    expect(netflix?.price_rise?.from).toBe("10.99");
    expect(netflix?.price_rise?.to).toBe("12.99");
    expect(netflix?.evidence.transactions).toHaveLength(12);
    expect(netflix?.evidence.interval_days?.median).toBe(30);
    // Derived: what the wrong relation implies must follow it, or the
    // row contradicts itself and betrays the seed.
    expect(netflix?.annualised).toBe("51.96");
    expect(netflix?.price_rise?.extra_per_year).toBe("8.00");
    expect(seeded.report.summary.annualised_total).toBe("942.72");
    expect(seeded.report.summary.recurring_count).toBe(5);
  });

  it("a rise dated to the month before it happened keeps every figure and moves only when", () => {
    // The second mis-relation, and the purest one: nothing derives from
    // the month, so *only* the relation moves. The report still says
    // £10.99 → £12.99 and an extra £24.00 a year, all three genuine.
    //
    // April rather than an arbitrary month because an off-by-one at the
    // boundary is the misreading a model actually makes — the same
    // adjacent-row confusion that read "funds brought forward" as income
    // in #568's rung 1. It is also self-refuting at the point the report
    // draws attention to: EvidenceRow highlights the chip whose date
    // starts with `price_rise.month`, and April's chip still reads
    // £10.99. The evidence contradicts the claim exactly where the claim
    // points.
    const seeded = seed(clean(), { operator: "wrong-rise-month", target: "Netflix" });
    const netflix = seeded.report.recurring.find((f) => f.merchant === "Netflix");

    expect(seeded.truth.operator).toBe("wrong-rise-month");
    expect(seeded.truth.gold).toBe("2025-05");
    expect(seeded.truth.shown).toBe("2025-04");

    expect(netflix?.price_rise?.month).toBe("2025-04");
    // Every figure genuine: a participant checking any number finds it
    // confirmed, which is what makes this a mis-relation and not an
    // invention.
    expect(netflix?.price_rise?.from).toBe("10.99");
    expect(netflix?.price_rise?.to).toBe("12.99");
    expect(netflix?.price_rise?.extra_per_year).toBe("24.00");
    expect(netflix?.amount_current).toBe("12.99");
    expect(netflix?.annualised).toBe("155.88");
    expect(netflix?.evidence.transactions).toHaveLength(12);
    // The report's own totals are untouched, because nothing derives
    // from when a rise happened.
    expect(seeded.report.summary.annualised_total).toBe("1046.64");
    expect(seeded.report.summary.price_rises).toBe(1);

    // The contradiction the participant has to find: the highlighted
    // month's payment is still the old price.
    const highlighted = netflix?.evidence.transactions.find((t) =>
      t.date.startsWith("2025-04"),
    );
    expect(highlighted?.amount).toBe("10.99");
  });

  it("refuses a rise it cannot misdate rather than seeding a month with no payment", () => {
    const before = clean();
    const netflix = before.recurring.find((f) => f.merchant === "Netflix");
    // January's predecessor is December of the year before, which has
    // no payment in this report. The refusal names the month that would
    // have highlighted nothing.
    if (netflix?.price_rise) netflix.price_rise.month = "2025-01";
    expect(() =>
      seed(before, { operator: "wrong-rise-month", target: "Netflix" }),
    ).toThrow(/no payment in 2024-12/);
  });

  it("refuses to misdate a rise that is not there", () => {
    // Spotify has no price rise. Seeding one would be inventing a claim
    // under the name of a mis-relation, and the truth record would
    // describe an error the report does not contain.
    expect(() =>
      seed(clean(), { operator: "wrong-rise-month", target: "Spotify" }),
    ).toThrow(/no price rise for Spotify/);
  });

  it("refuses an amount it cannot shift exactly rather than writing NaN", () => {
    const before = clean();
    const netflix = before.recurring.find((f) => f.merchant === "Netflix");
    if (netflix) netflix.amount_current = "1,299.00";
    expect(() => seed(before, { operator: "wrong-amount", target: "Netflix" })).toThrow(
      /1,299.00/,
    );
  });

  it("refuses a target it cannot find rather than seeding nothing", () => {
    // A silent no-op would produce a "seeded" report identical to the
    // control, and every participant shown it would be recorded as
    // having missed an error that was never there.
    expect(() =>
      seed(clean(), { operator: "wrong-amount", target: "Not A Merchant" }),
    ).toThrow(/Not A Merchant/);
  });
});
