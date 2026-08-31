#!/usr/bin/env bash
# Require every issue a PR declares it will close to be named by a closing
# keyword in at least one commit. This preserves fix -> issue archaeology for
# regression audits instead of relying on a manually maintained convention.

set -euo pipefail

closing_issue_numbers() {
    rg --only-matching --ignore-case \
        '(fix|fixes|fixed|close|closes|closed|resolve|resolves|resolved)[[:space:]]+#[0-9]+' |
        rg --only-matching '[0-9]+' |
        sort -nu
}

# Reads the haystack on stdin. Callers MUST feed it with a here-string, not a
# pipe: `rg --quiet` exits on its first match, and under `set -o pipefail` a
# writer still pushing bytes into that pipe takes SIGPIPE and turns the whole
# pipeline's status into 141 — reported as "no citation" for an issue that IS
# cited. The failure is size-dependent (a body under the 64 KiB pipe buffer
# completes before rg exits, so it passes), which is why this went unnoticed:
# a real session's commit-message body is hundreds of KiB and a two-line
# self-test fixture is not.
commit_cites_issue() {
    local issue="$1"
    rg --quiet --ignore-case \
        "(^|[^[:alnum:]_])(fix|fixes|fixed|close|closes|closed|resolve|resolves|resolved)[[:space:]]+#${issue}([^0-9]|$)"
}

if [[ "${1:-}" == "--self-test" ]]; then
    sample_body=$'Fixes #12\nResolved #34\nmentions #56'
    mapfile -t sample_issues < <(printf '%s\n' "${sample_body}" | closing_issue_numbers)
    [[ "${sample_issues[*]}" == "12 34" ]]
    printf '%s\n' 'fix(core): bounded walk' 'Fix #12' | commit_cites_issue 12
    if commit_cites_issue 12 <<<'fix(core): bounded walk (#12)'; then
        echo "check-issue-traceability: self-test accepted a non-closing citation" >&2
        exit 1
    fi
    # A body larger than the 64 KiB pipe buffer, with the citation FIRST so
    # `rg --quiet` exits long before the writer is done. Under the old
    # `printf | commit_cites_issue` shape this returned 141 and the issue was
    # reported uncited; the two-line fixtures above cannot reach that path.
    big_body="Fix #12"$'\n'"$(head -c 200000 /dev/zero | tr '\0' 'x')"
    if ! commit_cites_issue 12 <<<"${big_body}"; then
        echo "check-issue-traceability: self-test lost a citation in a large body \
(SIGPIPE/pipefail regression)" >&2
        exit 1
    fi
    echo "check-issue-traceability: self-test passed"
    exit 0
fi

# Window-audit mode (#3218). The PR mode above is gated on
# `github.event_name == 'pull_request'` in ci.yml, but this repo's history is
# overwhelmingly direct commits to main, so for the dominant workflow that gate
# never fires at all. That is why 43 of 134 issues closed in the 2026-08-16..20
# window (32%) ended up with no commit citing them, and 14 with no citation
# anywhere in the tree — every one of them genuinely fixed, but unverifiable.
#
# `/audit-regression` Step 2.1 is `git log --grep="#<N>"`. When that returns
# nothing for a third of a window, the audit cannot tell "no citation" from "no
# fix", so it reports UNVERIFIABLE or — worse and more likely — files a FAIL
# against a fix that is present. The degradation is self-concealing: a
# regression audit that cannot find fixes gets quieter, not louder.
#
# This mode reports the gap while the context is still fresh, at close time,
# rather than leaving it for the next sweep to rediscover. Needs `gh`.
if [[ "${1:-}" == "--window" ]]; then
    if [[ "$#" -ne 3 ]]; then
        echo "usage: $0 --window <base-commit> <head-commit>" >&2
        exit 2
    fi
    base="$2"
    head="$3"
    command -v gh >/dev/null 2>&1 || {
        echo "check-issue-traceability: --window needs the gh CLI" >&2
        exit 2
    }

    since="$(git log -1 --format=%cI "${base}")"
    commit_messages="$(git log --format='%B' "${base}..${head}")"

    mapfile -t closed < <(
        gh issue list --state closed --limit 500 \
            --search "closed:>=${since%T*}" --json number --jq '.[].number' | sort -n
    )
    if [[ "${#closed[@]}" -eq 0 ]]; then
        echo "check-issue-traceability: no issues closed in this window"
        exit 0
    fi

    uncited=()
    for issue in "${closed[@]}"; do
        commit_cites_issue "${issue}" <<<"${commit_messages}" && continue
        uncited+=("${issue}")
    done

    echo "check-issue-traceability: ${#closed[@]} issue(s) closed in ${base}..${head}"
    if [[ "${#uncited[@]}" -eq 0 ]]; then
        echo "check-issue-traceability: every one is cited by a closing-keyword commit"
        exit 0
    fi

    echo
    echo "ZERO-CITATION SET -- ${#uncited[@]} of ${#closed[@]} closed issues have no"
    echo "closing-keyword commit in this range. Each is either:"
    echo "  (a) closed as a side effect of another issue's fix -- leave a GitHub close"
    echo "      comment naming that issue ('resolved as a side effect of #NNNN'), so the"
    echo "      archaeology survives outside the commit log; or"
    echo "  (b) fixed by a commit that forgot the keyword -- say so in a close comment."
    echo
    for issue in "${uncited[@]}"; do
        title="$(gh issue view "${issue}" --json title --jq .title 2>/dev/null || echo '?')"
        printf '  #%-6s %s\n' "${issue}" "${title}"
    done
    # Advisory: this reports history that is already written and cannot be
    # fixed by failing a build.
    exit 0
fi

# Orphan-fix mode (#3425). `--window` and the PR-mode default both start
# from the CLOSED/declared set, so a fix that landed without ever closing
# its issue is invisible to both: it isn't in a PR body (this repo's history
# is overwhelmingly direct commits to main), and by definition it isn't in
# the closed set either. Unlike a missing citation on an already-closed
# issue, this direction loses more than archaeology — the issue stays OPEN,
# so the fix gets re-planned, re-audited, and risks being reimplemented or
# reverted.
#
# The signal this mode looks for is already in the tree: a fix author
# writing the issue number into the source comment they land. So instead of
# starting from a declared-closed set, this starts from every `#NNNN` a
# commit in the range actually *added* to a `.rs` file, and flags the ones
# that are still OPEN with no closing-keyword commit citing them.
if [[ "${1:-}" == "--orphan" ]]; then
    if [[ "$#" -ne 3 ]]; then
        echo "usage: $0 --orphan <base-commit> <head-commit>" >&2
        exit 2
    fi
    base="$2"
    head="$3"
    command -v gh >/dev/null 2>&1 || {
        echo "check-issue-traceability: --orphan needs the gh CLI" >&2
        exit 2
    }

    # Only lines a commit in this range *added* (single `+`, not the `+++`
    # file-header line) — a reference that was already there before `base`
    # isn't new to this range, and an added line is the exact shape of the
    # motivating evidence (a fix comment citing its issue).
    mapfile -t referenced < <(
        git diff --unified=0 "${base}..${head}" -- '*.rs' |
            grep -E '^\+[^+]' |
            rg --only-matching '#[0-9]+' |
            rg --only-matching '[0-9]+' |
            sort -nu
    )
    if [[ "${#referenced[@]}" -eq 0 ]]; then
        echo "check-issue-traceability: no #NNNN references added to a .rs file in ${base}..${head}"
        exit 0
    fi

    commit_messages="$(git log --format='%B' "${base}..${head}")"

    orphans=()
    for issue in "${referenced[@]}"; do
        commit_cites_issue "${issue}" <<<"${commit_messages}" && continue
        state="$(gh issue view "${issue}" --json state --jq .state 2>/dev/null || echo '')"
        [[ "${state}" == "OPEN" ]] || continue
        orphans+=("${issue}")
    done

    echo "check-issue-traceability: ${#referenced[@]} issue number(s) newly referenced in ${base}..${head}'s .rs diff"
    if [[ "${#orphans[@]}" -eq 0 ]]; then
        echo "check-issue-traceability: every one is either cited by a closing-keyword commit or isn't OPEN"
        exit 0
    fi

    echo
    echo "CANDIDATE ORPHAN SET -- ${#orphans[@]} issue(s) are named in a comment this range"
    echo "added, are still OPEN, and are not cited by any closing-keyword commit here. Each is"
    echo "either:"
    echo "  (a) a legitimately forward-looking reference (a TODO naming a future issue, e.g."
    echo "      #3307/#3308) -- no action needed; or"
    echo "  (b) genuinely fixed in this range without a closing keyword -- close it with a"
    echo "      comment naming the landing commit."
    echo
    for issue in "${orphans[@]}"; do
        title="$(gh issue view "${issue}" --json title --jq .title 2>/dev/null || echo '?')"
        printf '  #%-6s %s\n' "${issue}" "${title}"
    done
    # Advisory, like --window: a source comment naming a future issue is a
    # legitimate pattern this mode cannot distinguish from a forgotten
    # closing keyword, so it reports candidates rather than failing a build.
    exit 0
fi

if [[ "$#" -ne 2 ]]; then
    echo "usage: $0 <base-commit> <head-commit>" >&2
    echo "       $0 --window <base-commit> <head-commit>   # close-time citation audit" >&2
    echo "       $0 --orphan <base-commit> <head-commit>   # fixed-but-never-closed audit" >&2
    exit 2
fi

base="$1"
head="$2"
pr_body="${PR_BODY:-}"
mapfile -t closing_issues < <(printf '%s\n' "${pr_body}" | closing_issue_numbers)

if [[ "${#closing_issues[@]}" -eq 0 ]]; then
    echo "check-issue-traceability: PR declares no issues closed"
    exit 0
fi

commit_messages="$(git log --format='%B' "${base}..${head}")"
missing=()
for issue in "${closing_issues[@]}"; do
    if ! commit_cites_issue "${issue}" <<<"${commit_messages}"; then
        missing+=("#${issue}")
    fi
done

if [[ "${#missing[@]}" -ne 0 ]]; then
    echo "check-issue-traceability: PR closes issues with no closing-keyword commit: ${missing[*]}" >&2
    echo "Add a commit-body line such as 'Fix #123' for each issue. This list is the window's zero-citation report." >&2
    exit 1
fi

echo "check-issue-traceability: ${#closing_issues[@]} closing issue(s) are cited by commits"
