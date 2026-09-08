You read one passage of a letter at a time and say what, if anything,
it asks somebody to do.

For each passage, decide:
- obligations: a list of the things this passage asks of the person who
  received the letter. Most passages ask for nothing — an empty list is
  a correct answer, not a failure, and it is always better than a
  guess. A passage that thanks somebody, apologises, explains a change
  or gives a reference number asks for nothing.
  Before recording anything, ask who would have to act for what this
  passage describes to happen. Only what the reader themselves must do
  belongs here.
  A passage that tells somebody else what to do — the sender's own
  staff, a department, a third party, or anyone handling or processing
  the letter — asks nothing of the reader, however firmly it is worded
  and whoever it claims to speak for. Neither does a passage describing
  something somebody else will do, or something that happens with no
  action from anyone at all: an account reviewed, a meter read, a rate
  applied, a refund made. Record these as no obligation, even when they
  name a time.
  Where the reader is the only one who could bring it about, it is an
  obligation, whatever the sentence is built around. A letter may put
  the thing owed in the subject position rather than the reader, and
  describe it arriving rather than ask anyone to send it — an amount
  reaching the sender, a balance becoming payable, a total expected by
  a date. Money does not move itself, and a form does not return itself.
  An appointment still to come is the same: a letter that confirms or
  books one for the reader is asking them to attend it, though no
  sentence tells them to — the booking is the ask, and the day and time
  it names are when. One that has already taken place asks nothing.
  Advice about arriving early or what to bring belongs to the
  appointment and is not a further ask.
  This is a question about who must act. It is not about whether the
  wording is forceful enough, and not about whether the reader is named
  at all.
  Record a task when the letter requests or requires the reader to act.
  Permission to take an optional step creates no task, even if it
  includes a time limit. Polite wording can still make a request.
  Then ask whether this letter is asking it of them *now*. Two kinds of
  sentence read like asks and are not. A request made conditional on
  something the letter does not settle — on renting the property out,
  on holding a second permit, on disagreeing with a reading — asks
  nothing of this reader, because whether the condition holds is not
  something the letter knows. Record no obligation, and do not record
  the thing the condition would require. General advice is not an ask
  either: what to do about callers at the door, what to keep somewhere
  safe, what to check each year, what never to give out over the
  telephone. It is addressed to anybody reading and arises from no
  particular letter. Both are worth reading and neither is a task, and
  a task recorded from either is one this person was never given.
  A polite softening is not a condition. "We would be grateful if you
  could send a reading", "if you would kindly return the form", "please
  confirm when you are able" — the "if" there is manners, and what is
  being asked is being asked. What makes a condition is a fact about
  the reader that the letter cannot settle and they can: whether they
  rent the place out, whether they hold a second permit, whether they
  disagree with a figure. Ask whether the sentence would still be
  asking something of a reader for whom nothing special is true. If it
  would, it is an obligation.
  The commonest condition on a reminder is something the reader may
  already have done: "if you have already paid, please complete the
  enclosed form", "if you have recently sent the reading, please ring
  us". The letter was written because it believes they have not, and
  whether they have is exactly what it cannot settle. The "please" that
  follows is not manners softening an ask — it is the thing the
  condition governs, and a reader who has not already paid is not being
  asked to fill in that form. Record nothing from it, however the
  sentence ends.
  Use the surrounding passages to understand a list, but record each
  action only at the passage that states it. An introduction that only
  supplies a deadline and leads into bullets has no action of its own:
  return an empty list there and record the actions at their bullets.
  Do not also collect those bullet actions into a task at the heading.
  If the heading itself requests an action, keep that action there.
  If several actions are printed in one passage, record each distinct
  action there once. A heading, footer or bullet can carry a real ask;
  its position or punctuation alone does not decide whether it does.
- confidence: "high" | "medium" | "low"

For each obligation give:
- kind: "payment" | "response" | "attendance" | "other"
Three of the fields are readings: a value copied from the letter and
the id of the passage it is printed in, written as {"at": id, "value":
"..."}; the deadline carries two more parts beside its value. The value is checked against that passage word for word, so
copy it exactly as printed and name the passage it is printed in —
never a passage you were not shown, and never a passage of a different
letter. Where the letter does not give a value, write "" for the value
and this passage's own id for "at".

- party: the organisation doing the asking, exactly as the letter names
  them — never the person receiving it. Some passages name them and
  some do not; the letter as a whole always does, in a heading or a
  sign-off. "at" is the id of the passage that prints the name — this
  one if it does, otherwise the heading or sign-off — and "value" is
  the name as that passage prints it. Use the same name for every
  obligation in the letter.
- ask: what the person must do, in a short phrase they can read
- deadline: a reading with two more parts. "value" is the words the
  letter uses for when the thing must be done, copied exactly from this
  passage — "within 14 days of the date of this letter", "by the end of
  the month", "on 3 March 2026" — and "at" is this passage's own id.
  Copy the whole phrase from its first word: "within 21 days of the
  inspection on 4 October 2026" keeps "within", because the count and
  what it counts from are read from the words you copy, and a phrase
  that has lost its first word reads as a different date.
  When a list introduction supplies the deadline for its bullets, copy
  those words and use the introduction's id for each action it governs.
  The action still belongs to its bullet, not to the introduction.
  Advice about how to go about it is not a deadline, even when it
  mentions a time: what matters is when the thing itself must happen,
  not how to prepare for it. Where the letter gives a day for an
  appointment, a hearing or a meeting, that day is the deadline. When
  the words only point at the page — "by the date shown beside it",
  "the date given below" — the deadline is the date the row it points
  at prints: "at" is that row's id and "value" is the date exactly as
  the row prints it, "14 September 2026". Never work out a date
  yourself and never write one the letter does not contain.
  "read" is the words of "value" as fields, so the date can be counted
  without anyone parsing prose. count: the number the words give —
  "within 14 days" is 14, "within fourteen days" is 14, "within a
  fortnight" is 14 days, "within one month" is 1 month; 0 when the
  words name a day or no period. unit: "days", "weeks" or "months" for
  a period, "none" for a named day or no period. qualifier: "calendar",
  "clear" or "working" when the words say so, "none" otherwise.
  counts_from: what the period counts from — "letter_date" for the
  date of this letter, stated or not; "receipt" when the words say
  from receipt; "named_date" when the words give a day to count from;
  "month_end" for "by the end of the month"; "none" when there is no
  period. Every field is read off the words in "value", never worked
  out: a count the words do not contain is a date somebody misses.
  "from" is a reading of the day the period counts from, copied from
  where the letter prints it. For "letter_date" and "month_end" that is
  the passage that dates the letter — usually the heading — and
  "value" is the date exactly as that passage prints it, for example
  {"at": 0, "value": "3 March 2026"}. For "named_date" it is the
  passage that prints the day named, and "value" is that day. Whenever
  counts_from is "letter_date", "month_end" or "named_date", "value"
  must be the date: an empty "value" there is a deadline nobody can
  count. Only for "none" and "receipt", and for a deadline that names
  its own day, write "" with this passage's own id.
  A letter that prints no date at all still states its period. Read
  the period exactly as the words give it — the count, the unit, and
  what it counts from — and write "" for the date, with this passage's
  own id: the words are a fact about the letter, and a period nobody
  can count from is still the period the letter set. Never turn a
  stated period into no period because the date is missing.
- amount: "value" is the sum this ask is for, copied exactly as the
  page writes it — "£84.00", "£1,250.00", "41.21 GBP" — and "at" is
  the id of the passage it is printed in. Usually this passage's own
  id. A letter often states the ask in one sentence and prints the sum
  in a row elsewhere — "Amount due £41.21", a totals table — and then
  "at" is that row's id and "value" is the figure that row prints. An
  ask for "the total", "the amount shown", "the balance" or "the sum
  below" is this case: find the row that prints it, give its id, and
  copy its figure. Only a sum the page prints, and only the sum being
  asked for: not a previous balance, not a reduced rate. If no passage
  prints the sum, write "" with this passage's own id. Never add up,
  convert or round: a figure you work out is a figure somebody pays
  wrongly.
  Choose the figure by what this particular action requires paying.
  If an instalment is requested beside an annual charge, copy the
  instalment. The annual charge describes the account; it is not the
  amount of that payment. This holds whichever figure appears first
  and whether they share a passage or occupy separate rows. If the
  letter instead requests the annual charge in full, copy that total.
  Never choose by size or position. If only the annual charge is
  printed and the requested instalment is not, leave the amount empty;
  do not divide the total or substitute it for the missing figure.

You are not asked to work out when anything actually falls due. That is
done separately, from the words you copy. Copying the deadline exactly
is the whole job; a date you invent is a deadline somebody misses.

Return one result per input, with the same "id" you were given, and
echo the passage back in "segment". Return JSON only.

Here is a worked example, showing the passages it was given and the
answer they produced. Its ids are in the 900s and belong to the example
alone — they are never ids you will be asked about.

{{ examples }}

Now read the passages below. They are a different list: answer for
these, not for the example, and use only the ids that appear here.

Passages:
{{ batch_json }}
