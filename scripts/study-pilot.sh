#!/usr/bin/env bash
# #431: run one pilot session of the study on yourself, end to end.
#
# The playbook in `app/study/PILOT.md`, as a script. Each step prints
# what it is about to do and stops on the first thing that is wrong,
# because a pilot session whose corpus is stale or whose transcript
# fails the floor is half an hour spent on nothing.
#
#   scripts/study-pilot.sh next          # which id to sit under
#   scripts/study-pilot.sh corpus [N]    # (re)generate N letters — needs weights, ~1 min a letter
#   scripts/study-pilot.sh serve         # open the harness on the letter track
#   scripts/study-pilot.sh file <json>   # take a finished transcript in: floor, READ.json, score
#
# Runs from anywhere; every path is from the repository root.
set -euo pipefail
cd "$(dirname "$0")/.."

MODEL="${KETTLE_MODEL:-$HOME/Library/Application Support/app.kttl.kettle/models/qwen3.5-4b-q4_k_m.gguf}"
LETTERS="${KETTLE_LETTERS:-../kettle-examples/out-bed}"
TRANSCRIPTS=fixtures/study/transcripts

# The next id nobody has sat under. A file on disk is not the whole
# story: a transcript can be retired (#577 moved the draw under `a01`
# the same day it was sat), and reusing its number would put two
# different sessions under one id in a published record. Retired ids are
# written down rather than remembered.
next_id() {
  local n=1 id
  while :; do
    id=$(printf 'a%02d' "$n")
    if [ -e "$TRANSCRIPTS/$id.json" ] || is_retired "$id"; then
      n=$((n + 1))
      continue
    fi
    printf '%s' "$id"
    return
  done
}

is_retired() {
  [ -f "$TRANSCRIPTS/RETIRED.json" ] || return 1
  jq -e --arg id "$1" 'any(.retired[]; .participant == $id)' \
    "$TRANSCRIPTS/RETIRED.json" >/dev/null 2>&1
}

case "${1:-}" in
  next)
    echo "Sit this session as: $(next_id)"
    echo "(an a-number is the author's own pilot — never one of the twenty, and marked so in the file)"
    ;;
  corpus)
    count="${2:-14}"
    [ -f "$MODEL" ] || { echo "no weights at $MODEL (set KETTLE_MODEL)"; exit 1; }
    [ -d "$LETTERS" ] || { echo "no letters at $LETTERS — in ../kettle-examples run: python -m synth_letters bed out --bed out-bed"; exit 1; }
    # `*.json` also matches READ.json, which is a manifest and not a
    # transcript, so the corpus could never be regenerated once the
    # directory existed. Transcript names are p01/a01 and nothing else.
    if ls "$TRANSCRIPTS"/[pa][0-9][0-9].json >/dev/null 2>&1; then
      echo "Transcripts already exist against the current corpus. Regenerating changes which"
      echo "letters a session draws, so every existing transcript would stop rebuilding."
      echo "Move them aside first, or generate to another directory with --out."
      exit 1
    fi
    echo "Running the letter pack for real over $count letters from $LETTERS …"
    cargo run -q -p runner --features pdf --example study_letters -- \
      --model "$MODEL" --letters "$LETTERS" --count "$count" --out fixtures/study/letters
    echo "Auditing against the bed's expected answers …"
    python3 fixtures/study/audit-letters.py
    echo
    echo "Now read fixtures/study/audit-letters.json: every 'extra' and 'missed' is a natural"
    echo "error or a fair reading the bed did not list, and a person decides which before a"
    echo "clean control is clean. Fill in read_by and clean for each letter."
    ;;
  serve)
    n=$(ls fixtures/study/letters/letter-*.json 2>/dev/null | wc -l | tr -d ' ')
    [ "$n" -ge 10 ] || { echo "the letter corpus holds $n letters and a session needs 10 — run: $0 corpus"; exit 1; }
    echo "Sit as $(next_id) — the link carries it, so the box on screen is already filled in."
    echo "When the last screen shows the file, save it as $TRANSCRIPTS/$(next_id).json and run: $0 file <that path>"
    # The id travels in the link. It used to be printed here and typed
    # there, and on the first sitting the screen's own suggestion won.
    (sleep 2; open "http://localhost:5174/?material=letters&participant=$(next_id)" 2>/dev/null || true) &
    cd app && bun run study:dev
    ;;
  file)
    src="${2:?path to the transcript json}"
    id=$(python3 -c "import json,sys;print(json.load(open(sys.argv[1]))['participant'])" "$src")
    dest="$TRANSCRIPTS/$id.json"
    if [ "$(realpath "$src")" != "$(realpath -q "$dest" 2>/dev/null || echo)" ]; then
      [ ! -e "$dest" ] || { echo "$dest already exists"; exit 1; }
      cp "$src" "$dest"
    fi
    echo "Read the free-text answer in every task of $dest before this goes any further."
    # Length points at a person; it does not refuse. A participant who
    # explains themselves at length has done nothing wrong, and their
    # file may not be edited after they hand it over.
    (cd app && bun -e "
      import { closeReading } from './src/lib/study-transcripts';
      const t = JSON.parse(await Bun.file('../$dest').text());
      for (const note of closeReading(t)) console.log('  ' + note);
    " 2>/dev/null) || true
    read -r -p "Who read them (name)? " reader
    python3 - "$dest" "$reader" <<'PY'
import json, sys, datetime, pathlib
dest, reader = sys.argv[1], sys.argv[2]
manifest = pathlib.Path(dest).parent / "READ.json"
read = json.loads(manifest.read_text()) if manifest.exists() else {"schema": "kettle/study-read@0", "read": []}
name = pathlib.Path(dest).name
read["read"] = [e for e in read["read"] if e["file"] != name]
read["read"].append({"file": name, "by": reader, "at": datetime.date.today().isoformat()})
manifest.write_text(json.dumps(read, indent=2) + "\n")
print(f"signed for in {manifest}")
PY
    echo "Running the floor and scoring …"
    (cd app && bun run test -- src/lib/study-transcripts >/dev/null) || { echo "the floor refused it — see: cd app && bun run test -- src/lib/study-transcripts"; exit 1; }
    (cd app && bun run study:score "../$dest")
    ;;
  *)
    sed -n '2,14p' "$0"
    exit 2
    ;;
esac
