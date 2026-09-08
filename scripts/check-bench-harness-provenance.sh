#!/usr/bin/env bash
# Is the FSR bench harness still the one that produced the record we compare
# against?
#
# The R6a-stale-* tracker in ROADMAP.md is the only place in the repo that
# records whether two bench records are comparable, and its fold ritual carried
# the sentence "no commit against scripts/fsr-bench-matrix.sh or
# scripts/fsr_bench_report.py since <record>" forward verbatim through five
# updates. It was true when written (2026-08-19), became false on 2026-08-28,
# and was then repeated twice — licensing an apples-to-apples read of two
# matrices whose column sets, acceptance gates and render_sum arithmetic all
# differ (#4024).
#
# The sentence is derivable, so derive it. Run this at fold time and paste the
# verdict instead of copying the previous one.
#
#   scripts/check-bench-harness-provenance.sh              # every archived record
#   scripts/check-bench-harness-provenance.sh 34074b93     # one record commit
#   scripts/check-bench-harness-provenance.sh --strict …   # exit 1 on divergence
#   scripts/check-bench-harness-provenance.sh --self-test
#
# Needs full history: `git log <record>..HEAD` cannot answer on a shallow clone.

set -uo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
HARNESS_FILES=(scripts/fsr-bench-matrix.sh scripts/fsr_bench_report.py)

STRICT=0
SELF_TEST=0
RECORDS=()
for arg in "$@"; do
  case "$arg" in
    --strict) STRICT=1 ;;
    --self-test) SELF_TEST=1 ;;
    -*) echo "unknown flag: $arg" >&2; exit 2 ;;
    *) RECORDS+=("$arg") ;;
  esac
done

# Report on one record commit. Returns 1 if the harness moved since it.
check_record() {
  local record="$1" label="${2:-}"
  if ! git -C "$REPO" cat-file -e "${record}^{commit}" 2>/dev/null; then
    printf '%s%s: engine commit not in this clone (shallow?) — CANNOT VERIFY\n' \
      "${label:+$label  }" "$record"
    return 1
  fi
  local log
  log="$(git -C "$REPO" log --format='    %h %ad  %s' --date=short \
    "${record}..HEAD" -- "${HARNESS_FILES[@]}")"
  if [[ -z "$log" ]]; then
    printf '%s%s: harness byte-stable since this record — a re-run is a valid\n' \
      "${label:+$label  }" "$record"
    printf '    apples-to-apples comparison against it.\n'
    return 0
  fi
  local n
  n="$(printf '%s\n' "$log" | wc -l)"
  printf '%s%s: DIVERGED — %s commit(s) to the harness pair since this record.\n' \
    "${label:+$label  }" "$record" "$n"
  printf '%s\n' "$log"
  printf '    A re-run is NOT a guaranteed apples-to-apples comparison.\n'
  return 1
}

if (( SELF_TEST )); then
  fail=0
  # A record at HEAD can have nothing after it — the stable branch must fire.
  if ! check_record "$(git -C "$REPO" rev-parse HEAD)" >/dev/null; then
    echo "check-bench-harness-provenance: self-test called HEAD diverged from itself" >&2
    fail=1
  fi
  # `0e91fc5e` (2026-08-28) changed fsr_bench_report.py's render_sum. It is an
  # ancestor of HEAD forever, so `34074b93..HEAD` must always name it — this is
  # the exact fact the tracker asserted away twice (#4024).
  out="$(check_record 34074b93 2>&1)"
  if [[ "$out" != *DIVERGED* || "$out" != *0e91fc5e* ]]; then
    echo "check-bench-harness-provenance: self-test did not detect the known" \
         "post-34074b93 harness divergence" >&2
    printf '%s\n' "$out" >&2
    fail=1
  fi
  # Every commit that ever touched fsr_bench_report.py also touched
  # fsr-bench-matrix.sh, so no range in this history distinguishes "watches both"
  # from "watches only the shell script" — the check above passes either way.
  # Pin the watched set directly instead. The reporter is the half whose
  # arithmetic moved (0e91fc5e's render_sum), so dropping it from the pair is
  # precisely the blind spot this tool exists to close.
  for want in scripts/fsr-bench-matrix.sh scripts/fsr_bench_report.py; do
    found=0
    for have in "${HARNESS_FILES[@]}"; do
      [[ "$have" == "$want" ]] && found=1
    done
    if (( ! found )); then
      echo "check-bench-harness-provenance: self-test — $want is not in the" \
           "watched harness pair" >&2
      fail=1
    fi
  done
  (( fail )) && exit 1
  echo "check-bench-harness-provenance: self-test passed"
  exit 0
fi

diverged=0
if (( ${#RECORDS[@]} )); then
  for record in "${RECORDS[@]}"; do
    check_record "$record" || diverged=1
  done
else
  shopt -s nullglob
  for tsv in "$REPO"/docs/audits/BENCH_*.tsv; do
    # The harness stamps `engine=<commit>` into the TSV (#2835); pre-f19f7f15
    # archives carry a hand-written header with no engine token.
    engine="$(sed -n 's/.*[^a-z]engine=\([0-9a-f]\{7,\}\).*/\1/p' "$tsv" | head -1)"
    name="$(basename "$tsv")"
    if [[ -z "$engine" ]]; then
      printf '%s: no engine= stamp (pre-#2835 archive) — range not bounded, skipped\n' "$name"
      continue
    fi
    check_record "$engine" "$name" || diverged=1
  done
fi

if (( diverged && STRICT )); then
  exit 1
fi
exit 0
