// #431: what a transcript must be before it is published.
//
// The study's answers are published alongside its findings, so a
// stranger can re-read the answers a result came from rather than
// having to gather twenty more. That is the provenance half. The other
// half is that nobody can be identified from what is published, and the
// arrangement that makes it true — no list connecting a participant
// number to a person — is exactly the arrangement that means nobody can
// fix a file after the fact either. So the check happens before the
// commit, once, and there is no second chance at it.
//
// This is a **floor**, in the sense `scripts/check-boundary.sh` is one
// in the recordings archive: it reads shape, never meaning. A sentence
// in a free-text box that names somebody's employer is a perfectly
// well-formed string. What the floor can do is refuse the shapes that
// should never occur, and make the one thing it cannot check — a person
// reading the forty free-text answers — a recorded step rather than a
// good intention. `transcripts/READ.json` is where that is recorded,
// and `study-transcripts.test.ts` refuses a transcript nobody has
// signed for.

import { CONSENT, digestOf } from "./study-consent";
import { ABSENT } from "./study-response";

/** The longest free-text answer that reads at a glance. */
const FREE_TEXT_LIMIT = 300;

const ARMS = ["evidence", "emphasised"];
const ROLES = ["participant", "author-pilot"];
const MATERIALS = ["statements", "letters"];
const VERDICTS = ["accept", "reject"];

/**
 * Everything wrong with one parsed transcript, in the words a person
 * fixing it needs.
 *
 * A list rather than a boolean, and never a throw: twenty files are
 * checked in one go before a commit, and "the first one is bad" is a
 * worse answer than all of them at once.
 */
export function faults(value: unknown): string[] {
  const found: string[] = [];
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    return ["not an object"];
  }
  const transcript = value as Record<string, unknown>;

  // The consent form lists what the file holds. A file holding
  // something else is, quite literally, not what anybody agreed to.
  const expected = CONSENT.records.map((field) => field.key).sort();
  const actual = Object.keys(transcript).sort();
  if (actual.join(",") !== expected.join(",")) {
    found.push(
      `holds [${actual.join(", ")}], and the consent form says [${expected.join(", ")}]`,
    );
    return found;
  }

  if (transcript.schema !== "kettle/study-transcript@0") {
    found.push(`schema is ${JSON.stringify(transcript.schema)}`);
  }
  // `p01` is a participant; `a01` is the study's author sitting the
  // instrument themself, which the file must say rather than anybody
  // remember.
  if (typeof transcript.participant !== "string" || !/^[pa]\d{2}$/.test(transcript.participant)) {
    found.push(
      `participant is ${JSON.stringify(transcript.participant)}, not a number of the form p01 (or a01 for the author's own pilot)`,
    );
  }
  if (typeof transcript.role !== "string" || !ROLES.includes(transcript.role)) {
    found.push(`role is ${JSON.stringify(transcript.role)}`);
  } else if (
    typeof transcript.participant === "string" &&
    (transcript.role === "author-pilot") !== /^a\d{2}$/.test(transcript.participant)
  ) {
    found.push(
      `role ${transcript.role} disagrees with participant number ${transcript.participant}`,
    );
  }
  if (typeof transcript.material !== "string" || !MATERIALS.includes(transcript.material)) {
    found.push(`material is ${JSON.stringify(transcript.material)}`);
  }
  if (typeof transcript.arm !== "string" || !ARMS.includes(transcript.arm)) {
    found.push(`arm is ${JSON.stringify(transcript.arm)}`);
  }

  found.push(...consentFaults(transcript.consent));

  // The pool the session was drawn from, not the ten drawn: `rebuild`
  // compares this list against the corpus on disk and refuses one that
  // is not what the participant saw. Ten reports made "exactly ten"
  // look like the rule until the letters arrived eighteen at a time.
  if (!Array.isArray(transcript.corpus) || transcript.corpus.length < 10) {
    found.push("corpus is fewer than the ten ids a session is drawn from");
  } else if (transcript.corpus.some((id) => typeof id !== "string" || id === "")) {
    found.push("corpus holds something that is not a report id");
  }

  if (!Array.isArray(transcript.responses) || transcript.responses.length !== 10) {
    found.push("responses is not ten slots");
  } else {
    transcript.responses.forEach((response, index) => {
      found.push(...responseFaults(response, index));
    });
  }

  return found;
}

function consentFaults(value: unknown): string[] {
  if (typeof value !== "object" || value === null) return ["consent is missing"];
  const consent = value as Record<string, unknown>;
  const found: string[] = [];
  for (const field of ["version", "digest", "given_at"]) {
    if (typeof consent[field] !== "string" || (consent[field] as string).trim() === "") {
      found.push(`consent.${field} is empty`);
    }
  }
  // A transcript naming today's version must carry today's words. This
  // is the pin doing its second job: the first catches an edit before
  // anybody reads the page, this catches a file that claims a version
  // it was not produced under.
  if (consent.version === CONSENT.version && consent.digest !== digestOf(CONSENT)) {
    found.push(
      `consent names version ${CONSENT.version} with digest ${consent.digest}, and that version's words digest to ${digestOf(CONSENT)}`,
    );
  }
  return found;
}

function responseFaults(value: unknown, index: number): string[] {
  const at = `response ${index + 1}`;
  if (value === null) return [];
  if (typeof value !== "object") return [`${at} is not an answer`];
  const response = value as Record<string, unknown>;
  const found: string[] = [];

  if (typeof response.verdict !== "string" || !VERDICTS.includes(response.verdict)) {
    found.push(`${at} verdict is ${JSON.stringify(response.verdict)}`);
  }
  for (const field of ["confidence_before", "confidence_after"]) {
    const point = response[field];
    if (typeof point !== "number" || !Number.isInteger(point) || point < 1 || point > 5) {
      found.push(`${at} ${field} is ${JSON.stringify(point)}`);
    }
  }
  if (typeof response.elapsed_ms !== "number" || response.elapsed_ms < 0) {
    found.push(`${at} elapsed_ms is ${JSON.stringify(response.elapsed_ms)}`);
  }
  if (!Array.isArray(response.opened) || response.opened.some((id) => typeof id !== "string")) {
    found.push(`${at} opened is not a list of claims`);
  }

  for (const field of ["flagged", "offered"]) {
    const claims = response[field];
    if (!Array.isArray(claims) || claims.some((claim) => typeof claim !== "string")) {
      found.push(`${at} ${field} is not a list of claims`);
    }
  }
  if (Array.isArray(response.flagged) && Array.isArray(response.offered)) {
    // Every claim ticked was on the page, bar the one that says nothing
    // on the page carries it.
    const offered = response.offered as string[];
    const stray = (response.flagged as string[]).filter(
      (claim) => claim !== ABSENT && !offered.includes(claim),
    );
    if (stray.length > 0) {
      found.push(`${at} ticked ${JSON.stringify(stray)}, which the report did not offer`);
    }
  }
  if (response.correction !== null && typeof response.correction !== "string") {
    found.push(`${at} correction is ${JSON.stringify(response.correction)}`);
  }

  return found;
}

/**
 * The answers a person should read closely before publication, and why.
 *
 * Length was a **refusal** and should never have been: a participant
 * who explains themselves at length has done nothing wrong, and a file
 * may not be edited after they hand it over — so the rule as built made
 * a good answer unpublishable. Its own description said the long one
 * "deserves a person's eye", which is a routing decision and not a
 * validity one, and every file gets that eye anyway (`READ.json`).
 *
 * So it points rather than refuses. `scripts/study-pilot.sh` prints
 * this before it asks who read the answers.
 */
export function closeReading(value: unknown): string[] {
  const transcript = value as { responses?: unknown };
  if (!Array.isArray(transcript.responses)) return [];
  const found: string[] = [];
  transcript.responses.forEach((response, index) => {
    const text = (response as { correction?: unknown } | null)?.correction;
    if (typeof text === "string" && text.length > FREE_TEXT_LIMIT) {
      found.push(
        `response ${index + 1}: a ${text.length}-character answer, past the ${FREE_TEXT_LIMIT} that reads at a glance`,
      );
    }
  });
  return found;
}

/** One line of `transcripts/READ.json`: who read a file, and when. */
export interface ReadEntry {
  file: string;
  by: string;
  at: string;
}

export interface ReadManifest {
  schema: "kettle/study-read@0";
  read: ReadEntry[];
}

/**
 * Everything wrong with the manifest, given the files actually present.
 *
 * The rule is symmetric on purpose. An unlisted transcript is one
 * nobody signed for; a listed file that does not exist is a signature
 * for something that is not there, which is how a manifest comes to
 * describe a different set from the one on disk (#466's lesson about
 * declaring a tolerated family rather than remembering it).
 */
export function manifestFaults(manifest: unknown, files: string[]): string[] {
  if (typeof manifest !== "object" || manifest === null) return ["READ.json is not an object"];
  if ((manifest as Record<string, unknown>).schema !== "kettle/study-read@0") {
    return ["READ.json does not name its schema"];
  }
  const read = (manifest as Record<string, unknown>).read;
  if (!Array.isArray(read)) return ["READ.json has no read list"];

  const found: string[] = [];
  const listed = new Set<string>();
  for (const entry of read as ReadEntry[]) {
    if (typeof entry?.file !== "string" || entry.file === "") {
      found.push("an entry names no file");
      continue;
    }
    listed.add(entry.file);
    if (typeof entry.by !== "string" || entry.by.trim() === "") {
      found.push(`${entry.file} is signed by nobody`);
    }
    if (typeof entry.at !== "string" || !/^\d{4}-\d{2}-\d{2}$/.test(entry.at)) {
      found.push(`${entry.file} was read on ${JSON.stringify(entry.at)}`);
    }
    if (!files.includes(entry.file)) {
      found.push(`${entry.file} is listed as read and is not here`);
    }
  }
  for (const file of files) {
    if (!listed.has(file)) {
      found.push(`${file} is here and nobody has read it`);
    }
  }
  return found;
}
