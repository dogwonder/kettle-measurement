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
- confidence: "high" | "medium" | "low"

For each obligation give:
- kind: "payment" | "response" | "attendance" | "other"
Three of the fields are readings: a value copied from the letter and
the id of the passage it is printed in, written as {"at": id, "value":
"..."}. The value is checked against that passage word for word, so
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
- deadline: "value" is the words the letter uses for when the thing
  must be done, copied exactly from this passage — "within 14 days",
  "by the end of the month", "on 3 March 2026". Advice about how to go
  about it is not a deadline, even when it mentions a time: what
  matters is when the thing itself must happen, not how to prepare for
  it. Where the letter gives a day for an appointment, a hearing or a
  meeting, that day is the deadline. Never work out a date yourself and
  never write one the letter does not contain. "at" is the id of the
  passage the date is printed in: this passage's own id when the
  deadline is written here, whether as a day or as a period. When the
  words point elsewhere — "by the date shown beside it", "the date
  given below" — "at" is the id of the passage that prints that date,
  usually a due-date row. You are naming where the date is, not reading
  it: "value" stays the words this passage uses.
- anchor: what the deadline counts from, in the letter's own words —
  "the date of this letter", or a date the letter states. If nothing is
  given, write "no particular date".
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
