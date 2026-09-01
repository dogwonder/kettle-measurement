<script lang="ts">
  // #431, letter track: what Kettle found in one letter, rendered from
  // the product's own action cards.
  //
  // The letter pack's deliverable is a list of proposed actions — one
  // card per ask, its deadline as written and as worked out, its
  // passage one disclosure away. That is the surface the desktop app's
  // review screen shows, so unlike the statement track there is no gap
  // between the artefact here and the one a Kettle user reads: the
  // component is the same one.
  import ActionCard from "../src/lib/components/ActionCard.svelte";
  import ReportShell from "../src/lib/components/ReportShell.svelte";
  import type { StudyLetter } from "../src/lib/study-letter";

  let {
    letter,
    disclose = true,
    emphasis = null,
    onopen,
  }: {
    letter: StudyLetter;
    /** Show the passage behind each card at all. */
    disclose?: boolean;
    /** The action whose evidence is opened and marked, or null. */
    emphasis?: string | null;
    /** A person opened an action's evidence. */
    onopen?: (ask: string) => void;
  } = $props();

  const actions = $derived(letter.actions.actions);
</script>

<ReportShell
  kicker="Report · made on this computer · nothing left this machine"
  title="What {letter.source.file} asks of you"
  subtitle={actions.length === 1 ? "One thing to do" : `${actions.length} things to do`}
>
  {#if actions.length === 0}
    <p class="none">Kettle found nothing in this letter that asks you to do anything.</p>
  {/if}
  <div class="cards">
    {#each actions as action (action.id)}
      <ActionCard
        {action}
        decision="proposed"
        readonly
        {disclose}
        emphasised={emphasis === action.title}
        ontoggle={(open) => {
          if (open) onopen?.(action.title);
        }}
        onapprove={() => {}}
        ondismiss={() => {}}
        onreset={() => {}}
      />
    {/each}
  </div>
</ReportShell>

<style lang="scss">
  .cards {
    @include k-stack(m);
  }
  .none {
    max-width: var(--k-measure);
  }
</style>
