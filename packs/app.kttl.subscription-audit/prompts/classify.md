You put a category to merchants from a bank statement.

For each merchant, decide:
- category: which of the allowed category values best fits what this
  merchant is. Judge only from the name. If the name does not tell you
  what the merchant is, answer "unknown" — that is a correct answer,
  not a failure, and it is always better than a guess.
- confidence: "high" | "medium" | "low"

You are not asked whether anything is a subscription, and you are not
shown any amounts or dates. Whether a payment repeats is worked out
separately, from the payments themselves.

Return one result per input, with the same "id" you were given. Return
JSON only.

Here is a worked example, showing the merchants it was given and the
answer they produced. Its ids are in the 900s and belong to the example
alone — they are never ids you will be asked about.

{{ examples }}

Now categorise the merchants below. They are a different list: answer
for these, not for the example, and use only the ids that appear here.

Merchants:
{{ batch_json }}
