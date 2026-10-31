"""Closed issues described as open (ADVISORY — #6383, precision pass #6525).

Run by `_audit-validate.sh`:   python3 -I _audit-issue-advisory.py STATES FILE...
Self-test (`_audit-validate.sh --selftest` runs it):   python3 -I _audit-issue-advisory.py --selftest

STATES is a file of `<number> <OPEN|CLOSED>` lines. A hit is a `#N` that GitHub says is
CLOSED but that sits in prose carrying an "is still open" cue. The first version fired 11
times at HEAD and every one was a correct past-tense citation (#6525), for four reasons the
rules below answer:

1. List items. "fixed …: #A (…), #B (…)" — the second and later numbers are as fixed as the
   first. A fixed/closed word anywhere EARLIER IN THE SAME SENTENCE resolves every `#N` after it.
2. "don't re-file" is the house idiom for a closed fix ("Fixed … — verify, don't re-file"), so it
   is a cue only when the sentence says nothing of fixed/closed. "known open" / "still open" /
   "stays open" are not softened that way.
3. The cue window must not span clauses: it is the sentence holding the number (split on
   `. ` / `; `), not a fixed 160 characters either side.
4. "closed" AFTER the number ("#5367 (…) is closed") resolves it too.
"""
import re
import sys

# "known open" / "still open" / "stays open" always mean the issue is not done.
OPEN_CUE = re.compile(r"known[- ]open|still[- ]open|stays? open", re.I)
# "do not re-file" only means "open" when nothing in the sentence says the work is done.
REFILE_CUE = re.compile(r"do not re-file|don't re-file", re.I)
DONE = re.compile(r"\b(?:fixed|closed|resolved|shipped|retired|landed|superseded)\b", re.I)
# "open" as a verb, or part of a compound, is never a cue.
NOT_CUE = re.compile(r"fail-open|half-open|open-ended|open[- ]source|open conversation", re.I)
NUM = re.compile(r"#(\d{3,6})\b")
# A sentence break: a period or semicolon followed by whitespace. (A date like 2026-10-19 or
# a file name like `mesh.rs` has no whitespace after its dot, so it does not split.)
BREAK = re.compile(r"[.;]\s")


def sentence_bounds(line, at):
    """(start, end) of the sentence of `line` that contains offset `at`."""
    start = 0
    for m in BREAK.finditer(line, 0, at):
        start = m.end()
    m = BREAK.search(line, at)
    return start, (m.start() + 1 if m else len(line))


def is_hit(line, m):
    """Whether the `#N` match `m` in `line` is a closed issue described as open."""
    start, end = sentence_bounds(line, m.start())
    sentence = NOT_CUE.sub("", line[start:end])
    before = NOT_CUE.sub("", line[start : m.start()])
    after = NOT_CUE.sub("", line[m.end() : end])
    # A fixed/closed word earlier in the sentence already says it (rule 1), and so does one
    # that directly follows the number: `#5367 (…) is closed` (rule 4).
    if DONE.search(before):
        return False
    if re.match(r"[^#;]{0,160}?\b(?:is|are|was|were|now)\s+(?:closed|fixed|resolved)\b", after, re.I):
        return False
    # `open #N`, or "open" within 25 non-sentence characters after `#N`.
    tail = re.match(r"[^.;]{0,25}\bopen\b", line[m.end() : m.end() + 40], re.I)
    adjacent = re.search(r"\bopen $", line[max(0, m.start() - 6) : m.start()], re.I) or tail
    cued = bool(OPEN_CUE.search(sentence))
    if REFILE_CUE.search(sentence) and not DONE.search(sentence):
        cued = True
    return cued or bool(adjacent)


def scan_line(line, states):
    """Issue numbers on `line` that are CLOSED yet described as open."""
    return [
        m.group(1)
        for m in NUM.finditer(line)
        if states.get(m.group(1)) == "CLOSED" and is_hit(line, m)
    ]


def selftest():
    closed = {str(n): "CLOSED" for n in (5492, 5493, 5367, 5429, 5433, 6098, 6258, 3638, 5128, 5447,
                                          5538, 6365, 5967, 4999, 5000)}
    # (label, line, expected hit numbers). The first eleven are the #6525 false positives,
    # shortened to the structure that fooled the first version.
    cases = [
        ("list: fixed … #A / #B", "SCOL/PKIN `FLTR` — all since fixed, #5493 `136048a95` / #5492 `dae08000d`) — open the xEdit definition", []),
        ("fixed + don't re-file + list",
         "**Fixed since 2026-10-08 - verify, don't re-file**: #6098 (a city cell takes its own `.lod`), #6258 (a clause)", []),
        ("regression guard, do not re-file", "- **Fixed 2026-10-19 — regression guard, do not re-file**: #5429 (FO3 floor)", []),
        ("closed … forward scope, don't re-file",
         "Known and deferred — #3638 closed documentation-only (a decal-rendering feature build); forward scope, don't re-file.", []),
        ("closed AFTER the number", "**Known-open (verified)**: #4232 x. #5367 (greeting G) is closed, and so are its follow-ups: #5429 (FO3 pinned)", []),
        ("known-open about another subject, then a past-tense clause",
         "- #5538 contain explosions; the upstream instability stays open as root cause with no issue of its own. Two are fixed (#5538)", []),
        ("other clause says stays open", "The residual stays open (#5128 is a different thing); the known-open one is separate", ["5128"]),
        ("list under a Fixed lead after a different sentence",
         "Something is known-open. Fixed since the sync: #5447 (a), #5967 (b)", []),
        # True positives: a closed issue genuinely listed as open.
        ("known-open, plainly", "**Known-open (verified)**: #5000 the HNAM doc rot", ["5000"]),
        ("still open, plainly", "Cite #4999 (still open in the tracker) rather than re-filing.", ["4999"]),
        ("`open #N`", "see open #5000 for the fix", ["5000"]),
    ]
    failures = 0
    for label, line, want in cases:
        got = scan_line(line, closed)
        if got != want:
            failures += 1
            print(f"SELFTEST FAIL: issue advisory — {label}: expected {want}, got {got}")
        else:
            print(f"ok: issue advisory — {label}")
    return failures


def main(argv):
    if argv[1:2] == ["--selftest"]:
        return 1 if selftest() else 0
    states = dict(line.split() for line in open(argv[1]))
    hits = []
    for path in argv[2:]:
        try:
            lines = open(path, encoding="utf-8", errors="replace").read().splitlines()
        except OSError:
            continue
        for lineno, line in enumerate(lines, 1):
            for n in scan_line(line, states):
                hits.append((path, lineno, n))
    seen = set()
    rows = [h for h in hits if not (h in seen or seen.add(h))]
    if rows:
        print()
        print('ADVISORY — closed issues described as open (an "open" cue in the same sentence):')
        for path, lineno, n in rows:
            print(f"  {path}:{lineno} #{n} CLOSED")
        print(f"  {len(rows)} citation(s). Re-read each: if the work is genuinely unbuilt, point it at a")
        print('  live tracker; if it was fixed, drop the "known open" framing. Not a failure.')
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
