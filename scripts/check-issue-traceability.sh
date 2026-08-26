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
    if printf '%s\n' 'fix(core): bounded walk (#12)' | commit_cites_issue 12; then
        echo "check-issue-traceability: self-test accepted a non-closing citation" >&2
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
        printf '%s\n' "${commit_messages}" | commit_cites_issue "${issue}" && continue
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

if [[ "$#" -ne 2 ]]; then
    echo "usage: $0 <base-commit> <head-commit>" >&2
    echo "       $0 --window <base-commit> <head-commit>   # close-time citation audit" >&2
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
    if ! printf '%s\n' "${commit_messages}" | commit_cites_issue "${issue}"; then
        missing+=("#${issue}")
    fi
done

if [[ "${#missing[@]}" -ne 0 ]]; then
    echo "check-issue-traceability: PR closes issues with no closing-keyword commit: ${missing[*]}" >&2
    echo "Add a commit-body line such as 'Fix #123' for each issue. This list is the window's zero-citation report." >&2
    exit 1
fi

echo "check-issue-traceability: ${#closing_issues[@]} closing issue(s) are cited by commits"
