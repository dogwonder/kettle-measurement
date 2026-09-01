<script lang="ts">
  // #431: the letter the actions were read from.
  //
  // Part of the task for the reason the statement is: a dropped ask has
  // no card, no passage and no claim, so the only way to notice it is
  // to read the letter and find something the report never mentioned.
  import Details from "../src/lib/components/Details.svelte";

  let { text, file }: { text: string; file: string } = $props();

  const paragraphs = $derived(
    text
      .split(/\n\s*\n/)
      .map((block) => block.trim())
      .filter((block) => block.length > 0),
  );
</script>

<Details showLabel="Open the letter" hideLabel="Close the letter">
  <p class="what">Every word of <strong>{file}</strong>, as it arrived.</p>
  <div class="letter">
    {#each paragraphs as paragraph, index (index)}
      <p>{paragraph}</p>
    {/each}
  </div>
</Details>

<style lang="scss">
  .what {
    margin: 0 0 k-spacing(xs);
    @include k-font(small);
  }
  .letter {
    max-height: 60vh;
    overflow: auto;
    padding: k-spacing(s);
    background: var(--k-ground);
    border: var(--k-border-hair);

    p {
      max-width: var(--k-measure);
      /* A letter's line breaks are part of the letter. */
      white-space: pre-wrap;
      margin: 0 0 k-spacing(xs);
    }
  }
</style>
