<script lang="ts">
  // #431: the statement the report was made from.
  //
  // Part of the task, not a convenience. An invention and a
  // mis-relation can both be checked inside the report — the wrong
  // amount disagrees with its own transaction chips, the wrong period
  // disagrees with the median interval printed beside it. An omission
  // cannot: there is no row, no evidence and no claim, so the only way
  // to notice one is to read the statement and find something the
  // report never mentioned.
  //
  // Without this, omission detection would be zero by construction, and
  // a rate of zero produced by the harness looks exactly like a
  // finding about people.
  import DataTable from "../src/lib/components/DataTable.svelte";
  import Details from "../src/lib/components/Details.svelte";
  import { rowsOf } from "./corpus";

  let { csv, file }: { csv: string; file: string } = $props();

  const rows = $derived(rowsOf(csv));
</script>

<Details showLabel="Open the statement" hideLabel="Close the statement">
  <p class="what">Every line of <strong>{file}</strong>, as it arrived.</p>
  <div class="scroll">
    <DataTable
      columns={[
        { label: "Date" },
        { label: "Description" },
        { label: "Amount", numeric: true },
      ]}
      rows={rows.map((row) => [row.date, row.description, row.amount])}
    />
  </div>
</Details>

<style lang="scss">
  .what {
    margin: 0 0 k-spacing(xs);
    @include k-font(small);
  }
  /* A year of transactions is longer than any screen; the table scrolls
     inside its own box rather than the page scrolling sideways. */
  .scroll {
    max-height: 60vh;
    overflow: auto;
  }
</style>
