// #431: what a participant is asked to agree to.
//
// The harness already refused to open a session without a consent
// version, on the grounds that "they agreed" cannot answer "to what".
// But the version was a constant here and the words were paragraphs in
// a Svelte file, so editing the copy moved neither, and a transcript
// could name a document that said something else. This file is the
// document: the screen renders it and the transcript stamps it, from
// one object, so the two cannot drift.
//
// Two facts in it are not ours to write - who holds the files, and who
// a participant asks about them. Both are marked in the text and
// declared beside it with a reason and a date, the same staged
// exception this repo keeps for a govuk component it has not adopted.
// A blank would have been handed to somebody.

export interface ConsentSection {
  heading: string;
  paragraphs: string[];
  /** Render the list of recorded fields after these paragraphs. */
  records?: boolean;
  /**
   * Paragraphs that come after that list.
   *
   * Here rather than in the component because the digest covers this
   * file: a sentence living in the markup would be words on the page
   * that the version does not stand behind, which is the whole defect
   * this module was built to close.
   */
  after?: string[];
}

/** One field of the transcript, and the words a participant reads for it. */
export interface RecordedField {
  /** The key as it appears in the file. */
  key: string;
  plain: string;
}

/** A fact the text needs and only the study's runner can supply. */
export interface Unsettled {
  field: string;
  why: string;
  /** When it was first left open, so an old gap is visible as old. */
  since: string;
}

export interface ConsentText {
  /**
   * The day this wording came into force.
   *
   * A date rather than a number: what somebody agreed to is a document,
   * and whoever asks later what it said needs to be able to find it.
   * The digest is what makes the date honest — see `digestOf`.
   */
  version: string;
  title: string;
  sections: ConsentSection[];
  records: RecordedField[];
  unsettled: Unsettled[];
}

export const CONSENT: ConsentText = {
  // 27 August 2026, later the same day: the letter track and the
  // author's own pilot sessions named in the text, and two more keys
  // in the list of what is written down. Still no session opened under
  // the morning's words, so the version stands and the digest moves.
  //
  // 27 August 2026: the two gaps filled, and one promise withdrawn with
  // them. The 26 August wording offered deletion by participant number,
  // which the arrangement decided the next day cannot honour — there is
  // no list to look the number up in. A form may not promise a thing
  // its own design has made impossible, so the sentence is replaced
  // rather than softened. No session was opened under either earlier
  // text, so no transcript can confuse the three.
  // 28 August 2026 carries two digests, and the digest is what
  // distinguishes them: `307da060` fixed the `corpus` wording and
  // `bb057ccb` added `drawn`. Both changed the same day, before anybody
  // outside the study had sat it — the only two transcripts stamped
  // under `307da060` are the retired author pilots. A reader comparing
  // stamps compares digests, which is the whole reason the stamp
  // carries one.
  version: "2026-08-28",
  title: "Reading reports: what taking part involves",
  sections: [
    {
      heading: "What this is",
      paragraphs: [
        "Kettle reads a document — a bank statement, or a letter that asks something of you — and writes down what it found. This study is about what it wrote, not about you. We want to know whether showing you where each figure or date came from helps you notice when one is wrong.",
        "There are ten reports, all of one kind — ten statements or ten letters — and it takes about half an hour.",
      ],
    },
    {
      heading: "What you will be asked to do",
      paragraphs: [
        "Each report comes in two steps. First you see what Kettle found on its own, and we ask how much you believe it. Then the evidence arrives — where each figure or date came from — along with the statement or letter the report was written from, and we ask whether you accept the report and how confident you are now.",
        "Some of these reports contain a mistake and some do not, and we will not tell you which. That is the question the study is asking. Saying a report is wrong when it turns out to be right is a useful answer too, so please say what you actually think rather than what you think we are hoping for.",
      ],
    },
    {
      heading: "Everything here is invented",
      paragraphs: [
        "Every statement, every letter, every name, every amount and every date was made up for this study. No real person's records or post are involved, and nothing on your own computer is read.",
      ],
    },
    {
      heading: "What gets written down",
      paragraphs: ["Your answers are kept in one file, and this is all of it:"],
      records: true,
      after: [
        "Not your name, not your email address, and nothing else about you.",
      ],
    },
    {
      heading: "Where it goes",
      paragraphs: [
        "Nowhere by itself. Your answers stay in this browser while you work, appear on the last screen, and are handed to whoever is running the session. There is no server and this page uploads nothing. You can read the file before you hand it over.",
        "Because nothing is saved anywhere, closing or reloading this page loses your answers and the session would have to start again.",
      ],
    },
    {
      heading: "Nobody writes down which number is you",
      paragraphs: [
        "You will be given a participant number. No list is kept connecting that number to your name, so nobody can look you up in these files afterwards — and neither can we. That is the whole of the arrangement, rather than a promise to be careful with something we hold.",
        "If the person running the study sits a session themself, to try the instrument out, that file is marked as theirs and is never counted among the twenty.",
        "Your answers are held by DGW Ltd, the company that makes Kettle.",
      ],
    },
    {
      heading: "The answers are published",
      paragraphs: [
        "When the study is written up, all twenty files are published alongside what we conclude from them. That is deliberate: it means somebody who doubts a finding can re-read the answers it came from, rather than having to ask twenty more people for half an hour of their time.",
        "They are published as they stand — numbers, not names. Which is why the two boxes you can type into are worth a second's thought: please do not put anything about yourself in them.",
      ],
    },
    {
      heading: "Stopping, and changing your mind",
      paragraphs: [
        "You can stop at any point and you do not have to give a reason.",
        "Until you hand the file over, nothing of yours exists anywhere else. If you would rather it went no further, do not hand it over — that is the end of it, and no reason is needed.",
        "After you hand it over we cannot take it back. That is a consequence of not knowing who you are rather than a policy: with no list connecting your number to you, there is nothing to search for and so nothing to delete. It is worth deciding before you pass it on.",
      ],
    },
    {
      heading: "Questions",
      paragraphs: [
        "Ask anything you like during the session, including what is being measured — you will be told. The one thing you will not be told is which of the ten reports carry a mistake, because that is what is being measured.",
        "Afterwards, or if you would rather ask in writing: Rich Holman, dogwonder@gmail.com.",
      ],
    },
  ],
  // The keys of the transcript, in the words a participant reads. One
  // list, so the form and the file cannot describe different things;
  // `study-consent.test.ts` holds them to the same set.
  records: [
    {
      key: "schema",
      plain:
        "The name and version of the file's own format, so it can still be read years from now.",
    },
    { key: "participant", plain: "Your participant number." },
    {
      key: "role",
      plain:
        "Whether you were a participant, or the person running the study trying it out on themself.",
    },
    { key: "material", plain: "Whether you were shown statements or letters." },
    {
      key: "arm",
      plain: "Which of the two ways of showing a report you were given.",
    },
    {
      key: "consent",
      plain:
        "The date you agreed to this page, and which version of it you were shown.",
    },
    {
      key: "corpus",
      // It says the pool because the file holds the pool. The 27 August
      // wording said "which ten documents you were shown", which was
      // true of the statements — there are exactly ten — and false of
      // every letters session: the list is all eighteen letters the ten
      // were drawn from, so it names eight documents nobody saw. The
      // key-list mechanism could not catch it, because the key was
      // right and only its meaning was wrong.
      plain:
        "The full set of documents your ten were drawn from, which is how your session can be rebuilt from your participant number.",
    },
    {
      key: "drawn",
      plain: "Which ten documents you were shown, in the order you saw them.",
    },
    {
      key: "responses",
      plain:
        "What you said about each report, how confident you were before and after checking it, how long you spent on it, and which pieces of evidence you opened.",
    },
  ],
  // Both gaps filled on 27 August 2026. The mechanism stays: a new
  // `[UNSETTLED: field]` marker is declared here with its reason and
  // date, and `study-consent.test.ts` then fails on the finished
  // ratchet until somebody writes the words — because people are being
  // asked to agree to this now, declaring a hole is no longer enough.
  unsettled: [],
};

/** What a transcript carries about the agreement. */
export interface ConsentStamp {
  /** Which text: the date it came into force. */
  version: string;
  /** Which words: `digestOf` the text that was rendered. */
  digest: string;
  given_at: string;
}

const MARKER = /\[UNSETTLED: ([a-z-]+)\]/g;

/** The gaps actually marked in the text, whatever has been declared. */
export function unsettledIn(text: ConsentText): Set<string> {
  const found = new Set<string>();
  for (const line of words(text)) {
    for (const match of line.matchAll(MARKER)) found.add(match[1]!);
  }
  return found;
}

/**
 * A short digest of every word in the text.
 *
 * FNV-1a, matching the participant-id hash in `study-session.ts`: this
 * detects drift between the version and the words, it does not resist
 * anybody. What it buys is that two participants a fortnight apart
 * cannot come out of the analysis looking like they read the same page
 * when they did not.
 */
export function digestOf(text: ConsentText): string {
  let value = 0x811c9dc5;
  const joined = [text.version, ...words(text)].join(" ");
  for (let i = 0; i < joined.length; i += 1) {
    value ^= joined.charCodeAt(i);
    value = Math.imul(value, 0x01000193);
  }
  return (value >>> 0).toString(16).padStart(8, "0");
}

/** The agreement, as the transcript records it. */
export function stamp(text: ConsentText, given_at: string): ConsentStamp {
  return { version: text.version, digest: digestOf(text), given_at };
}

/** Every word on the page, in the order it is rendered. */
function words(text: ConsentText): string[] {
  const lines = [text.title];
  for (const section of text.sections) {
    lines.push(section.heading, ...section.paragraphs);
    if (section.records) {
      for (const field of text.records) lines.push(field.key, field.plain);
    }
    lines.push(...(section.after ?? []));
  }
  return lines;
}
