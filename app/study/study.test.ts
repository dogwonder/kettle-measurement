// #431: the harness a participant actually uses.
//
// Automated at the widest stable boundary — the whole session, driven
// the way a person drives it — so the acceptance example is the thing
// the study runs, not a unit beneath it. The units are tested in
// `src/lib/study-*.test.ts`; what this file holds is that the interface
// produces the record those units score.

import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/svelte";
import { afterEach, describe, expect, it } from "vitest";
import { CONSENT, type ConsentText } from "../src/lib/study-consent";
import { scores } from "../src/lib/study-record";
import { ABSENT } from "../src/lib/study-response";
import { plan } from "../src/lib/study-session";
import type { RunReport } from "../src/lib/types";
import { planLetters } from "../src/lib/study-letter-session";
import { corpus, letters, rowsOf, statementFor, uncleanLetters } from "./corpus";
import ReportView from "./ReportView.svelte";
import TaskScreen from "./TaskScreen.svelte";
import StudyApp from "./StudyApp.svelte";

const studyRoot = dirname(fileURLToPath(import.meta.url));
const repoRoot = dirname(dirname(studyRoot));

afterEach(cleanup);

/** Tick the clock by a fixed step so review time is measurable without
 * a test that waits. */
function clock(step = 1_000): () => number {
  let at = 0;
  return () => (at += step);
}

/**
 * The consent text with a hole in it.
 *
 * The shipped text is finished (`study-consent.test.ts` holds it to
 * that), so the refusal it is capable of needs an unfinished document
 * to demonstrate on. Kept rather than deleted with the gaps it was
 * written for: the next open question about how these files are handled
 * arrives the same way, and the mechanism is what stops it reaching a
 * participant as a blank.
 */
function unfinished(): ConsentText {
  return {
    ...CONSENT,
    sections: CONSENT.sections.map((section, index) =>
      index === 0
        ? { ...section, paragraphs: ["[UNSETTLED: retention]"] }
        : section,
    ),
    unsettled: [
      { field: "retention", why: "An example's stand-in.", since: "2026-08-27" },
    ],
  };
}

async function consent(participant = "p01", material: "statements" | "letters" = "statements") {
  render(StudyApp, {
    now: () => "2026-09-01T10:00:00Z",
    clock: clock(),
    material,
  });
  await fireEvent.input(screen.getByLabelText(/participant number/i), {
    target: { value: participant },
  });
  await fireEvent.click(screen.getByLabelText(/happy to take part/i));
  await fireEvent.click(screen.getByRole("button", { name: "Start" }));
}

/** Answer the task on screen: confidence, check, verdict, confidence. */
async function answer(options: {
  before: number;
  after: number;
  reject?: { which?: string | string[]; instead?: string };
  open?: string;
}) {
  await fireEvent.click(screen.getByRole("radio", { name: String(options.before) }));
  await fireEvent.click(screen.getByRole("button", { name: "Check it" }));

  if (options.open !== undefined) {
    const row = screen.getByText(options.open).closest("details");
    (row as HTMLDetailsElement).open = true;
    await fireEvent(row as HTMLDetailsElement, new Event("toggle"));
  }

  await fireEvent.click(
    screen.getByRole("radio", {
      name:
        options.reject === undefined
          ? /Everything in it looks right/
          : /Something in it is wrong/,
    }),
  );
  if (options.reject !== undefined) {
    // Picked, never typed, and now more than one may be picked: the box
    // this replaced took prose, and the scorer compares an answer to
    // the claim's title exactly, so the old idiom passed only because
    // the test typed the gold answer in — which no participant can do,
    // and none did.
    for (const which of options.reject.which === undefined
      ? []
      : [options.reject.which].flat()) {
      await fireEvent.click(screen.getByRole("checkbox", { name: which }));
    }
    if (options.reject.instead !== undefined) {
      await fireEvent.input(screen.getByLabelText(/What should they say instead\?/), {
        target: { value: options.reject.instead },
      });
    }
  }
  const afters = screen.getAllByRole("radio", { name: String(options.after) });
  await fireEvent.click(afters[afters.length - 1] as HTMLElement);
  await fireEvent.click(screen.getByRole("button", { name: "Next report" }));
}

describe("the participant harness", () => {
  it("sits under the number it was issued with, rather than the one the box suggests", async () => {
    // The script prints the id in a terminal; the session happens in a
    // browser. On the first sitting the screen won: the box suggested
    // `p01`, the author typed it, and the file came out claiming to be
    // one of the twenty. So the id travels with the link, and the
    // participant cannot edit what somebody else is answerable for.
    render(StudyApp, { now: () => "2026-09-01T10:00:00Z", clock: clock(), participant: "a01" });
    const box = screen.getByLabelText(/Your participant number/) as HTMLInputElement;
    expect(box.value).toBe("a01");
    expect(box.readOnly).toBe(true);

    await fireEvent.click(screen.getByRole("checkbox"));
    await fireEvent.click(screen.getByRole("button", { name: "Start" }));
    const session = planLetters("a01", letters, uncleanLetters);
    for (let i = 0; i < session.tasks.length; i += 1) await answer({ before: 3, after: 3 });

    const written = screen.getByLabelText("Your answers") as HTMLTextAreaElement;
    const transcript = JSON.parse(written.value);
    expect(transcript.participant).toBe("a01");
    expect(transcript.role).toBe("author-pilot");
  });

  it("scores a participant who says an ask is missing, on the class with no card to point at", async () => {
    // The omission operators drop a claim, so `truth.target` names
    // something that is not on the page. A list of what *is* there can
    // never hold the right answer, which is why the list carries one
    // option that is not a claim.
    await consent();
    const session = plan("p01", corpus);
    const omission = session.tasks.findIndex((task) => task.class === "omission");
    expect(omission).toBeGreaterThan(-1);

    for (let i = 0; i < omission; i += 1) await answer({ before: 3, after: 3 });
    await answer({ before: 3, after: 2, reject: { which: ABSENT } });
    for (let i = omission + 1; i < session.tasks.length; i += 1) {
      await answer({ before: 3, after: 3 });
    }

    const written = screen.getByLabelText("Your answers") as HTMLTextAreaElement;
    const transcript = JSON.parse(written.value);
    expect(transcript.responses[omission].flagged).toEqual([ABSENT]);
    expect(scores(transcript, corpus)[omission]?.outcome).toBe("correct-detection");
  });

  it("shows every word the transcript stamps, and nothing the file does not hold", async () => {
    // The page *is* the consent document, so what is checked here is
    // the text we would actually hand somebody — not a fixture.
    render(StudyApp, { now: () => "2026-09-01T10:00:00Z", clock: clock() });

    for (const section of CONSENT.sections) {
      expect(
        screen.getByRole("heading", { name: section.heading }),
      ).toBeInTheDocument();
      for (const paragraph of [...section.paragraphs, ...(section.after ?? [])]) {
        expect(screen.getByText(paragraph)).toBeInTheDocument();
      }
    }
    // The list of recorded fields is the transcript's own key list, in
    // a participant's words, so the form cannot describe a file the
    // harness does not write.
    for (const field of CONSENT.records) {
      expect(screen.getByText(field.plain)).toBeInTheDocument();
    }
    expect(
      screen.getByText(`This page, version ${CONSENT.version}.`),
    ).toBeInTheDocument();

    // No leftover notice: a finished form must not carry the furniture
    // of an unfinished one.
    expect(
      screen.queryByRole("heading", { name: "Not ready to run a session" }),
    ).not.toBeInTheDocument();
  });

  it("refuses to start over a consent text with a gap in it", async () => {
    // Demonstrated on an unfinished document, because the shipped one
    // is finished. The refusal is a property of the harness, not of
    // today's wording, and it is what stops the next open question
    // about these files reaching a participant as a blank.
    render(StudyApp, {
      now: () => "2026-09-01T10:00:00Z",
      clock: clock(),
      text: unfinished(),
    });
    await fireEvent.input(screen.getByLabelText(/participant number/i), {
      target: { value: "p01" },
    });
    await fireEvent.click(screen.getByLabelText(/happy to take part/i));

    expect(
      screen.getByRole("heading", { name: "Not ready to run a session" }),
    ).toBeInTheDocument();
    expect(screen.getByText("Still to be written: retention")).toBeInTheDocument();
    const start = screen.getByRole("button", { name: "Start" });
    expect(start).toBeDisabled();
    await fireEvent.click(start);
    // Pressed anyway — the attribute only stops a mouse. What stops a
    // session is the refusal, said in the words of the gap.
    expect(screen.getByRole("alert")).toHaveTextContent(
      /retention still to be written/,
    );
    expect(screen.queryByText("Report 1 of 10")).not.toBeInTheDocument();
  });

  it("asks before the evidence exists and again after it does", async () => {
    // The gate is what makes "confidence before checking" mean the same
    // moment for everybody. Step one is the report's figures with
    // nothing behind them; step two is the evidence and the statement.
    await consent();
    const first = plan("p01", corpus).tasks[0]!;
    const merchant = first.report.recurring[0]!.merchant;

    expect(screen.getByText("Report 1 of 10")).toBeInTheDocument();
    expect(screen.getByText(merchant)).toBeInTheDocument();
    expect(screen.queryByText("Show")).not.toBeInTheDocument();
    expect(screen.queryByText(/Open the statement/)).not.toBeInTheDocument();
    expect(
      screen.getByText(/How confident are you that this report is right\?/),
    ).toBeInTheDocument();

    await fireEvent.click(screen.getByRole("radio", { name: "4" }));
    await fireEvent.click(screen.getByRole("button", { name: "Check it" }));

    expect(screen.getAllByText("Show").length).toBeGreaterThan(0);
    expect(screen.getByText(/Open the statement/)).toBeInTheDocument();
    expect(screen.getByText(/How confident are you now\?/)).toBeInTheDocument();
  });

  it("records what the participant did, and it scores as a detection", async () => {
    await consent();
    const session = plan("p01", corpus);
    const first = session.tasks[0]!;
    const target = first.truth?.target;
    // The first task in p01's session carries a seed; a clean one would
    // make this example test nothing.
    expect(target).toBeTypeOf("string");

    await answer({
      before: 4,
      after: 2,
      open: target as string,
      reject: { which: target as string },
    });

    // Task two is on screen, and the answer to task one is in the
    // transcript rather than in a variable somewhere.
    expect(screen.getByText("Report 2 of 10")).toBeInTheDocument();

    // Walk out through the remaining tasks to reach the file.
    for (let i = 1; i < session.tasks.length; i += 1) {
      await answer({ before: 3, after: 3 });
    }

    const written = screen.getByLabelText("Your answers") as HTMLTextAreaElement;
    const transcript = JSON.parse(written.value);
    expect(transcript.participant).toBe("p01");
    expect(transcript.arm).toBe(session.arm);
    expect(transcript.responses).toHaveLength(10);

    const first_answer = transcript.responses[0];
    expect(first_answer.flagged).toEqual([target]);
    expect(first_answer.opened).toEqual([target]);
    expect(first_answer.confidence_before).toBe(4);
    expect(first_answer.confidence_after).toBe(2);
    expect(first_answer.elapsed_ms).toBeGreaterThan(0);

    const table = scores(transcript, corpus);
    expect(table).toHaveLength(10);
    expect(table[0]?.outcome).toBe("correct-detection");
    expect(table[0]?.opened_target).toBe(true);
  });

  it("hands the participant the statement, because an omission is not in the report", async () => {
    // The one class that cannot be checked inside the report. Without
    // the source document its detection rate would be zero by
    // construction, and a zero the harness produced looks exactly like
    // a finding about people.
    const session = plan("p01", corpus);
    const omission = session.tasks.find((task) => task.class === "omission")!;
    const dropped = omission.truth?.target as string;

    expect(omission.report.recurring.map((f) => f.merchant)).not.toContain(dropped);
    const lines = rowsOf(statementFor(omission.report));
    expect(lines.some((row) => row.description.length > 0)).toBe(true);
    // The dropped commitment is still every bit as present in the
    // statement as it was before it was dropped from the report.
    const source = corpus.find((report) => report.run.id === omission.document)!;
    const raw = source.recurring.find((f) => f.merchant === dropped)!.raw_merchant;
    expect(lines.some((row) => row.description === raw)).toBe(true);
  });

  it("points at a claim in one arm and at nothing in the other", async () => {
    // Condition 3 and condition 4 are the same report and the same
    // emphasis target; the only difference is whether the arm asks for
    // it. Driven through TaskScreen rather than ReportView, because the
    // arm is exactly what TaskScreen decides — a test that passed the
    // emphasis in by hand would prove the component works and leave the
    // decision unchecked.
    //
    // No participant is assigned `emphasised` (see `armFor`): condition
    // 4 is not run in this study. The presentation stays built and
    // stays tested here, so it is ready to be a study of its own rather
    // than something to rebuild from a description.
    const session = plan("p01", corpus);
    const task = session.tasks[0]!;

    const evidence = render(TaskScreen, {
      task,
      arm: "evidence",
      total: 10,
      onanswer: () => {},
      now: clock(),
    });
    await fireEvent.click(screen.getByRole("radio", { name: "3" }));
    await fireEvent.click(screen.getByRole("button", { name: "Check it" }));
    expect(evidence.container.querySelector(".emphasised")).toBeNull();
    cleanup();

    const emphasised = render(TaskScreen, {
      task,
      arm: "emphasised",
      total: 10,
      onanswer: () => {},
      now: clock(),
    });
    await fireEvent.click(screen.getByRole("radio", { name: "3" }));
    await fireEvent.click(screen.getByRole("button", { name: "Check it" }));
    const marked = emphasised.container.querySelectorAll(".emphasised");
    expect(marked).toHaveLength(1);
    expect(marked[0]?.textContent).toContain(task.emphasis);
    // Open, so the evidence is pointed at rather than merely available.
    expect((marked[0] as HTMLDetailsElement).open).toBe(true);
  });

  it("shows the same figures the runner's own report shows", async () => {
    // The harness assembles the report from the app's components rather
    // than framing `report.html`, because three of #431's four
    // conditions are the same report shown differently and a rendered
    // document can only be one of them.
    //
    // That is a real gap: today the desktop app shows `report.html`, so
    // this assembly is not yet the artefact a Kettle user reads. This
    // bounds it to presentation — every merchant and every figure the
    // runner printed has to be on the page here too, or the study is
    // measuring a different report rather than a different layout.
    const run = JSON.parse(
      readFileSync(join(repoRoot, "fixtures/run-01/results.json"), "utf8"),
    ) as RunReport;
    const html = readFileSync(join(repoRoot, "fixtures/run-01/report.html"), "utf8");
    const printed = html.split("</style>").pop() ?? "";

    const { container } = render(ReportView, { report: run });
    const shown = container.textContent ?? "";

    for (const finding of run.recurring) {
      expect(printed, `${finding.merchant} is not in report.html`).toContain(
        finding.merchant,
      );
      expect(shown, `${finding.merchant} is missing here`).toContain(finding.merchant);
      for (const amount of [finding.amount_current, finding.annualised]) {
        const money = `£${Number(amount).toLocaleString("en-GB", {
          minimumFractionDigits: 2,
        })}`;
        expect(shown, `${finding.merchant}: ${money}`).toContain(money);
      }
    }
  });

  it("refuses a participant it cannot place in the enrolment order, in the words of the refusal", async () => {
    // The design is fixed-n with a futility stop at n = 10, so where a
    // participant sits in the order is protocol rather than
    // bookkeeping. A harness that swallowed the refusal would run a
    // session it could not analyse under either rule, and nobody would
    // find out until the analysis.
    render(StudyApp, {
      now: () => "2026-09-01T10:00:00Z",
      clock: clock(),
    });
    await fireEvent.input(screen.getByLabelText(/participant number/i), {
      target: { value: "pilot" },
    });
    await fireEvent.click(screen.getByLabelText(/happy to take part/i));
    await fireEvent.click(screen.getByRole("button", { name: "Start" }));

    expect(within(screen.getByRole("alert")).getByText(/enrolment number/)).toBeInTheDocument();
    expect(screen.queryByText("Report 1 of 10")).not.toBeInTheDocument();
  });

  it("runs the letter track: the cards, the letter, and a file that says who sat it", async () => {
    // The author's own pilot (27 August 2026): an `a`-numbered id, the
    // letter corpus, and a transcript marked as a pilot rather than
    // remembered as one.
    await consent("a01", "letters");
    const session = planLetters("a01", letters, uncleanLetters);
    const first = session.tasks[0]!;
    expect(screen.getByText("Report 1 of 10")).toBeInTheDocument();
    expect(screen.getByText(`What ${first.letter.source.file} asks of you`)).toBeInTheDocument();

    // Before checking there is no evidence to open and no letter.
    expect(screen.queryByText(/Show the evidence/)).toBeNull();
    expect(screen.queryByText(/Open the letter/)).toBeNull();
    await fireEvent.click(screen.getByRole("radio", { name: "3" }));
    await fireEvent.click(screen.getByRole("button", { name: "Check it" }));
    expect(screen.getByText(/Open the letter/)).toBeInTheDocument();
    if (first.letter.actions.actions.length > 0) {
      expect(screen.getAllByText(/Show the evidence/).length).toBe(first.letter.actions.actions.length);
    }
    // The cards are to read, not to act on.
    expect(screen.queryByRole("button", { name: /Approve/ })).toBeNull();

    // Finish the task and the rest of the session.
    await fireEvent.click(screen.getByRole("radio", { name: /Everything in it looks right/ }));
    const afters = screen.getAllByRole("radio", { name: "3" });
    await fireEvent.click(afters[afters.length - 1] as HTMLElement);
    await fireEvent.click(screen.getByRole("button", { name: "Next report" }));
    for (let i = 1; i < session.tasks.length; i += 1) {
      await answer({ before: 3, after: 3 });
    }

    const written = screen.getByLabelText("Your answers") as HTMLTextAreaElement;
    const transcript = JSON.parse(written.value);
    expect(transcript.participant).toBe("a01");
    expect(transcript.role).toBe("author-pilot");
    expect(transcript.material).toBe("letters");
    expect(transcript.corpus).toEqual(letters.map((letter) => letter.id));
    const table = scores(transcript, { material: "letters", letters, unclean: uncleanLetters });
    expect(table).toHaveLength(10);
    expect(table[0]?.outcome).toBe(first.truth === null ? "correct-acceptance" : "false-acceptance");
  });

  it("names the seeded action when a letter task is caught", async () => {
    await consent("a02", "letters");
    const session = planLetters("a02", letters, uncleanLetters);
    const seeded = session.tasks[0]!;
    // Skip to the answer on the first task, whatever it carries. A
    // dropped obligation is the one case where the right answer is not
    // on the page, so it is picked as an absence rather than as a card.
    const target = seeded.truth?.target ?? seeded.letter.actions.actions[0]?.title ?? "";
    const dropped = seeded.truth?.operator === "dropped-obligation";
    await answer({
      before: 4,
      after: 2,
      ...(seeded.truth !== null && !dropped ? { open: target } : {}),
      reject: { which: dropped ? ABSENT : target },
    });
    expect(screen.getByText("Report 2 of 10")).toBeInTheDocument();
    for (let i = 1; i < session.tasks.length; i += 1) {
      await answer({ before: 3, after: 3 });
    }
    const transcript = JSON.parse((screen.getByLabelText("Your answers") as HTMLTextAreaElement).value);
    const table = scores(transcript, { material: "letters", letters, unclean: uncleanLetters });
    expect(table[0]?.outcome).toBe(seeded.truth === null ? "false-alarm" : "correct-detection");
    if (seeded.truth !== null && seeded.truth.operator !== "dropped-obligation") {
      expect(table[0]?.opened_target).toBe(true);
    }
  });
  it("asks every question on GOV.UK form components, not hand-rolled controls", async () => {
    // The forms are the one place govuk's evidence is strongest —
    // fieldset and legend semantics, hit targets, focus, hints — and
    // the study is the first screen in Kettle that has one. A control
    // the skin does not know about is a rule that says nowhere it came
    // from, and the correction box floating beside its label on the
    // first `a03` screen is what that looks like.
    render(StudyApp, { now: () => "2026-09-01T10:00:00Z", clock: clock(), material: "statements" });
    const has = (selector: string) => document.body.querySelector(selector) !== null;
    expect(has(".govuk-input")).toBe(true);
    expect(has(".govuk-checkboxes__input")).toBe(true);
    await fireEvent.input(screen.getByLabelText(/participant number/i), { target: { value: "p01" } });
    await fireEvent.click(screen.getByLabelText(/happy to take part/i));
    await fireEvent.click(screen.getByRole("button", { name: "Start" }));
    expect(has(".govuk-fieldset__legend")).toBe(true);
    expect(has(".govuk-radios__input")).toBe(true);
    await fireEvent.click(screen.getByRole("radio", { name: "4" }));
    await fireEvent.click(screen.getByRole("button", { name: "Check it" }));
    await fireEvent.click(screen.getByRole("radio", { name: /Something in it is wrong/ }));
    expect(has(".govuk-checkboxes__input")).toBe(true);
    expect(has(".govuk-textarea")).toBe(true);
    expect(has(".govuk-hint")).toBe(true);
    expect(document.body.querySelector("input:not([class*='govuk-']), textarea:not([class*='govuk-'])")).toBeNull();
  });
});
