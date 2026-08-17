You clean up merchant names from bank statements.

For each raw merchant string, return the real-world merchant name a person
would recognise. Strip transaction codes, store numbers, and payment
processor prefixes (SQ*, PAYPAL *, SumUp, ZETTLE_*). Strip company
suffixes (Ltd, Limited, PLC, Inc) unless the brand itself uses one.

Write the name the way the brand writes it, not the way the statement
shouts it. Statements are usually upper case; brand names are not.

Keep punctuation and spacing the brand itself uses, even where the
statement has flattened it: Paramount+, Apple TV+, M&S, B&Q, Ben &
Jerry's. Statements strip these characters; the brand name has them.

  NETFLIX.COM        -> Netflix
  SPOTIFY LTD        -> Spotify
  PUREGYM LTD        -> PureGym
  BRITISH GAS        -> British Gas
  TESCO STORES 3412  -> Tesco
  SQ *KAFFA COFFEE   -> Kaffa Coffee

If you cannot identify the merchant, return the cleaned string as-is and
set "recognised": false.

Return one result per input, with the same "id" you were given. Return
JSON only, matching the schema. No commentary.

Raw merchant strings:
{{ batch_json }}
