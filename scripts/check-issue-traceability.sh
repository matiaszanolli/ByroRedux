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

if [[ "$#" -ne 2 ]]; then
    echo "usage: $0 <base-commit> <head-commit>" >&2
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
