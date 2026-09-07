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

# Any `#NNNN` at all, closing keyword or not — the reverse-direction signal
# (#3504). A commit that fixes something and merely mentions its issue is
# still findable by `/audit-regression`'s `git log --grep`; one that names no
# issue anywhere is not findable by any means. Same here-string contract as
# `commit_cites_issue` above.
commit_mentions_issue() {
    rg --quiet '(^|[^[:alnum:]_])#[0-9]+([^0-9]|$)'
}

# The base of a pushed range. `github.event.before` is 40 zeros when the
# branch is created, and is a commit that no longer exists after a
# force-push; both would make `git log base..head` fail the job for a reason
# that has nothing to do with traceability. Fall back to the single tip
# commit in that case.
push_base() {
    local before="$1" head="$2"
    if [[ -z "${before}" ]] ||
        [[ "${before}" =~ ^0+$ ]] ||
        ! git rev-parse --verify --quiet "${before}^{commit}" >/dev/null; then
        echo "${head}~1"
    else
        echo "${before}"
    fi
}

# Does this commit touch Rust source? `--name-only --format=` prints just the
# paths; a merge commit prints nothing, which is the right answer for it.
commit_touches_rust() {
    git show --name-only --format= "$1" | rg --quiet '\.rs$'
}

# Every issue closed on or after <since-date>, one number per line. Needs
# `gh`. `gh`'s `closed:` qualifier has DAY granularity, which is why callers
# must widen the citation range they pair this with rather than assuming the
# closed set lines up with a commit range.
closed_issues_since() {
    gh issue list --state closed --limit 500 \
        --search "closed:>=$1" --json number --jq '.[].number' | sort -n
}

# Of the issue numbers passed as arguments, print those that the haystack on
# stdin does NOT cite with a closing keyword. Shared by `--window` and
# `--push`. Same here-string contract as `commit_cites_issue` — and the
# haystack is slurped up front so the per-issue `rg --quiet` calls below
# cannot take SIGPIPE from a writer that is still going.
uncited_among() {
    local messages issue
    messages="$(cat)"
    for issue in "$@"; do
        commit_cites_issue "${issue}" <<<"${messages}" && continue
        printf '%s\n' "${issue}"
    done
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
    # --- push mode (#3504) ---
    if ! commit_mentions_issue <<<'refactor(core): bounded walk (#12)'; then
        echo "check-issue-traceability: self-test missed a bare #N mention" >&2
        exit 1
    fi
    if commit_mentions_issue <<<'refactor(core): bounded walk'; then
        echo "check-issue-traceability: self-test invented a citation" >&2
        exit 1
    fi
    # A commit message may legitimately contain a `#` that is not an issue
    # reference (a Markdown heading, a shell comment in a quoted block).
    if commit_mentions_issue <<<'docs: document the # sigil'; then
        echo "check-issue-traceability: self-test read a bare # as an issue" >&2
        exit 1
    fi
    mapfile -t sample_uncited < <(
        uncited_among 12 34 56 <<<$'Fix #12\nrefactor: touches #34 without a keyword'
    )
    if [[ "${sample_uncited[*]}" != "34 56" ]]; then
        echo "check-issue-traceability: self-test mis-split the uncited set \
(got '${sample_uncited[*]}')" >&2
        exit 1
    fi
    zero_sha='0000000000000000000000000000000000000000'
    if [[ "$(push_base "${zero_sha}" HEAD)" != 'HEAD~1' ]]; then
        echo "check-issue-traceability: self-test did not fall back on a created branch" >&2
        exit 1
    fi
    if [[ "$(push_base '' HEAD)" != 'HEAD~1' ]]; then
        echo "check-issue-traceability: self-test did not fall back on an empty before-SHA" >&2
        exit 1
    fi
    if [[ "$(push_base 'deadbeefdeadbeefdeadbeefdeadbeefdeadbeef' HEAD)" != 'HEAD~1' ]]; then
        echo "check-issue-traceability: self-test did not fall back on a force-pushed-away SHA" >&2
        exit 1
    fi
    real_base="$(git rev-parse HEAD~1)"
    if [[ "$(push_base "${real_base}" HEAD)" != "${real_base}" ]]; then
        echo "check-issue-traceability: self-test rewrote a usable before-SHA" >&2
        exit 1
    fi
    # `commit_touches_rust` against a scratch repo rather than this one's
    # tip: whether the last commit here happens to touch Rust is not a
    # property the self-test should depend on.
    scratch="$(mktemp -d)"
    (
        cd "${scratch}"
        git init -q .
        : >notes.md && git add notes.md
        git -c user.email=t@t -c user.name=t commit -qm 'docs: notes'
        : >lib.rs && git add lib.rs
        git -c user.email=t@t -c user.name=t commit -qm 'feat: lib'
        commit_touches_rust HEAD || {
            echo "check-issue-traceability: self-test missed a .rs commit" >&2
            exit 1
        }
        if commit_touches_rust HEAD~1; then
            echo "check-issue-traceability: self-test called a docs commit Rust" >&2
            exit 1
        fi
    )
    rm -rf "${scratch}"

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
    mapfile -t closed < <(closed_issues_since "${since%T*}")
    if [[ "${#closed[@]}" -eq 0 ]]; then
        echo "check-issue-traceability: no issues closed in this window"
        exit 0
    fi
    mapfile -t uncited < <(
        uncited_among "${closed[@]}" <<<"$(git log --format='%B' "${base}..${head}")"
    )

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

# Push mode (#3504). #3218 diagnosed the mechanism — the CI job was gated on
# `github.event_name == 'pull_request'`, and this repo's history is
# overwhelmingly direct commits to main, so for the dominant workflow the gate
# never fired at all — and then fixed it with a `--window` report invoked by
# hand at session close. The trigger condition was never changed, so the
# measured gap did not move: 43 of 134 (32%) uncited when #3218 was filed,
# 123 of 400 (31%) ten days later.
#
# This mode is what runs on every push to main. It reports BOTH directions
# over the pushed range:
#
#   issue -> commit  an issue closed in this window that no commit cites
#                    (delegated to `--window`, which already does exactly
#                    this against live `gh` state)
#   commit -> issue  a pushed commit that touches `*.rs` and names no issue
#                    at all — the direction neither `--window` nor `--orphan`
#                    can see, since both start from a set of issues
#
# Findings are emitted as GitHub workflow annotations so they land on the
# commit, in the Actions UI, at push time. It exits 0 on findings by design:
# a push's history is already written, and failing main's CI cannot add a
# citation to a commit that is already on the branch. What changes versus
# #3218 is *when* the signal appears — attached to the push, while the author
# still has the context — instead of being reconstructed by an auditor weeks
# later, or not at all. Enforcement still belongs to the PR path below, which
# is the only point where the message can still be edited.
if [[ "${1:-}" == "--push" ]]; then
    if [[ "$#" -ne 3 ]]; then
        echo "usage: $0 --push <before-sha> <after-sha>" >&2
        exit 2
    fi
    head="$3"
    base="$(push_base "$2" "${head}")"

    echo "check-issue-traceability: push range ${base}..${head}"

    # issue -> commit. Citations are searched over the whole branch, not the
    # pushed range: `gh` can only filter closures by DAY, so a range of one
    # commit would otherwise report every issue closed earlier that day —
    # each already cited by an earlier push — as uncited. Needs `gh`; without
    # it (a fork run with no token) this half is skipped rather than failing.
    if command -v gh >/dev/null 2>&1; then
        pushed_day="$(git log -1 --format=%cI "${base}")"
        mapfile -t closed < <(closed_issues_since "${pushed_day%T*}")
        uncited=()
        if [[ "${#closed[@]}" -gt 0 ]]; then
            mapfile -t uncited < <(
                uncited_among "${closed[@]}" <<<"$(git log --format='%B' "${head}")"
            )
        fi
        if [[ "${#closed[@]}" -eq 0 ]]; then
            echo "check-issue-traceability: no issues closed since ${pushed_day%T*}"
        elif [[ "${#uncited[@]}" -eq 0 ]]; then
            echo "check-issue-traceability: all ${#closed[@]} issue(s) closed since \
${pushed_day%T*} are cited by a commit"
        else
            echo
            echo "ZERO-CITATION SET -- ${#uncited[@]} of ${#closed[@]} issues closed since"
            echo "${pushed_day%T*} have no closing-keyword commit anywhere on this branch."
            for issue in "${uncited[@]}"; do
                title="$(gh issue view "${issue}" --json title --jq .title 2>/dev/null || echo '?')"
                printf '  #%-6s %s\n' "${issue}" "${title}"
                echo "::warning title=Closed issue with no citing commit::#${issue} ${title}"
            done
            echo
        fi
    else
        echo "check-issue-traceability: no gh CLI, skipping the closed-issue half"
    fi

    # commit -> issue.
    uncited_commits=()
    while read -r sha; do
        [[ -n "${sha}" ]] || continue
        commit_touches_rust "${sha}" || continue
        commit_mentions_issue <<<"$(git log -1 --format='%B' "${sha}")" && continue
        uncited_commits+=("${sha}")
    done < <(git log --format='%H' "${base}..${head}")

    if [[ "${#uncited_commits[@]}" -eq 0 ]]; then
        echo "check-issue-traceability: every .rs-touching commit in this range names an issue"
        exit 0
    fi

    echo
    echo "UNCITED FIX SET -- ${#uncited_commits[@]} commit(s) touch a .rs file and name no"
    echo "issue at all. These are invisible to both existing audit modes, which start"
    echo "from a set of issues: no PR body declares them, and if the work was never"
    echo "filed, no closed-issue sweep will find it either."
    for sha in "${uncited_commits[@]}"; do
        subject="$(git log -1 --format='%s' "${sha}")"
        printf '  %s  %s\n' "${sha:0:12}" "${subject}"
        echo "::warning title=Fix with no issue reference::${sha:0:12} ${subject}"
    done
    # Advisory — see the block comment above.
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
    echo "       $0 --push   <before-sha> <after-sha>      # push-to-main annotations" >&2
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
