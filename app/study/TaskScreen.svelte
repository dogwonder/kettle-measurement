<script lang="ts">
  // #431: one task, in two steps.
  //
  // The two steps are what make "confidence before checking" mean the
  // same thing for everybody. In step one the report shows its figures
  // and nothing behind them, and the question is asked there. In step
  // two the evidence and the statement arrive, and the answer is given.
  // Without the gate, "before" would mean whatever each participant
  // happened to do first, and a paired within-participant measure would
  // be comparing differently-defined moments.
  //
  // The gate is identical for every arm and every error class, so it
  // cannot bias the primary comparison — it costs realism (a person
  // reading their own report opens evidence as they go) and buys a
  // measure that means one thing. Declared here rather than discovered
  // in the analysis.
  //
  // The copy says nothing about what is expected to happen. No mention
  // of errors having been planted, no encouragement to trust or
  // distrust, and the same words on every task whether it carries a
  // seed or not.
  import Button from "../src/lib/components/Button.svelte";
  import type { LetterTask } from "../src/lib/study-letter-session";
  import { ABSENT, type Response } from "../src/lib/study-response";
  import type { Arm, Task } from "../src/lib/study-session";
  import { statementFor } from "./corpus";
  import LetterSourceView from "./LetterSourceView.svelte";
  import LetterView from "./LetterView.svelte";
  import ReportView from "./ReportView.svelte";
  import StatementView from "./StatementView.svelte";

  let {
    task,
    arm,
    total,
    onanswer,
    now = () => Date.now(),
  }: {
    /** A statement task or a letter task; the questions are the same. */
    task: Task | LetterTask;
    arm: Arm;
    total: number;
    onanswer: (response: Response) => void;
    /** Injected so a test can measure without waiting. */
    now?: () => number;
  } = $props();

  type Step = "read" | "check";

  let step = $state<Step>("read");
  let before = $state<number | null>(null);
  let after = $state<number | null>(null);
  let verdict = $state<"accept" | "reject" | null>(null);
  let flagged = $state<string[]>([]);
  let correction = $state("");
  let opened = $state<string[]>([]);
  // Set by the effect below, which also runs on mount — reading the
  // `now` prop here would capture it once and never see a new task.
  let startedAt = $state(0);

  // A new task resets everything. Without this the second task inherits
  // the first one's answer and the participant confirms it by accident.
  $effect(() => {
    task.index;
    step = "read";
    before = null;
    after = null;
    verdict = null;
    flagged = [];
    correction = "";
    opened = [];
    startedAt = now();
  });

  // What is on the page to point at, in the words the cards head
  // themselves with: the same strings `record` puts in `opened` and
  // both scorers compare a flag against. Derived rather than copied,
  // so a claim cannot be offered that the report does not show.
  const claims = $derived(
    "letter" in task
      ? task.letter.actions.actions.map((action) => action.title)
      : task.report.recurring.map((finding) => finding.merchant),
  );

  const scale = [1, 2, 3, 4, 5];
  const SCALE_ENDS = ["Not at all confident", "Completely confident"] as const;

  function check() {
    if (before === null) return;
    step = "check";
  }

  function submit() {
    if (verdict === null || after === null) return;
    onanswer({
      verdict,
      flagged: verdict === "reject" ? flagged : [],
      offered: claims,
      correction: correction.trim() === "" ? null : correction.trim(),
      confidence_before: before as 1 | 2 | 3 | 4 | 5,
      confidence_after: after as 1 | 2 | 3 | 4 | 5,
      opened,
      elapsed_ms: now() - startedAt,
    });
  }

  function record(claim: string) {
    if (!opened.includes(claim)) opened = [...opened, claim];
  }
</script>

<header>
  <p class="progress">Report {task.index + 1} of {total}</p>
</header>

{#if "letter" in task}
  <LetterView
    letter={task.letter}
    disclose={step === "check"}
    emphasis={arm === "emphasised" ? task.emphasis : null}
    onopen={record}
  />
{:else}
  <ReportView
    report={task.report}
    disclose={step === "check"}
    emphasis={arm === "emphasised" ? task.emphasis : null}
    onopen={record}
  />
{/if}

{#if step === "check"}
  <section class="aside">
    {#if "letter" in task}
      <LetterSourceView text={task.letter.source.text} file={task.letter.source.file} />
    {:else}
      <StatementView csv={statementFor(task.report)} file={task.report.run.input.file} />
    {/if}
  </section>
{/if}

<section class="ask">
  {#if step === "read"}
    {@render scaleQuestion("before", "How confident are you that this report is right?")}
    <Button label="Check it" onclick={check} />
  {:else}
    <div class="govuk-form-group">
      <fieldset class="govuk-fieldset">
        <legend class="govuk-fieldset__legend govuk-fieldset__legend--m">
          Having checked it, what do you say about this report?
        </legend>
        <div class="govuk-radios govuk-radios--small">
          <div class="govuk-radios__item">
            <input class="govuk-radios__input" id="verdict-accept" type="radio" name="verdict" value="accept" bind:group={verdict} />
            <label class="govuk-label govuk-radios__label" for="verdict-accept">Everything in it looks right</label>
          </div>
          <div class="govuk-radios__item">
            <input class="govuk-radios__input" id="verdict-reject" type="radio" name="verdict" value="reject" bind:group={verdict} />
            <label class="govuk-label govuk-radios__label" for="verdict-reject">Something in it is wrong</label>
          </div>
        </div>
      </fieldset>
    </div>

    {#if verdict === "reject"}
      <!-- Picked from what is on the page, not typed: both scorers
           compare a flag to the claim's title exactly, so a prose
           answer could never be one, and the first sitting of this
           instrument rejected ten reports and scored ten false alarms.
           `ABSENT` is the one option that is not a claim, because an
           omission's answer is the ask that is not there.

           Tick boxes rather than radios, because a report does not
           always have one thing wrong with it. Asking "which one?"
           made a participant who saw the seed *and* something else
           choose between them, and three of the author's first thirty
           answers put the second complaint in the correction box for
           want of anywhere to say it. Ticking everything is not a free
           pass: each tick that carries no seed is scored as a false
           alarm of its own. -->
      <div class="govuk-form-group">
        <fieldset class="govuk-fieldset" aria-describedby="flagged-hint">
          <legend class="govuk-fieldset__legend govuk-fieldset__legend--m">
            Which ones? Tick everything you think is wrong.
          </legend>
          <div class="govuk-hint" id="flagged-hint">Leave them all unticked if you can't say.</div>
          <div class="govuk-checkboxes govuk-checkboxes--small">
            {#each [...claims, ABSENT] as claim, i (claim)}
              <div class="govuk-checkboxes__item">
                <input class="govuk-checkboxes__input" id="flagged-{i}" type="checkbox" value={claim} bind:group={flagged} />
                <label class="govuk-label govuk-checkboxes__label" for="flagged-{i}">{claim}</label>
              </div>
            {/each}
          </div>
        </fieldset>
      </div>
      <!-- The correction box is now the only route by which anything
           about a participant could reach a file that gets published —
           the box beside it became a list of the report's own claims —
           so the reminder lives beside it, as its hint. On the consent
           page alone it would have been read once, ten reports ago. -->
      <div class="govuk-form-group">
        <label class="govuk-label" for="correction">
          What should they say instead? Leave this blank if you can't say.
        </label>
        <div class="govuk-hint" id="correction-hint">
          Please don't type anything about yourself — these answers are published.
        </div>
        <textarea class="govuk-textarea" id="correction" rows="4" aria-describedby="correction-hint" bind:value={correction}></textarea>
      </div>
    {/if}

    {@render scaleQuestion("after", "How confident are you now?")}
    <Button label="Next report" onclick={submit} />
  {/if}
</section>

<!-- One five-point scale, asked twice. The ends are hints rather than
     labels: a label names one control, and "Not at all confident"
     names the direction of five. -->
{#snippet scaleQuestion(name: "before" | "after", question: string)}
  <div class="govuk-form-group">
    <fieldset class="govuk-fieldset" aria-describedby="{name}-hint">
      <legend class="govuk-fieldset__legend govuk-fieldset__legend--m">{question}</legend>
      <div class="govuk-hint" id="{name}-hint">1 is {SCALE_ENDS[0].toLowerCase()}, 5 is {SCALE_ENDS[1].toLowerCase()}.</div>
      <div class="govuk-radios govuk-radios--inline govuk-radios--small">
        {#each scale as point (point)}
          <div class="govuk-radios__item">
            {#if name === "before"}
              <input class="govuk-radios__input" id="before-{point}" type="radio" name="before" value={point} bind:group={before} />
            {:else}
              <input class="govuk-radios__input" id="after-{point}" type="radio" name="after" value={point} bind:group={after} />
            {/if}
            <label class="govuk-label govuk-radios__label" for="{name}-{point}">{point}</label>
          </div>
        {/each}
      </div>
    </fieldset>
  </div>
{/snippet}

<style lang="scss">
  header {
    @include k-repel;
    margin-bottom: k-spacing(xs);
  }
  .progress {
    margin: 0;
    @include k-font(tiny, 700);
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--k-grey);
  }
  .aside,
  .ask {
    margin-top: k-spacing(m);
  }
</style>
