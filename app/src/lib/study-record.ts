// #431: a finished session, written down.
//
// Two of the issue's "done when" clauses are properties of this file's
// output rather than of the study's conduct, so they are built in
// rather than promised.
//
// **Raw participant data is separated from product telemetry.** A
// transcript holds what a person said and nothing the product produced.
// The two have different owners, different retention and different
// consent, and a file that mixed them would have to be handled as the
// stricter of the two forever.
//
// **The study reproduces from committed fixtures.** A session is
// deterministic given the participant id and the corpus, so a
// transcript stores the id and the corpus's run ids — not the ten
// seeded reports. That is what makes the record small enough to keep
// and checkable by somebody who was not in the room.
//
// Scores are derived on read, never stored. This project bumps
// `SCORING_VERSION` when the meaning of a score changes and refuses
// baselines from another version; the same discipline applies here for
// the same reason — a stored outcome is a claim frozen under whatever
// rule was current, and re-asking it should cost a re-read rather than
// another afternoon of somebody's time.

import type { ConsentStamp } from "./study-consent";
import type { StudyLetter } from "./study-letter";
import { scoreLetter } from "./study-letter-response";
import { planLetters, type LetterSessionPlan } from "./study-letter-session";
import { score, type Response, type Score } from "./study-response";
import { plan, type Arm, type SessionPlan } from "./study-session";
import type { RunReport } from "./types";

/**
 * Which documents a session reads. Two tracks since 27 August 2026:
 * the ten audited statements, and the synthetic letters. A bare
 * `RunReport[]` is accepted as the statement track, which is what every
 * earlier caller passed.
 */
/**
 * The documents a session is drawn from, and a person's read of which
 * of them the pipeline got wrong (#577).
 *
 * `unclean` travels with the corpus rather than beside it because two
 * things need the same answer — the draw, which must not use one as a
 * clean control, and the scorer, which must not call a catch on one a
 * false alarm. Declared once, so a harness and a scorer cannot come to
 * different views of the same session.
 */
export type StudyCorpus =
  | { material: "statements"; reports: RunReport[]; unclean: ReadonlySet<string> }
  | { material: "letters"; letters: StudyLetter[]; unclean: ReadonlySet<string> };

export type Material = StudyCorpus["material"];

/**
 * Who sat the session. An `a`-numbered id is the study's author running
 * the instrument on themself — a pilot of the harness, never one of the
 * twenty, and marked so in the file rather than remembered.
 */
export type Role = "participant" | "author-pilot";

export function roleOf(participant: string): Role {
  return /^a\d{2}$/.test(participant.trim()) ? "author-pilot" : "participant";
}

export function asCorpus(corpus: StudyCorpus | RunReport[]): StudyCorpus {
  return Array.isArray(corpus)
    ? { material: "statements", reports: corpus, unclean: new Set() }
    : corpus;
}

function idsOf(corpus: StudyCorpus): string[] {
  return corpus.material === "statements"
    ? corpus.reports.map((report) => report.run.id)
    : corpus.letters.map((letter) => letter.id);
}

export interface Transcript {
  schema: "kettle/study-transcript@0";
  participant: string;
  role: Role;
  material: Material;
  arm: Arm;
  consent: ConsentStamp;
  /** The corpus's run ids, in the order it was given to `plan`. */
  corpus: string[];
  /**
   * The ten documents this session showed, in the order it showed them.
   *
   * The corpus list above catches a corpus that changed. It cannot
   * catch a *judgement* that changed: the draw takes its clean controls
   * only from letters the audit calls clean (#577), so correcting the
   * audit moves the draw while the corpus stays identical. `a01` was
   * scored that way for an hour and the table looked perfectly
   * ordinary — three of its rows were simply about other letters.
   *
   * So the session records what it showed, and `rebuild` refuses a plan
   * that disagrees rather than scoring one. The same principle as the
   * consent stamp: record the words shown, do not recompute them.
   */
  drawn: string[];
  /** One slot per task, `null` until answered. */
  responses: (Response | null)[];
}

/** Open a session: consent first, then the plan. */
export function begin(
  participant: string,
  given: StudyCorpus | RunReport[],
  consent: ConsentStamp,
): Transcript {
  const corpus = asCorpus(given);
  // Not a boolean, and not a version alone. A transcript that recorded
  // only "they agreed" could not answer what they agreed to, and a
  // version could name a document whose words had since moved under
  // it — so the digest of the rendered text travels too. `stamp` is the
  // only way to build one, so in the harness the pair cannot disagree.
  if (
    consent.version.trim() === "" ||
    consent.digest.trim() === "" ||
    consent.given_at.trim() === ""
  ) {
    throw new Error(
      "consent must name the text that was shown, its words, and when it was given",
    );
  }
  const session =
    corpus.material === "statements"
      ? plan(participant, corpus.reports)
      : planLetters(participant, corpus.letters, corpus.unclean);
  return {
    schema: "kettle/study-transcript@0",
    participant,
    role: roleOf(participant),
    material: corpus.material,
    arm: session.arm,
    consent,
    corpus: idsOf(corpus),
    drawn: session.tasks.map((task) => task.document),
    responses: session.tasks.map(() => null),
  };
}

/** One answer, in its task's slot. */
export function record(
  transcript: Transcript,
  index: number,
  response: Response,
): Transcript {
  if (index < 0 || index >= transcript.responses.length) {
    throw new Error(
      `task ${index} is not in this session, which has ${transcript.responses.length} tasks`,
    );
  }
  const responses = [...transcript.responses];
  responses[index] = response;
  return { ...transcript, responses };
}

/**
 * The session this transcript describes, rebuilt from the corpus.
 *
 * Refuses a corpus that is not the one the participant saw. A plan
 * drawn from different reports assigns different seeds to different
 * documents, so scoring against it would produce a complete table with
 * nothing in it to show that anything had gone wrong.
 */
export function rebuild(
  transcript: Transcript,
  given: StudyCorpus | RunReport[],
): SessionPlan | LetterSessionPlan {
  const corpus = asCorpus(given);
  if (corpus.material !== transcript.material) {
    throw new Error(
      `this transcript was recorded over ${transcript.material}, and the corpus given is ${corpus.material}`,
    );
  }
  const ids = idsOf(corpus);
  if (ids.join(" ") !== transcript.corpus.join(" ")) {
    throw new Error(
      `this transcript was recorded against corpus [${transcript.corpus.join(", ")}], not [${ids.join(", ")}]`,
    );
  }
  const session =
    corpus.material === "statements"
      ? plan(transcript.participant, corpus.reports)
      : planLetters(transcript.participant, corpus.letters, corpus.unclean);
  // The corpus is the same and the plan is not: something the draw
  // depends on has been judged differently since. Refuse, rather than
  // score ten answers against documents nobody was shown.
  const drew = session.tasks.map((task) => task.document);
  if (transcript.drawn !== undefined && drew.join(" ") !== transcript.drawn.join(" ")) {
    throw new Error(
      `this transcript showed [${transcript.drawn.join(", ")}] and the session now draws [${drew.join(", ")}] — something the draw depends on has been judged differently since it was sat`,
    );
  }
  return session;
}

/**
 * One row per answered task. An unanswered task has no row: a
 * participant who stopped early has not accepted the rest, and a
 * defaulted row would say they did.
 */
/**
 * The documents an audit records as carrying an error the pipeline made
 * on its own, by `document` id (#577).
 *
 * Passing none scores as before, which is what the harness does: a
 * session in a browser has no audit to hand, and it is not the
 * harness's job to judge the corpus. The command line has both.
 */
export type Unclean = ReadonlySet<string>;

export function scores(transcript: Transcript, given: StudyCorpus | RunReport[]): Score[] {
  const corpus = asCorpus(given);
  const audit = (document: string) => ({ unclean: corpus.unclean.has(document) });
  if (corpus.material === "statements") {
    const session = rebuild(transcript, corpus) as SessionPlan;
    return session.tasks.flatMap((task) => {
      const response = transcript.responses[task.index];
      return response == null ? [] : [score(task, response, audit(task.document))];
    });
  }
  const session = rebuild(transcript, corpus) as LetterSessionPlan;
  return session.tasks.flatMap((task) => {
    const response = transcript.responses[task.index];
    return response == null ? [] : [scoreLetter(task, response, audit(task.document))];
  });
}
