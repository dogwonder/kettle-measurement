// #431: what one participant sees on the letter track, decided before
// they arrive.
//
// The same promises `study-session.ts` keeps for statements — order
// and seed assignment drawn from a generator seeded by the participant
// id, so a finished session rebuilds from the id alone — over the
// letter corpus and the letter operators. The mix is the frozen one:
// three inventions, three mis-relations, two omissions, two clean.
//
// One operator per class here where statements carry two mis-relation
// operators. A letter's invention and mis-relation differ in *kind*
// of evidence rather than in which field moved, and the value moved is
// always the deadline — so there is no second operator to alternate
// with, and the learnability argument that split the statement
// operators does not arise the same way: the participant cannot tell
// from the page whether a date was read or worked out, which is the
// point of the pair.

import {
  eligible,
  seedLetter,
  type LetterOperator,
  type LetterTruth,
  type StudyLetter,
} from "./study-letter";
import {
  armFor,
  enrolment,
  generator,
  hash,
  MIX,
  shuffled,
  type Arm,
  type TaskClass,
} from "./study-session";

export interface LetterTask {
  index: number;
  /** Which corpus letter this task was built from. */
  document: string;
  class: TaskClass;
  /** What the participant sees. */
  letter: StudyLetter;
  /** The gold answer, or `null` for a clean letter. */
  truth: LetterTruth | null;
  /** The action the emphasised arm points at, drawn as it is for statements. */
  emphasis: string | null;
}

export interface LetterSessionPlan {
  participant: string;
  enrolment: number;
  arm: Arm;
  tasks: LetterTask[];
}

const OPERATOR_FOR: Record<Exclude<TaskClass, "clean">, LetterOperator> = {
  invention: "misquoted-deadline",
  "mis-relation": "misresolved-deadline",
  omission: "dropped-obligation",
};

function emphasisFor(shown: StudyLetter, random: () => number): string | null {
  const claims = shown.actions.actions.map((action) => action.title);
  if (claims.length === 0) return null;
  return claims[Math.floor(random() * claims.length)] as string;
}

/**
 * One participant's session over letters.
 *
 * Classes are drawn in a shuffled order and each takes the first
 * unused letter that can carry it, rather than pairing letter and
 * class by position: a letter with one undated ask cannot carry a
 * mis-relation, and a session that failed on that would fail for the
 * corpus rather than for the participant. The draw is still the
 * participant's own, so it is reproducible from the id.
 */
export function planLetters(
  participant: string,
  corpus: StudyLetter[],
  /**
   * The letters a person's read of the audit records as carrying an
   * error the pipeline made on its own (#577). Clean controls are
   * never drawn from these: the two clean tasks are what the
   * false-alarm rate is measured on, and a control the pipeline
   * already got wrong cannot measure it.
   *
   * Defaults to none so a test may plan over a bare corpus. Every real
   * caller passes the corpus's own set, which is why it travels on
   * `StudyCorpus` — the harness that shows a session and the scorer
   * that reads it must not be able to disagree about which letters
   * were eligible.
   */
  unclean: ReadonlySet<string> = new Set(),
): LetterSessionPlan {
  // Protocol before anything else: a participant with no place in the
  // enrolment order is refused in those words, not in the corpus's.
  const placed = enrolment(participant);
  if (corpus.length < MIX.length) {
    throw new Error(
      `a session needs ${MIX.length} letters, one per task; the corpus holds ${corpus.length} letters`,
    );
  }
  const random = generator(hash(participant));
  const documents = shuffled(corpus, random);
  const classes = shuffled(MIX, random);
  const taken = new Set<string>();
  /** Operator-and-ask pairs already used in this session. */
  const used = new Set<string>();

  // Scarcest class first — a mis-resolution needs a date somebody
  // worked out, which fewer letters carry — so that a clean task never
  // takes the one letter a seed needed. The order on the page is still
  // `classes`; only the choosing is by priority.
  const PRIORITY: TaskClass[] = ["mis-relation", "invention", "omission", "clean"];
  const tasks: LetterTask[] = [];
  for (const taskClass of PRIORITY) {
    for (const [index, each] of classes.entries()) {
      if (each !== taskClass) continue;
      if (taskClass === "clean") {
        const letter = documents.find(
          (candidate) => !taken.has(candidate.id) && !unclean.has(candidate.id),
        );
        if (letter === undefined) {
          throw new Error(
            `no letter left for a clean task: ${unclean.size} of ${corpus.length} are audited unclean, and the rest are in this session`,
          );
        }
        taken.add(letter.id);
        tasks.push({ index, document: letter.id, class: taskClass, letter, truth: null, emphasis: emphasisFor(letter, random) });
        continue;
      }
      const operator = OPERATOR_FOR[taskClass];
      let done = false;
      for (const letter of documents) {
        if (taken.has(letter.id)) continue;
        const targets = eligible(letter, operator).filter((ask) => !used.has(`${operator}:${ask}`));
        if (targets.length === 0) continue;
        taken.add(letter.id);
        const target = targets[Math.floor(random() * targets.length)] as string;
        used.add(`${operator}:${target}`);
        const seeded = seedLetter(letter, { operator, target });
        tasks.push({
          index,
          document: letter.id,
          class: taskClass,
          letter: seeded.letter,
          truth: seeded.truth,
          emphasis: emphasisFor(seeded.letter, random),
        });
        done = true;
        break;
      }
      if (!done) {
        throw new Error(
          `no letter in the corpus can carry ${operator} for task ${index + 1}: every letter that could is already in this session`,
        );
      }
    }
  }
  tasks.sort((a, b) => a.index - b.index);

  return { participant, enrolment: placed, arm: armFor(participant), tasks };
}
