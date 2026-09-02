#!/usr/bin/env python3
"""Flag a closed issue whose enumerated acceptance criteria outnumber the
closing-keyword commits that cite it.

The existing `check-issue-traceability.sh` answers "was this closed issue
cited by ANY closing commit at all" — it found 95 zero-citation closes in the
2026-08-16..20 window, all of them fine on inspection. This script answers a
different question that gate cannot see: a closed issue *with* a citation,
where the issue's own body enumerates several acceptance criteria and only
one (or a few) of them are actually accounted for by the commits that closed
it. #2372 (EX-16) is the motivating case — six acceptance bullets, closed by
one commit that split five of them into follow-up issues rather than
implementing them; #2372 was later audited by hand and found 0 of 6 done.

Detection has no way to know which criterion a commit addresses — matching a
sentence in an issue body against a commit message is not reliable enough to
build a hard gate on, and this repo's own `feedback_audit_finding_hygiene`
memory says as much about premise-checking generally. So this is a *coverage*
proxy, not a semantic one: count the criteria the issue body enumerates,
count the distinct closing-keyword commits that cite the issue, and flag
when there are fewer commits than criteria. That is deliberately permissive
in the safe direction (many single-commit closes legitimately satisfy every
criterion in one shot; those are the expected false positives) and useless
in the other one (a wordy multi-paragraph commit that closes all six
criteria in one message still shows as "1 commit" and gets flagged) — so
the output is a review queue, not a verdict. See `_audit-common`'s citation
gate for the sibling zero-citation check.

Recognizing "acceptance criteria": only ~33 of ~3450 closed issues at the
time this was written carry an `Acceptance` / `Acceptance Criteria` heading
(with or without leading `#`s — the older epic-plan template, e.g. #2372,
uses a bare `Acceptance` line; newer issues use `## Acceptance Criteria`)
followed by a bullet, numbered, or checkbox list. Every other closed issue
in the tracker uses `## Completeness Checks` instead (3180 of them), which
is a per-finding audit checklist (SIBLING/TESTS/DOCS/...), not an epic's
acceptance criteria, and is deliberately NOT treated as a criteria list
here. Both recognized shapes are internally consistent everywhere they
appear, so this script recognizes them rather than guessing at prose.
Recommended convention going forward: always `## Acceptance Criteria`
(the `#`-heading form, not the bare-line form), one bullet per criterion,
each phrased as a single independently-verifiable claim — that is the
minimum a tool (or a person) can score without re-reading the whole issue.

Usage:
    check-acceptance-coverage.py [--limit N] [--min-criteria N] [--json FILE]

Needs the `gh` CLI, authenticated, run from inside the repo (uses `git log`
on the current branch for citations).
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from dataclasses import dataclass, field

HEADING_RE = re.compile(r"^#{0,4}\s*acceptance(\s+criteria)?\s*:?\s*$", re.IGNORECASE)
ANY_HEADING_RE = re.compile(r"^#{1,4}\s+")
CHECKBOX_ITEM_RE = re.compile(r"^\s*[-*]\s*\[[ xX]\]\s+\S")
NUMBERED_ITEM_RE = re.compile(r"^\s*\d+[.)]\s+\S")
BULLET_ITEM_RE = re.compile(r"^\s*[-*]\s+\S")

CLOSING_KEYWORDS = (
    r"fix|fixes|fixed|close|closes|closed|resolve|resolves|resolved"
)


def extract_criteria_section(body: str) -> list[str] | None:
    """Return the list-item lines under an Acceptance(-Criteria) heading, or
    None if the body has no such heading."""
    lines = body.splitlines()
    start = None
    for i, line in enumerate(lines):
        if HEADING_RE.match(line.strip()):
            start = i + 1
            break
    if start is None:
        return None

    items: list[str] = []
    for line in lines[start:]:
        stripped = line.rstrip()
        if ANY_HEADING_RE.match(stripped):
            break
        if (
            CHECKBOX_ITEM_RE.match(stripped)
            or NUMBERED_ITEM_RE.match(stripped)
            or BULLET_ITEM_RE.match(stripped)
        ):
            items.append(stripped.strip())
    return items


def citing_commit_count(issue_number: int, commit_log: str) -> int:
    """Distinct commits (by the %x00-delimited record) whose message uses a
    closing keyword against this issue number. Mirrors
    `check-issue-traceability.sh`'s `commit_cites_issue` regex exactly so the
    two scripts agree on what counts as a citation."""
    # `\s+` (not `[ \t]+`) between the keyword and the `#N` — commit bodies
    # wrap, and `check-issue-traceability.sh`'s `[[:space:]]+` already
    # crosses a line break, so this must too or the two scripts disagree
    # about what counts as a citation.
    pattern = re.compile(
        rf"(^|[^A-Za-z0-9_])({CLOSING_KEYWORDS})\s+#{issue_number}([^0-9]|$)",
        re.IGNORECASE | re.MULTILINE,
    )
    count = 0
    for record in commit_log.split("\x00"):
        if pattern.search(record):
            count += 1
    return count


@dataclass
class Finding:
    number: int
    title: str
    closed_at: str
    criteria: list[str]
    citing_commits: int
    detail: list[str] = field(default_factory=list)


def run_gh_closed_issues(limit: int) -> list[dict]:
    out = subprocess.run(
        [
            "gh",
            "issue",
            "list",
            "--state",
            "closed",
            "--limit",
            str(limit),
            "--json",
            "number,title,body,closedAt",
        ],
        check=True,
        capture_output=True,
        text=True,
    )
    return json.loads(out.stdout)


def run_git_commit_log() -> str:
    out = subprocess.run(
        ["git", "log", "--format=%x00%H %B"],
        check=True,
        capture_output=True,
        text=True,
    )
    return out.stdout


def self_test() -> int:
    # A heading with 3 bullets, 1 citing commit -> under-covered.
    body_heading = "Problem\nsomething.\n\n## Acceptance Criteria\n- one\n- two\n- three\n"
    items = extract_criteria_section(body_heading)
    assert items == ["- one", "- two", "- three"], items

    # #2372's actual shape: no `#`, just a bare "Acceptance" line.
    body_bare = "Plan.\n\nAcceptance\n- a\n- b\n\nDepends on X.\n"
    items = extract_criteria_section(body_bare)
    assert items == ["- a", "- b"], items

    # Numbered and checkbox variants both count.
    assert extract_criteria_section("## Acceptance\n1. a\n2. b\n") == ["1. a", "2. b"]
    assert extract_criteria_section(
        "## Acceptance criteria\n- [ ] a\n- [x] b\n"
    ) == ["- [ ] a", "- [x] b"]

    # A "Completeness Checks" heading (the dominant per-finding template)
    # must NOT be mistaken for acceptance criteria.
    assert extract_criteria_section("## Completeness Checks\n- [ ] SIBLING\n") is None

    # No heading at all.
    assert extract_criteria_section("## Description\nnothing here.\n") is None

    # The list stops at the next heading, even mid-list.
    items = extract_criteria_section("## Acceptance\n- a\n- b\n## Evidence\n- c\n")
    assert items == ["- a", "- b"], items

    # Citation matching crosses a line wrap (commit bodies wrap at ~72
    # cols) and requires the closing keyword immediately before `#N`, not
    # just anywhere in the same commit — mirrors
    # `check-issue-traceability.sh`'s `commit_cites_issue`.
    log = "\x00deadbeef Fix\n#42: bounded walk\n"
    assert citing_commit_count(42, log) == 1
    log = "\x00deadbeef mentions #42 in passing\n"
    assert citing_commit_count(42, log) == 0
    log = "\x00deadbeef Fix #421 not 42\n"
    assert citing_commit_count(42, log) == 0
    log = "\x00aaa Fix #42\n\x00bbb Also fix #42 differently\n"
    assert citing_commit_count(42, log) == 2

    print("check-acceptance-coverage: self-test passed")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--self-test", action="store_true", help="run built-in self-tests and exit")
    ap.add_argument("--limit", type=int, default=4000, help="max closed issues to fetch")
    ap.add_argument(
        "--min-criteria",
        type=int,
        default=2,
        help="minimum list items to treat a section as a criteria list (default 2)",
    )
    ap.add_argument("--json", help="also write the full closed-issue set to this file")
    ap.add_argument(
        "--issues-json",
        help="read closed issues from this file instead of calling gh (for repeat runs)",
    )
    args = ap.parse_args()

    if args.self_test:
        return self_test()

    if args.issues_json:
        with open(args.issues_json) as f:
            issues = json.load(f)
    else:
        issues = run_gh_closed_issues(args.limit)
        if args.json:
            with open(args.json, "w") as f:
                json.dump(issues, f)

    commit_log = run_git_commit_log()

    with_heading = 0
    findings: list[Finding] = []
    for issue in issues:
        body = issue.get("body") or ""
        items = extract_criteria_section(body)
        if items is None:
            continue
        with_heading += 1
        if len(items) < args.min_criteria:
            continue
        cited = citing_commit_count(issue["number"], commit_log)
        if cited < len(items):
            findings.append(
                Finding(
                    number=issue["number"],
                    title=issue["title"],
                    closed_at=issue.get("closedAt", "?"),
                    criteria=items,
                    citing_commits=cited,
                )
            )

    findings.sort(key=lambda f: (f.criteria.__len__() - f.citing_commits), reverse=True)

    print(
        f"check-acceptance-coverage: {len(issues)} closed issues scanned, "
        f"{with_heading} carry an Acceptance(-Criteria) heading"
    )
    if not findings:
        print("check-acceptance-coverage: no under-cited acceptance lists found")
        return 0

    print()
    print(
        f"REVIEW QUEUE -- {len(findings)} closed issue(s) enumerate more acceptance "
        "criteria than they have closing-keyword commits citing them. This is a"
    )
    print(
        "coverage proxy, not a verdict -- a single commit or comment can legitimately"
    )
    print(
        "satisfy every criterion at once. Check each by hand against the issue's own"
    )
    print("close comment / linked commits.")
    print()
    for f in findings:
        print(
            f"  #{f.number:<6} {f.criteria.__len__()} criteria, {f.citing_commits} "
            f"citing commit(s)  {f.title}"
        )
    return 0


if __name__ == "__main__":
    sys.exit(main())
