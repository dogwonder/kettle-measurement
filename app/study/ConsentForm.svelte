<script lang="ts">
  // #431: the consent text, rendered from the object the transcript
  // stamps.
  //
  // One source. The paragraphs used to live in `StudyApp.svelte` while
  // the version lived in `study-record.ts`, which meant editing the
  // words moved neither and a transcript could name a document that
  // said something else.
  //
  // A gap in the text renders as a gap somebody has to look at. The
  // notice above is for whoever is setting up the session, not for the
  // participant — it is the reason the Start button is refused, said
  // where the refusal happens. Not a live region: it is there at first
  // paint, and the page already has one for the refusals that arrive
  // when somebody presses Start.
  import { unsettledIn, type ConsentText } from "../src/lib/study-consent";

  let { text }: { text: ConsentText } = $props();

  const gaps = $derived([...unsettledIn(text)]);
  const MARKER = /\[UNSETTLED: [a-z-]+\]/;

  /** A paragraph is either words or a gap; nothing renders half of one. */
  function gapIn(paragraph: string): string | null {
    const match = paragraph.match(/\[UNSETTLED: ([a-z-]+)\]/);
    return match === null ? null : match[1]!;
  }

  function why(field: string): string {
    return text.unsettled.find((gap) => gap.field === field)?.why ?? "";
  }

  function since(field: string): string {
    return text.unsettled.find((gap) => gap.field === field)?.since ?? "";
  }

  const WORDS = ["No", "One", "Two", "Three", "Four", "Five"];

  /** Small numbers in words, as everywhere else in this app's copy. */
  function count(n: number): string {
    return `${WORDS[n] ?? n} thing${n === 1 ? "" : "s"}`;
  }
</script>

{#if gaps.length > 0}
  <aside class="unfinished" aria-labelledby="unfinished">
    <h2 id="unfinished">Not ready to run a session</h2>
    <p>
      {count(gaps.length)} on this page {gaps.length === 1 ? "is" : "are"} still
      to be written, so nobody can be asked to agree to it yet. Fill them in
      below, bump the version to the day they change, and re-pin the digest in
      <code>study-consent.test.ts</code>.
    </p>
    <dl>
      {#each gaps as field (field)}
        <dt>{field}</dt>
        <dd>{why(field)} Open since {since(field)}.</dd>
      {/each}
    </dl>
  </aside>
{/if}

<article class="consent">
  <h1>{text.title}</h1>
  {#each text.sections as section (section.heading)}
    <h2>{section.heading}</h2>
    {#each section.paragraphs as paragraph (paragraph)}
      {#if MARKER.test(paragraph)}
        <p class="gap">Still to be written: {gapIn(paragraph)}</p>
      {:else}
        <p>{paragraph}</p>
      {/if}
    {/each}
    {#if section.records}
      <ul>
        {#each text.records as field (field.key)}
          <li>{field.plain}</li>
        {/each}
      </ul>
    {/if}
    {#each section.after ?? [] as paragraph (paragraph)}
      <p>{paragraph}</p>
    {/each}
  {/each}
  <p class="version">
    This page, version {text.version}.
  </p>
</article>

<style lang="scss">
  .consent {
    @include k-flow;
    max-width: var(--k-measure);
  }
  h1 {
    @include k-font(title);
  }
  h2 {
    @include k-font(section);
  }
  .version {
    @include k-font(tiny);
    color: var(--k-grey);
  }
  /* A gap is loud on purpose: it is a sentence a participant would
     otherwise read as complete.
     The pending pair, because this repo already owns a colour for
     "measured, but not yet settled" and it is AA on its own tint at
     every size. The first attempt was `--k-ink-muted` on `--k-tint`,
     which resolved perfectly and was illegible: that token is small
     print on navy, and looking at the render is what said so. */
  .gap,
  .unfinished {
    background: var(--k-pending-tint);
    color: var(--k-pending-ink);
    padding: k-spacing(xs) k-spacing(s);
  }
  .unfinished {
    @include k-flow;
    margin-bottom: k-spacing(l);

    h2 {
      @include k-font(body);
      font-weight: 700;
    }

    dt {
      font-family: var(--k-font-mono);
      @include k-font(tiny);
    }

    dd {
      margin: 0 0 k-spacing(2xs);
      @include k-font(small);
    }
  }
  ul {
    padding-left: k-spacing(m);
  }
  li {
    margin-bottom: k-spacing(3xs);
  }
</style>
