<script lang="ts">
  // #431: the whole session — consent, ten tasks, and the file.
  //
  // The harness is deliberately separate from the product. #431 says so
  // in as many words ("the comparison belongs in a separate study/demo
  // harness"), and the reason is not tidiness: a study needs conditions
  // the product must never grow, starting with a report shown without
  // its evidence.
  //
  // Nothing is sent anywhere. The transcript is written by the
  // participant's own browser and handed to whoever is running the
  // session, which is the same boundary the product keeps and the only
  // one worth promising in a consent form.
  import Button from "../src/lib/components/Button.svelte";
  import {
    CONSENT,
    stamp,
    unsettledIn,
    type ConsentText,
  } from "../src/lib/study-consent";
  import {
    begin,
    record,
    type Material,
    type StudyCorpus,
    type Transcript,
  } from "../src/lib/study-record";
  import type { Response } from "../src/lib/study-response";
  import ConsentForm from "./ConsentForm.svelte";
  import { corpus, letters, uncleanLetters } from "./corpus";
  import TaskScreen from "./TaskScreen.svelte";
  import { plan, type SessionPlan } from "../src/lib/study-session";
  import { planLetters, type LetterSessionPlan } from "../src/lib/study-letter-session";

  let {
    now = () => new Date().toISOString(),
    clock = () => Date.now(),
    text = CONSENT,
    material = "letters",
    participant: issued = null,
  }: {
    /** Injected so a transcript is reproducible in a test. */
    now?: () => string;
    clock?: () => number;
    /**
     * Which documents the session reads. Letters by default since 27
     * August 2026; the statement track stays available. Chosen by
     * whoever runs the session, never by the participant, so it is a
     * prop and a `?material=` query rather than a control on the gate.
     */
    material?: Material;
    /**
     * The number this session was issued with, from `?participant=`.
     * Whoever runs the session decides it — the same reason `material`
     * is a prop — and when it is given the box is theirs, not the
     * participant's, so nobody can sit under a number somebody else is
     * answerable for.
     */
    participant?: string | null;
    /** The words on the gate. A prop so a test can drive the session
     * without waiting for the two facts only the study's runner can
     * supply — see `unfinished`. */
    text?: ConsentText;
  } = $props();

  let participant = $state(issued ?? "");
  let agreed = $state(false);
  let session = $state<SessionPlan | LetterSessionPlan | null>(null);

  const chosen = $derived<StudyCorpus>(
    material === "letters"
      ? { material, letters, unclean: uncleanLetters }
      : { material, reports: corpus, unclean: new Set<string>() },
  );
  let transcript = $state<Transcript | null>(null);
  let at = $state(0);
  let refusal = $state<string | null>(null);

  const done = $derived(session !== null && at >= session.tasks.length);

  // A consent text with a gap in it is not a consent text. Refusing at
  // the gate is the only place the refusal can do any good: after this
  // button, somebody has already read the page.
  const unfinished = $derived(unsettledIn(text).size > 0);

  function start() {
    refusal = null;
    if (unfinished) {
      // Disabling the button is what a person sees; this is what makes
      // it true. A disabled attribute stops a click and nothing else,
      // and the one thing that must not happen here is a session opened
      // over a page with a hole in it.
      refusal = `this consent text is not finished — ${[...unsettledIn(text)].join(", ")} still to be written, so nobody can be asked to agree to it`;
      return;
    }
    try {
      transcript = begin(participant.trim(), chosen, stamp(text, now()));
      session =
        chosen.material === "letters"
          ? planLetters(participant.trim(), chosen.letters, chosen.unclean)
          : plan(participant.trim(), chosen.reports);
      at = 0;
    } catch (error) {
      // A refusal reaches the person running the session, in the words
      // the refusal used. A harness that swallowed one would start a
      // session it could not score and nobody would find out until the
      // analysis.
      refusal = error instanceof Error ? error.message : String(error);
    }
  }

  function answer(response: Response) {
    if (transcript === null) return;
    transcript = record(transcript, at, response);
    at += 1;
  }

  const file = $derived(
    transcript === null ? "" : JSON.stringify(transcript, null, 2),
  );
</script>

<main class="page">
{#if session === null || transcript === null}
  <section class="gate">
    <ConsentForm {text} />
    <div class="form">
      <div class="govuk-form-group">
        <label class="govuk-label" for="participant">Your participant number</label>
        <input
          class="govuk-input govuk-input--width-5"
          id="participant"
          type="text"
          bind:value={participant}
          placeholder="p01"
          readonly={issued !== null}
        />
      </div>
      <div class="govuk-form-group">
        <div class="govuk-checkboxes govuk-checkboxes--small">
          <div class="govuk-checkboxes__item">
            <input class="govuk-checkboxes__input" id="agreed" type="checkbox" bind:checked={agreed} />
            <label class="govuk-label govuk-checkboxes__label" for="agreed">
              I have read this and I am happy to take part
            </label>
          </div>
        </div>
      </div>
      {#if refusal}
        <p class="govuk-error-message" role="alert">{refusal}</p>
      {/if}
    </div>
    <Button
      label="Start"
      onclick={start}
      disabled={unfinished || !agreed || participant.trim() === ""}
    />
  </section>
{:else if done}
  <section class="gate">
    <h1>That's everything — thank you</h1>
    <p>
      Your answers are below. Hand this to whoever is running the session; it
      has not been sent anywhere.
    </p>
    <textarea readonly rows="16" aria-label="Your answers">{file}</textarea>
  </section>
{:else}
  <TaskScreen
    task={session.tasks[at]!}
    arm={session.arm}
    total={session.tasks.length}
    onanswer={answer}
    now={clock}
  />
{/if}
</main>

<style lang="scss">
  /* The harness owns its own page: `#study` is a bare div, and a card
     flush against the viewport edge is what that looks like. */
  .page {
    max-width: 56rem;
    margin: 0 auto;
    padding: k-spacing(l) k-spacing(m) k-spacing(2xl);
  }
  .gate {
    @include k-card;
    @include k-flow;
    padding: k-spacing(xl);
    max-width: var(--k-measure);
  }
  h1 {
    @include k-font(title);
  }
  .form {
    /* The document ends and the form begins; the flow's own step is
       for paragraphs of one thing. */
    --k-flow-space: #{k-spacing(l)};
  }
  textarea {
    width: 100%;
    font-family: var(--k-font-mono);
    @include k-font(tiny);
    border: var(--k-border-hair);
    padding: k-spacing(xs);
  }
</style>
