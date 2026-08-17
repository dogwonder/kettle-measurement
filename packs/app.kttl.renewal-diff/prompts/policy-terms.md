You read one passage of an insurance document at a time and say which
named values, if any, it states.

For each passage, decide:
- terms: a list of the named values this passage states. Most passages
  state none — an empty list is a correct answer, not a failure, and it
  is always better than a guess. A passage that explains what cover
  means, describes how to claim, or thanks somebody for renewing states
  no named value.
- confidence: "high" | "medium" | "low"

For each term give:
- term: "premium" | "compulsory_excess" | "voluntary_excess" |
  "total_excess" | "cover_limit" | "no_claims_discount" |
  "cooling_off_period" | "other"

  Documents rarely use these words. "Cover limit" is the most a policy
  will pay out, and a schedule may label it "Insurance amount", "Sum
  insured", "Limit of indemnity" or "Maximum we will pay" — those are
  all "cover_limit".

  Use "other" when the passage states a named value that is none of
  these. That is a real answer and the right one: the value goes in
  front of the person instead of being compared. Never stretch one of
  the named terms to cover something it is not.

- basis: "annual" | "monthly" | "per_claim" | "per_policy"

  What the value is measured against, as the passage says it. A premium
  quoted as twelve instalments is "monthly"; the same policy's total for
  the year is "annual". These are different values, and calling one the
  other turns an instalment into a price rise.

- value: the value exactly as the document writes it — "£1,234.56",
  "£500", "14 days", "60%". Copy the currency symbol, the commas and
  the decimals as they appear. Never round, never convert, never work
  out a total, and never write a number the passage does not contain.

- quote: the words the value comes from, copied exactly from the
  passage — and the quote must contain the value itself. "Insurance
  amount" is a label, not a quote: on a schedule with three sections it
  is true of three different amounts, so it shows nothing about which
  one you read. "Insurance amount: £604,400.00." does show it. Include
  enough words on either side of the value to make clear which line it
  came from. This is checked against the document: a quote that is not
  in the passage, or that does not contain its own value, means the
  whole term goes to the person instead of being used.

You are not asked to compare anything, to say whether a value went up
or down, or to work out a difference. That is done separately, from the
values you copy. Copying them exactly is the whole job.

Return one result per input, with the same "id" you were given, and
echo the passage back in "segment". Return JSON only.

Passages:
{{ batch_json }}
