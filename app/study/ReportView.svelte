<script lang="ts">
  // #431: one report, rendered from the product's own components.
  //
  // Not the runner's `report.html`. Three of the issue's four conditions
  // are the same report shown differently — without immediately visible
  // evidence, with it, and with it pointed at — and a rendered document
  // can only be one of those. A re-renderable report is what the
  // conditions require, so the study assembles the app's component
  // inventory rather than framing a finished HTML file.
  //
  // The gap that leaves is real and named: today the desktop app shows
  // `report.html` in an iframe, so this assembly is not yet the
  // artefact a Kettle user reads. `study.test.ts` holds the two to the
  // same figures for the same run, which bounds the gap to presentation
  // rather than to content — and the gap closes for good when the app's
  // own report screen is built on these components.
  import CheckYourselfList from "../src/lib/components/CheckYourselfList.svelte";
  import DataTable from "../src/lib/components/DataTable.svelte";
  import EvidenceRow from "../src/lib/components/EvidenceRow.svelte";
  import ReportShell from "../src/lib/components/ReportShell.svelte";
  import SummaryCard from "../src/lib/components/SummaryCard.svelte";
  import { formatAmount, shortDate } from "../src/lib/format";
  import type { RunReport } from "../src/lib/types";

  let {
    report,
    disclose = true,
    emphasis = null,
    onopen,
  }: {
    report: RunReport;
    /** Show the evidence behind the figures at all. */
    disclose?: boolean;
    /** The claim whose evidence is opened and marked, or null. */
    emphasis?: string | null;
    /** A person opened a claim's evidence. */
    onopen?: (merchant: string) => void;
  } = $props();

  const period = $derived(
    `${shortDate(report.run.input.period.from)} to ${shortDate(report.run.input.period.to)}`,
  );
</script>

<ReportShell
  kicker="Report · made on this computer · nothing left this machine"
  title="Subscriptions and regular spending in {report.run.input.file}"
  subtitle="{report.run.input.rows} payments, {period}"
>
  <SummaryCard title="What this adds up to">
    <ul class="totals">
      <li>
        <strong>{formatAmount(report.summary.annualised_total)}</strong> a year on
        recurring payments
      </li>
      <li>
        <strong>{formatAmount(report.summary.monthly_equivalent)}</strong> a month,
        on average
      </li>
      <li>
        <strong>{report.summary.recurring_count}</strong>
        recurring {report.summary.recurring_count === 1 ? "payment" : "payments"}
      </li>
      <li>
        <strong>{report.summary.price_rises}</strong>
        price {report.summary.price_rises === 1 ? "rise" : "rises"} found
      </li>
    </ul>
    <p>{report.summary.note}</p>
  </SummaryCard>

  <h2>Recurring payments — {formatAmount(report.summary.annualised_total)} a year</h2>
  <div class="head" aria-hidden="true">
    <span>Merchant</span><span>Every</span><span>Costs</span><span>A year</span>
    {#if disclose}<span></span>{/if}
  </div>
  {#each report.recurring as finding (finding.merchant)}
    <EvidenceRow
      {finding}
      {disclose}
      emphasised={disclose && emphasis === finding.merchant}
      ontoggle={(open) => {
        if (open) onopen?.(finding.merchant);
      }}
    />
  {/each}

  {#if report.regular_spend.length > 0}
    <h2>Everything else Kettle found</h2>
    <DataTable
      caption="Payments Kettle recognised but did not judge to be a subscription."
      columns={[
        { label: "Merchant" },
        { label: "Visits", numeric: true },
        { label: "Typical visit", numeric: true },
        { label: "Total", numeric: true },
      ]}
      rows={report.regular_spend.map((spend) => [
        spend.merchant,
        String(spend.visits),
        formatAmount(spend.typical_visit),
        formatAmount(spend.total),
      ])}
    />
  {/if}

  {#if report.income.length > 0}
    <h2>Money coming in</h2>
    <DataTable
      columns={[{ label: "From" }, { label: "Every" }, { label: "Amount", numeric: true }]}
      rows={report.income.map((line) => [
        line.merchant,
        line.period,
        formatAmount(line.amount),
      ])}
    />
  {/if}

  {#if report.needs_review.length > 0}
    <h2>Needs your review</h2>
    <ul class="review">
      {#each report.needs_review as item (item.raw_merchant)}
        <li><strong>{item.raw_merchant}</strong> — {item.reason} {item.note}</li>
      {/each}
    </ul>
  {/if}

  {#if report.check_yourself.length > 0}
    <h2>Check these yourself</h2>
    <CheckYourselfList items={report.check_yourself} />
  {/if}
</ReportShell>

<style lang="scss">
  h2 {
    @include k-font(section);
    margin: k-spacing(m) 0 k-spacing(2xs);
  }
  .totals {
    list-style: none;
    padding: 0;
    margin: 0 0 k-spacing(xs);
    display: grid;
    gap: k-spacing(3xs);
  }
  .head {
    display: grid;
    grid-template-columns: 2.1fr 0.9fr 0.9fr 0.9fr 0.9fr;
    gap: k-spacing(xs);
    padding-bottom: k-spacing(3xs);
    border-bottom: var(--k-border-hair);
    @include k-font(tiny, $weight: 700);
    color: var(--k-grey);

    span:not(:first-child) {
      text-align: right;
    }
  }
  .review {
    padding-left: k-spacing(s);
    @include k-font(small);
  }
</style>
