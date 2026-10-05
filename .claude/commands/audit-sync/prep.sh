#!/usr/bin/env bash
# .claude/commands/audit-sync/prep.sh
#
# Deterministic half of /audit-sync: everything the fan-out agents need, computed
# once so ten agents don't each re-derive it.
#
#   .claude/commands/audit-sync/prep.sh <scratch-dir> [--base <rev>]
#
# Writes into <scratch-dir>:
#   BRIEF.md                     brief.tmpl rendered with base/head/scratch
#   open.txt                     every OPEN issue number (snapshot)
#   cited.txt                    every #NNN cited by an audit skill or shared file
#   closed_since.tsv             number<TAB>title, closed since the base commit date
#   cited_closed_recent_map.txt  `#N: skill …` for cited issues closed since base
#   weights.tsv                  skill<TAB>routed-changed-files<TAB>cited-closed-since
#   proposals.md                 empty collector for out-of-scope agent proposals
# and checks that groups.txt covers every audit-* directory exactly once.

set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

here=.claude/commands/audit-sync
scratch="${1:?usage: prep.sh <scratch-dir> [--base <rev>]}"
shift
base="$(awk '{print $1; exit}' "$here/BASELINE")"
if [[ "${1:-}" == "--base" ]]; then base="${2:?--base needs a rev}"; fi
git rev-parse --verify -q "$base^{commit}" >/dev/null || { echo "bad base: $base" >&2; exit 1; }

mkdir -p "$scratch"
head="$(git rev-parse --short HEAD)"
base_date="$(git log -1 --format=%cs "$base")"
ncommits="$(git rev-list --count "$base..HEAD")"
skills=( $(ls -d .claude/commands/audit-*/ | sed -E 's#.*/audit-([^/]+)/#\1#') )

# --- groups.txt coverage -----------------------------------------------------
grouped="$(grep -vE '^\s*(#|$)' "$here/groups.txt" | cut -d: -f2- | tr ' ' '\n' | grep -v '^$' | sort)"
dups="$(uniq -d <<<"$grouped")"
missing="$(comm -23 <(printf '%s\n' "${skills[@]}" | sort) <(sort -u <<<"$grouped"))"
unknown="$(comm -13 <(printf '%s\n' "${skills[@]}" | sort) <(sort -u <<<"$grouped"))"
[[ -z "$dups$missing$unknown" ]] || {
    echo "groups.txt mismatch — duplicated: ${dups:-none}; ungrouped: ${missing:-none}; unknown: ${unknown:-none}" >&2
    exit 1
}

# --- issue state -------------------------------------------------------------
grep -ohE '#[0-9]{3,5}\b' .claude/commands/audit-*/SKILL.md .claude/commands/_audit-*.md \
    | tr -d '#' | sort -u > "$scratch/cited.txt"
gh issue list --state open --limit 5000 --json number --jq '.[].number' | sort -u > "$scratch/open.txt"
gh issue list --state closed --search "closed:>=$base_date" --limit 2000 \
    --json number,title --jq '.[] | "\(.number)\t\(.title)"' > "$scratch/closed_since.tsv"
: > "$scratch/cited_closed_recent_map.txt"
for n in $(comm -12 "$scratch/cited.txt" <(cut -f1 "$scratch/closed_since.tsv" | sort -u)); do
    where="$(grep -lE "#$n\b" .claude/commands/audit-*/SKILL.md .claude/commands/_audit-*.md \
        | sed -E 's#.claude/commands/(audit-)?##; s#/SKILL.md##' | tr '\n' ' ')"
    echo "#$n: $where" >> "$scratch/cited_closed_recent_map.txt"
done

# --- per-skill weight --------------------------------------------------------
# Changed files routed through the ownership map; `per-game` fans out to every
# title; `Dim N` qualifiers and `(…)` notes are dropped.
games="fnv fo3 oblivion skyrim fo4 starfield"
routed="$(git diff --name-only "$base..HEAD" | .claude/commands/_audit-route.sh | cut -f3 \
    | sed -E 's/\([^)]*\)//g; s/Dim [0-9+]+//g; s/[;,]/\n/g' | sed -E 's/^ +| +$//g' | grep -v '^$' \
    | sed "s/^per-game$/${games// /\\n}/" | sort | uniq -c)"
{
    printf 'skill\trouted_files\tcited_closed_since\n'
    for s in "${skills[@]}"; do
        r="$(awk -v s="$s" '$2==s {print $1}' <<<"$routed")"
        c="$(grep -cE "(^|[: ])$s( |$)" "$scratch/cited_closed_recent_map.txt" || true)"
        printf '%s\t%s\t%s\n' "$s" "${r:-0}" "${c:-0}"
    done
} > "$scratch/weights.tsv"

# --- brief -------------------------------------------------------------------
sed -e "s#{{BASE}}#$base#g" -e "s#{{BASE_DATE}}#$base_date#g" -e "s#{{HEAD}}#$head#g" \
    -e "s#{{NCOMMITS}}#$ncommits#g" -e "s#{{SCRATCH}}#$scratch#g" "$here/brief.tmpl" > "$scratch/BRIEF.md"
[[ -f "$scratch/proposals.md" ]] || : > "$scratch/proposals.md"

echo "base $base ($base_date) → HEAD $head: $ncommits commits"
echo "cited issues: $(wc -l < "$scratch/cited.txt"); cited & closed since base: $(wc -l < "$scratch/cited_closed_recent_map.txt")"
echo
echo "group weights (routed files / cited-closed):"
while IFS= read -r line; do
    [[ "$line" =~ ^[[:space:]]*(#|$) ]] && continue
    g="${line%%:*}"; members="${line#*:}"; rf=0; cc=0
    for s in $members; do
        read -r r c < <(awk -F'\t' -v s="$s" '$1==s {print $2, $3}' "$scratch/weights.tsv")
        rf=$((rf + r)); cc=$((cc + c))
    done
    printf '  %-16s %5d / %-4d %s\n' "$g" "$rf" "$cc" "$members"
done < "$here/groups.txt"
echo
echo "brief: $scratch/BRIEF.md"
