#!/usr/bin/env python3
"""Automatic half of the PRD gate — check every `docs/prd/*.md` against the
structural rules in `docs/prd/README.md` (§7 lists the checks by id).

A PRD states the *promise* a feature makes; the design docs under
`docs/engine/` own the *how*. Before a PRD can be APPROVED (the state that
unlocks implementation issues) it must pass this script with zero errors
and a manual review against the rubric in `docs/prd/README.md` §8. This
script never judges whether a requirement is a good one — it checks the
shape a reviewer and a tool can rely on: the header block, the section
set, requirement-id discipline, phase exit criteria, resolvable links and
anchors, the revision log, and the state-dependent rules (an APPROVED PRD
has an `approved` verdict in its log and every decision marked Decided; an
ACTIVE one names its tracking issues).

Errors (`E..`) fail the run. Warnings (`W..`) are the manual-review queue
and do not fail unless `--strict` is passed. `--list` prints the PRD
registry straight from the headers (the one home for "which PRDs exist and
in what state"). `--self-test` builds a conformant PRD in a scratch tree,
mutates it one rule at a time and asserts each rule fires — the CI job runs
it before the real check, like the sibling scripts in this directory.

Usage:
    check-prd-conformance.py [--strict] [--root DIR] [PRD.md ...]
    check-prd-conformance.py --list
    check-prd-conformance.py --self-test

No dependencies beyond the standard library; Python 3.9+.
"""

from __future__ import annotations

import argparse
import re
import sys
import tempfile
from dataclasses import dataclass, field
from pathlib import Path
from typing import Dict, Iterable, List, Optional, Tuple

STATES = ("PROPOSED", "APPROVED", "ACTIVE", "SHIPPED", "WITHDRAWN", "SUPERSEDED")
APPROVED_STATES = ("APPROVED", "ACTIVE", "SHIPPED")
TRACKED_STATES = ("ACTIVE", "SHIPPED")
CLOSED_STATES = ("WITHDRAWN", "SUPERSEDED")
VERDICTS = ("revise", "approved", "withdrawn")

STATUS_RE = re.compile(r"^\*\*Status\*\*:\s*([A-Z]+)\s*\((\d{4}-\d{2}-\d{2})\)")
SCOPE_RE = re.compile(r"^\*\*Scope\*\*:")
RELATED_RE = re.compile(r"^\*\*Related\*\*:")
TRACKING_RE = re.compile(r"^\*\*Tracking\*\*:(.*)$")
SUPERSEDED_RE = re.compile(r"^\*\*(Superseded by|Reason)\*\*:\s*\S")
PREFIX_DECL_RE = re.compile(r"Requirement IDs use the `([A-Z]{2,6})-` prefix")
# A requirement *definition*: a bold id opening a list item or table cell.
# Citations of another document's ids appear mid-sentence and never match.
ID_DEF_RE = re.compile(r"^\s*(?:[-*]|\d+\.|\|)\s*\*\*([A-Z]{2,6}(?:-[A-Z]{1,6})?-\d{3})")
PRD_ID_RE = re.compile(r"^([A-Z]{2,6})-([A-Z]{1,6})-(\d{3})$")
PHASE_RE = re.compile(r"^\*\*Phase\s+\d+")
DECISION_RE = re.compile(r"^\s*\d+\.\s+\*\*Q(\d+)\b")
NO_DECISIONS_RE = re.compile(r"\b(no decisions|none open|no open decisions)\b", re.IGNORECASE)
DECIDED_RE = re.compile(r"\*\*Decided\*\*")
RECOMMEND_RE = re.compile(r"recommend", re.IGNORECASE)
NORMATIVE_RE = re.compile(r"\b(MUST NOT|MUST|SHOULD NOT|SHOULD|MAY)\b")
HEAD_DATE_RE = re.compile(r"HEAD on \d{4}-\d{2}-\d{2}")
TODAY_COLUMN_RE = re.compile(r"^\|.*\|\s*Today\s*\|", re.IGNORECASE)
LINK_RE = re.compile(r"!?\[([^\]]*)\]\(([^)\s]+)(?:\s+\"[^\"]*\")?\)")
CODE_SPAN_RE = re.compile(r"`[^`]*`")
HTML_TAG_RE = re.compile(r"<[^>]+>")
HEADING_RE = re.compile(r"^(#{1,6})\s+(.*?)\s*#*\s*$")
LOG_ROW_RE = re.compile(r"^\|\s*(\d+)\s*\|\s*(\S+)\s*\|(.*)\|(.*)\|\s*$")
ISO_DATE_RE = re.compile(r"^\d{4}-\d{2}-\d{2}$")
ISSUE_REF_RE = re.compile(r"#\d+")
TEMPLATE_TOKENS = ("XXX-", "YYYY-MM-DD", "<slug>", "<Feature name>", "<feature name>")
TBD_RE = re.compile(r"\b(TBD|TODO)\b")
SLUG_RE = re.compile(r"^[a-z0-9]+(?:-[a-z0-9]+)*$")

# Required sections, in template order: (key, matcher over the normalised
# heading text, human name). H2 and H3 both count, so "4. Goals and
# non-goals" with "4.1 Goals" / "4.2 Non-goals" beneath satisfies both.
REQUIRED_SECTIONS: Tuple[Tuple[str, str, str], ...] = (
    ("thesis", "thesis", "Product thesis"),
    ("terminology", "terminolog", "Terminology"),
    ("goals", "goal", "Goals"),
    ("non-goals", "non-goal", "Non-goals"),
    ("requirements", "requirements", "Requirements"),
    ("per-game", "per-game", "Per-game scope"),
    ("acceptance", "acceptance", "Acceptance"),
    ("metrics", "metric", "Metrics"),
    ("delivery", "delivery", "Delivery sequence"),
    ("decisions", "decision", "Decisions required"),
    ("document-map", "document map", "Document map"),
    ("revision-log", "revision", "Revision and review log"),
)


@dataclass
class Finding:
    code: str
    message: str
    line: Optional[int] = None

    @property
    def is_error(self) -> bool:
        return self.code.startswith("E")


@dataclass
class Heading:
    level: int
    text: str
    line: int
    slug: str = ""


@dataclass
class Prd:
    path: Path
    lines: List[str]
    headings: List[Heading] = field(default_factory=list)
    state: str = ""
    date: str = ""
    prefix: str = ""
    ids: List[str] = field(default_factory=list)
    decisions: int = 0
    decided: int = 0
    tracking: List[str] = field(default_factory=list)
    findings: List[Finding] = field(default_factory=list)

    def err(self, code: str, message: str, line: Optional[int] = None) -> None:
        self.findings.append(Finding(code, message, line))

    def warn(self, code: str, message: str, line: Optional[int] = None) -> None:
        self.findings.append(Finding(code, message, line))


# ── Markdown helpers ──────────────────────────────────────────────────────


def strip_fences(lines: List[str]) -> List[str]:
    """Blank out what the rendered page does not show — fenced code blocks
    and HTML comments (the template's guidance) — so headings, ids, links
    and header lines inside them are neither counted nor checked. Line
    numbers are preserved."""
    out: List[str] = []
    in_fence = in_comment = False
    for line in lines:
        if not in_comment and (line.lstrip().startswith("```") or line.lstrip().startswith("~~~")):
            in_fence = not in_fence
            out.append("")
            continue
        if in_fence:
            out.append("")
            continue
        line = re.sub(r"<!--.*?-->", "", line)
        if in_comment:
            if "-->" not in line:
                out.append("")
                continue
            in_comment = False
            line = line.split("-->", 1)[1]
        if "<!--" in line:
            in_comment = True
            line = line.split("<!--", 1)[0]
        out.append(line)
    return out


def github_slug(heading_text: str) -> str:
    """GitHub's heading → anchor rule: render inline markdown to text, lower
    case, drop everything but word characters, hyphens and spaces, turn each
    space into a hyphen. Duplicate handling (`-1`, `-2`) lives in `slugs_of`."""
    text = LINK_RE.sub(lambda m: m.group(1), heading_text)
    text = HTML_TAG_RE.sub("", text)
    text = text.replace("`", "")
    text = text.lower()
    text = re.sub(r"[^\w\- ]", "", text)
    return text.replace(" ", "-")


def parse_headings(lines: List[str]) -> List[Heading]:
    headings: List[Heading] = []
    seen: Dict[str, int] = {}
    for idx, line in enumerate(strip_fences(lines), start=1):
        m = HEADING_RE.match(line)
        if not m:
            continue
        base = github_slug(m.group(2))
        n = seen.get(base, 0)
        seen[base] = n + 1
        slug = base if n == 0 else f"{base}-{n}"
        headings.append(Heading(len(m.group(1)), m.group(2), idx, slug))
    return headings


_SLUG_CACHE: Dict[Path, set] = {}


def slugs_of(path: Path) -> set:
    if path not in _SLUG_CACHE:
        try:
            text = path.read_text(encoding="utf-8")
        except OSError:
            _SLUG_CACHE[path] = set()
        else:
            _SLUG_CACHE[path] = {h.slug for h in parse_headings(text.splitlines())}
    return _SLUG_CACHE[path]


def normalise_heading(text: str) -> str:
    text = re.sub(r"^\d+(\.\d+)*\.?\s+", "", text.strip())
    text = text.replace("`", "").replace("*", "")
    return text.lower()


def section_bounds(prd: Prd, key_substr: str, level: int = 2) -> Optional[Tuple[int, int]]:
    """(start_line, end_line) of the first H<level> whose normalised text
    contains `key_substr`, running to the next heading of the same or a
    higher level. 1-based inclusive start, exclusive end."""
    for i, h in enumerate(prd.headings):
        if h.level == level and key_substr in normalise_heading(h.text):
            end = len(prd.lines) + 1
            for later in prd.headings[i + 1:]:
                if later.level <= level:
                    end = later.line
                    break
            return h.line, end
    return None


def bullets_in(lines: List[str], start: int, end: int) -> List[Tuple[int, str]]:
    """Top-level list items in [start, end): (line, joined text)."""
    items: List[Tuple[int, str]] = []
    current: Optional[Tuple[int, List[str]]] = None
    for idx in range(start, min(end, len(lines) + 1)):
        line = lines[idx - 1]
        if re.match(r"^(?:[-*]|\d+\.)\s+", line):
            if current:
                items.append((current[0], " ".join(current[1])))
            current = (idx, [line])
        elif current and line.startswith(("  ", "\t")):
            current[1].append(line.strip())
        elif current and line.strip() == "":
            items.append((current[0], " ".join(current[1])))
            current = None
        elif current:
            current[1].append(line.strip())
    if current:
        items.append((current[0], " ".join(current[1])))
    return items


# ── Checks ────────────────────────────────────────────────────────────────


def check_location(prd: Prd, root: Path) -> None:
    rel = prd.path.resolve().relative_to(root.resolve()) if prd.path.resolve().is_relative_to(root.resolve()) else prd.path
    if prd.path.parent.resolve() != (root / "docs" / "prd").resolve():
        prd.err("E01", f"PRDs live in docs/prd/, not {rel.parent}/ (move it; the directory is the type)")
    stem = prd.path.stem
    if not SLUG_RE.match(stem):
        prd.err("E01", f"file name `{stem}` is not a kebab-case slug")
    if stem.endswith("-prd"):
        prd.err("E01", f"drop the `-prd` suffix from `{stem}`; docs/prd/ already says what it is")


def check_header(prd: Prd) -> None:
    body = strip_fences(prd.lines)
    first = next((l for l in body if l.strip()), "")
    if not first.startswith("# ") or not first.rstrip().endswith("— Product Requirements"):
        prd.err("E02", "first line must be an H1 ending in `— Product Requirements`", 1)
    status_line = next(((i, l) for i, l in enumerate(body, 1) if l.startswith("**Status**")), None)
    if not status_line:
        prd.err("E03", "missing `**Status**: STATE (YYYY-MM-DD)` line")
    else:
        m = STATUS_RE.match(status_line[1])
        if not m:
            prd.err("E03", "status line must read `**Status**: STATE (YYYY-MM-DD)` before any prose", status_line[0])
        else:
            prd.state, prd.date = m.group(1), m.group(2)
            if prd.state not in STATES:
                prd.err("E03", f"status `{prd.state}` is not one of {', '.join(STATES)}", status_line[0])
    if not any(SCOPE_RE.match(l) for l in body):
        prd.err("E04", "missing `**Scope**:` line")
    if not any(RELATED_RE.match(l) for l in body):
        prd.err("E05", "missing `**Related**:` line (the owning designs this PRD states the promise for)")
    for i, l in enumerate(body, 1):
        m = TRACKING_RE.match(l)
        if m:
            prd.tracking = ISSUE_REF_RE.findall(m.group(1))
            if not prd.tracking:
                prd.err("E15", "`**Tracking**:` names no `#NNNN` issue", i)
    text = "\n".join(body)
    m = PREFIX_DECL_RE.search(text)
    if not m:
        prd.err("E06", "missing the declaration sentence: Requirement IDs use the `XXX-` prefix (2–6 capitals)")
    else:
        prd.prefix = m.group(1)
    for i, l in enumerate(body, 1):
        for tok in TEMPLATE_TOKENS:
            if tok in l and not (tok == "XXX-" and PREFIX_DECL_RE.search(l) and prd.prefix == "XXX"):
                prd.err("E16", f"template placeholder `{tok}` left in the text", i)
                break
        if TBD_RE.search(l):
            prd.warn("W02", "TBD/TODO in a PRD — mark it `Phase 0: verify` with what would settle it, or decide", i)


def check_sections(prd: Prd) -> None:
    norm = [(h, normalise_heading(h.text)) for h in prd.headings if h.level in (2, 3)]
    starts: Dict[str, int] = {}
    for key, needle, name in REQUIRED_SECTIONS:
        hit = None
        for h, n in norm:
            if key == "goals" and "non-goal" in n:
                continue
            if needle in n:
                hit = h
                break
        if hit is None:
            prd.err("E08", f"missing required section: {name} (heading containing `{needle}`)")
        else:
            starts[key] = hit.line
    # Goals and non-goals are one block ("4. Goals and non-goals" over two
    # H3s), so the order check sees the block, not the pair.
    order: List[int] = []
    goals_block = [starts[k] for k in ("goals", "non-goals") if k in starts]
    for key, _, _ in REQUIRED_SECTIONS:
        if key == "goals" and goals_block:
            order.append(min(goals_block))
        elif key not in ("goals", "non-goals") and key in starts:
            order.append(starts[key])
    if order and order != sorted(order):
        prd.warn("W03", "required sections are not in template order (see docs/prd/TEMPLATE.md)")


def check_ids(prd: Prd, root: Path) -> None:
    seen: Dict[str, int] = {}
    body = strip_fences(prd.lines)
    for i, l in enumerate(body, 1):
        m = ID_DEF_RE.match(l)
        if not m:
            continue
        rid = m.group(1)
        prd.ids.append(rid)
        pm = PRD_ID_RE.match(rid)
        if not pm:
            prd.err("E07", f"`{rid}` is not of the form PREFIX-AREA-NNN", i)
            continue
        if prd.prefix and pm.group(1) != prd.prefix:
            prd.err("E07", f"`{rid}` does not use the declared `{prd.prefix}-` prefix", i)
        if rid in seen:
            prd.err("E07", f"`{rid}` is defined twice (first at line {seen[rid]})", i)
        seen.setdefault(rid, i)
    if prd.prefix and not prd.ids:
        prd.err("E07", f"no requirement defined with the `{prd.prefix}-` prefix (bold id opening a list item)")
    # Prefix collision with another PRD or a design document's id family.
    if prd.prefix:
        owners = id_family_owners(root)
        others = sorted(p for p in owners.get(prd.prefix, set()) if p.resolve() != prd.path.resolve())
        if others:
            names = ", ".join(str(p.relative_to(root)) for p in others)
            prd.err("E06", f"prefix `{prd.prefix}-` is already used by {names}; pick another")


_FAMILY_CACHE: Dict[Path, Dict[str, set]] = {}


def id_family_owners(root: Path) -> Dict[str, set]:
    """Map every requirement-id first segment (`LMC`, `FIX`, `SEC`, …) to the
    documents that *define* ids in it, across docs/prd and docs/engine."""
    if root in _FAMILY_CACHE:
        return _FAMILY_CACHE[root]
    owners: Dict[str, set] = {}
    for folder in ("docs/prd", "docs/engine"):
        for path in sorted((root / folder).glob("*.md")):
            if path.name in ("README.md", "TEMPLATE.md"):
                continue
            try:
                lines = path.read_text(encoding="utf-8").splitlines()
            except OSError:
                continue
            for l in strip_fences(lines):
                m = ID_DEF_RE.match(l)
                if m:
                    owners.setdefault(m.group(1).split("-")[0], set()).add(path)
    _FAMILY_CACHE[root] = owners
    return owners


# R1: an ID retired in place as "(withdrawn, rev N)" keeps its slot and states no requirement.
WITHDRAWN_RE = re.compile(r"\(withdrawn, rev \d+\)", re.IGNORECASE)


def check_requirements_wording(prd: Prd) -> None:
    bounds = section_bounds(prd, "requirements")
    if not bounds:
        return
    for line, text in bullets_in(strip_fences(prd.lines), *bounds):
        m = ID_DEF_RE.match(text)
        if m and not NORMATIVE_RE.search(text) and not WITHDRAWN_RE.search(text):
            prd.warn("W01", f"`{m.group(1)}` states no MUST / SHOULD / MAY — is it a requirement or a note?", line)


def check_phases(prd: Prd) -> None:
    bounds = section_bounds(prd, "delivery")
    if not bounds:
        return
    body = strip_fences(prd.lines)
    start, end = bounds
    phases: List[Tuple[int, List[str]]] = []
    for idx in range(start + 1, end):
        line = body[idx - 1]
        if PHASE_RE.match(line):
            phases.append((idx, [line]))
        elif phases and line.strip():
            phases[-1][1].append(line)
    if not phases:
        prd.err("E09", "delivery sequence defines no `**Phase N — …**` paragraph", start)
    for line, text in phases:
        if "Exit:" not in " ".join(text):
            prd.err("E09", "phase has no `Exit:` criterion", line)


def check_decisions(prd: Prd) -> None:
    bounds = section_bounds(prd, "decision")
    if not bounds:
        return
    body = strip_fences(prd.lines)
    items = [(l, t) for l, t in bullets_in(body, *bounds) if DECISION_RE.match(t)]
    prd.decisions = len(items)
    prd.decided = sum(1 for _, t in items if DECIDED_RE.search(t))
    section_text = "\n".join(body[bounds[0] - 1: bounds[1] - 1])
    if not items and not NO_DECISIONS_RE.search(section_text):
        prd.err("E10", "decisions section lists no `N. **Qn — …**` item and does not say there are none", bounds[0])
    for line, text in items:
        if not RECOMMEND_RE.search(text) and not DECIDED_RE.search(text):
            prd.warn("W04", "decision names no recommendation (the recommendation is the first option)", line)
    if prd.state in APPROVED_STATES and prd.decided < prd.decisions:
        prd.err(
            "E15",
            f"{prd.state} requires every decision to carry `**Decided** (YYYY-MM-DD): …`; "
            f"{prd.decisions - prd.decided} of {prd.decisions} still open",
            bounds[0],
        )


def check_links(prd: Prd) -> None:
    own_slugs = {h.slug for h in prd.headings}
    for i, raw in enumerate(strip_fences(prd.lines), 1):
        line = CODE_SPAN_RE.sub("", raw)
        for m in LINK_RE.finditer(line):
            target = m.group(2)
            if target.startswith(("http://", "https://", "mailto:")):
                continue
            path_part, _, anchor = target.partition("#")
            if path_part == "":
                if anchor and anchor not in own_slugs:
                    prd.err("E11", f"anchor `#{anchor}` matches no heading in this document", i)
                continue
            resolved = (prd.path.parent / path_part).resolve()
            if not resolved.exists():
                prd.err("E11", f"link target `{path_part}` does not exist", i)
                continue
            if anchor and resolved.suffix == ".md" and anchor not in slugs_of(resolved):
                prd.err("E11", f"anchor `#{anchor}` matches no heading in {path_part}", i)


def check_head_date(prd: Prd) -> None:
    body = strip_fences(prd.lines)
    if any(TODAY_COLUMN_RE.match(l) for l in body) and not any(HEAD_DATE_RE.search(l) for l in body):
        prd.err("E12", "a `Today` column needs a dated anchor: say `at HEAD on YYYY-MM-DD` near the table")


def check_index(prd: Prd, root: Path) -> None:
    index = root / "docs" / "engine" / "index.md"
    try:
        text = index.read_text(encoding="utf-8")
    except OSError:
        prd.err("E13", "docs/engine/index.md is missing")
        return
    if f"../prd/{prd.path.name}" not in text:
        prd.err("E13", f"docs/engine/index.md has no row linking ../prd/{prd.path.name}")


def check_revision_log(prd: Prd) -> None:
    bounds = section_bounds(prd, "revision")
    if not bounds:
        return
    body = strip_fences(prd.lines)
    start, end = bounds
    header_ok = False
    rows: List[Tuple[int, int, str, str]] = []
    for idx in range(start, end):
        line = body[idx - 1]
        if line.startswith("|") and not header_ok:
            cells = [c.strip().lower() for c in line.strip("|").split("|")]
            if cells[:4] == ["rev", "date", "change", "review"]:
                header_ok = True
            continue
        m = LOG_ROW_RE.match(line)
        if m:
            rows.append((idx, int(m.group(1)), m.group(2), m.group(4).strip()))
    if not header_ok:
        prd.err("E14", "revision log needs a table headed `| Rev | Date | Change | Review |`", start)
        return
    if not rows:
        prd.err("E14", "revision log has no rows", start)
        return
    expected = 1
    for line, rev, date, review in rows:
        if rev != expected:
            prd.err("E14", f"revision numbers must run 1, 2, 3 … (found {rev}, expected {expected})", line)
        expected = rev + 1
        if not ISO_DATE_RE.match(date):
            prd.err("E14", f"revision date `{date}` is not YYYY-MM-DD", line)
        verdict = review.split(":")[-1].strip().lower().rstrip(".")
        if review not in ("—", "-", "") and verdict not in VERDICTS:
            prd.err("E14", f"review cell `{review}` must be `—` or `<who>: revise|approved|withdrawn`", line)
    last_line, _, last_date, _ = rows[-1]
    if prd.date and last_date != prd.date:
        prd.err("E14", f"last log row is dated {last_date} but the status line says {prd.date}; they move together", last_line)
    verdicts = {r[3].split(":")[-1].strip().lower().rstrip(".") for r in rows}
    if prd.state in APPROVED_STATES and "approved" not in verdicts:
        prd.err("E15", f"{prd.state} requires a log row whose review cell ends in `approved`")
    if prd.state in TRACKED_STATES and not prd.tracking:
        prd.err("E15", f"{prd.state} requires a `**Tracking**: #NNNN …` header line")
    if prd.state in CLOSED_STATES and not any(SUPERSEDED_RE.match(l) for l in body):
        prd.err("E15", f"{prd.state} requires a `**Superseded by**:` link or a `**Reason**:` line")


def check_prd(path: Path, root: Path) -> Prd:
    lines = path.read_text(encoding="utf-8").splitlines()
    prd = Prd(path=path, lines=lines, headings=parse_headings(lines))
    check_location(prd, root)
    check_header(prd)
    check_sections(prd)
    check_ids(prd, root)
    check_requirements_wording(prd)
    check_phases(prd)
    check_decisions(prd)
    check_links(prd)
    check_head_date(prd)
    check_index(prd, root)
    check_revision_log(prd)
    return prd


# ── Discovery and reporting ───────────────────────────────────────────────


def discover(root: Path) -> Tuple[List[Path], List[Path]]:
    """(PRDs in docs/prd, misplaced `*-prd.md` files elsewhere under docs)."""
    prd_dir = root / "docs" / "prd"
    prds = sorted(p for p in prd_dir.glob("*.md") if p.name not in ("README.md", "TEMPLATE.md")) if prd_dir.is_dir() else []
    misplaced = sorted(p for p in (root / "docs").rglob("*-prd.md") if prd_dir not in p.parents)
    return prds, misplaced


def report(prd: Prd, root: Path, strict: bool) -> Tuple[int, int]:
    rel = prd.path.relative_to(root) if prd.path.is_relative_to(root) else prd.path
    errors = [f for f in prd.findings if f.is_error]
    warnings = [f for f in prd.findings if not f.is_error]
    verdict = "FAIL" if errors or (strict and warnings) else "ok"
    print(f"{rel}: {verdict} — {prd.state or '?'} ({prd.date or '?'}), prefix {prd.prefix or '?'}, "
          f"{len(prd.ids)} ids, {prd.decisions} decisions ({prd.decided} decided), "
          f"{len(errors)} errors, {len(warnings)} warnings")
    for f in sorted(errors, key=lambda f: (f.line or 0, f.code)):
        where = f"{rel}:{f.line}" if f.line else str(rel)
        print(f"  {f.code} {where}: {f.message}")
    if strict:
        for f in sorted(warnings, key=lambda f: (f.line or 0, f.code)):
            where = f"{rel}:{f.line}" if f.line else str(rel)
            print(f"  {f.code} {where}: {f.message}")
    else:
        by_code: Dict[str, List[Finding]] = {}
        for f in warnings:
            by_code.setdefault(f.code, []).append(f)
        for code, group in sorted(by_code.items()):
            first = next((g for g in group if g.line), group[0])
            at = f", first at {rel}:{first.line}" if first.line else ""
            print(f"  {code} ×{len(group)}: {group[0].message}{at} (run with --strict to list all)")
    return len(errors), len(warnings)


def list_registry(root: Path) -> int:
    prds, _ = discover(root)
    print(f"{'PRD':<40} {'State':<10} {'Date':<11} {'Prefix':<7} {'IDs':>4} {'Decisions':>12}  Tracking")
    for path in prds:
        prd = check_prd(path, root)
        dec = f"{prd.decided}/{prd.decisions} decided"
        print(f"{path.stem:<40} {prd.state:<10} {prd.date:<11} {prd.prefix:<7} {len(prd.ids):>4} {dec:>12}  {' '.join(prd.tracking) or '—'}")
    return 0


def run(root: Path, paths: Iterable[Path], strict: bool) -> int:
    prds, misplaced = discover(root)
    selected = list(paths) or prds
    total_err = total_warn = 0
    for p in misplaced:
        total_err += 1
        print(f"{p.relative_to(root)}: FAIL\n  E01 {p.relative_to(root)}: a `*-prd.md` outside docs/prd/ — move it there and drop the suffix")
    if not selected and not misplaced:
        print("check-prd-conformance: no PRDs under docs/prd/ (nothing to check)")
        return 0
    for path in selected:
        e, w = report(check_prd(path, root), root, strict)
        total_err += e
        total_warn += w
    failed = total_err or (strict and total_warn)
    print(f"check-prd-conformance: {len(selected)} PRD(s), {total_err} error(s), {total_warn} warning(s)"
          + (" — see docs/prd/README.md §7 for the rule behind each code" if failed else ""))
    return 1 if failed else 0


# ── Self-test ─────────────────────────────────────────────────────────────

GOOD_PRD = """# Widget Streaming — Product Requirements

**Status**: PROPOSED (2026-11-01). First draft.

**Scope**: What it means to stream widgets.

**Related**: [Design](../engine/design.md).

Normative terms **MUST**, **SHOULD** and **MAY** have their RFC 2119
meanings. Requirement IDs use the `WDG-` prefix.

## 1. Product thesis

Why this exists.

## 2. Terminology

| Term | Meaning |
|---|---|
| Widget | A thing |

## 3. Core model

The model, with a table dated at HEAD on 2026-11-01.

| # | Mechanism | Disp. | Today |
|---|---|---|---|
| W1 | Load | D1 | shipped |

## 4. Goals and non-goals

### 4.1 Goals

- **WDG-G-001.** Widgets MUST stream. See [design §1](../engine/design.md#1-overview).

### 4.2 Non-goals

- **WDG-NG-001.** Not sprockets.

## 5. Requirements

### 5.1 Loading (`WDG-LD`)

- **WDG-LD-001.** A widget MUST load within budget.
- **WDG-LD-002.** A widget SHOULD unload when far.

## 6. Deliberate deviations and risks

| # | Legacy behaviour | Redux behaviour | Risk |
|---|---|---|---|
| 1 | none | none | none |

## 7. Per-game scope and acceptance

### 7.1 Per-game scope

| Game | In scope |
|---|---|
| FNV | yes |

### 7.2 Acceptance

A scenario passes when it passes.

## 8. Metrics

| Metric | Definition |
|---|---|
| Rate | a ÷ b |

## 9. Delivery sequence

**Phase 0 — Catalog (no engine code).** Do the survey. Exit: the catalog is checked in.

**Phase 1 — Build.** Build it. Exit: FNV scenario passes.

## 10. Decisions required

1. **Q1 — Default radius.** Recommended: 3. Alternative: 5.

## 11. Document map

| Need | Go to |
|---|---|
| Design | [design.md](../engine/design.md) |

## 12. Revision and review log

| Rev | Date | Change | Review |
|---|---|---|---|
| 1 | 2026-11-01 | First draft | — |
"""

GOOD_DESIGN = """# Design

## 1. Overview

- **DSG-001.** A design requirement.
"""

GOOD_INDEX = """# Index

| Doc | Crate(s) | What it covers |
|---|---|---|
| [Widget Streaming — PRD](../prd/widget-streaming.md) | core | PROPOSED |
"""


def _write_tree(root: Path, prd_text: str = GOOD_PRD, index_text: str = GOOD_INDEX, name: str = "widget-streaming.md") -> Path:
    (root / "docs" / "prd").mkdir(parents=True, exist_ok=True)
    (root / "docs" / "engine").mkdir(parents=True, exist_ok=True)
    (root / "docs" / "engine" / "design.md").write_text(GOOD_DESIGN, encoding="utf-8")
    (root / "docs" / "engine" / "index.md").write_text(index_text, encoding="utf-8")
    path = root / "docs" / "prd" / name
    path.write_text(prd_text, encoding="utf-8")
    return path


def self_test() -> int:
    failures: List[str] = []

    def expect(label: str, prd_text: str, codes: Iterable[str], index_text: str = GOOD_INDEX, name: str = "widget-streaming.md") -> None:
        _FAMILY_CACHE.clear()
        _SLUG_CACHE.clear()
        with tempfile.TemporaryDirectory(prefix="prd-selftest-") as tmp:
            root = Path(tmp)
            path = _write_tree(root, prd_text, index_text, name)
            prd = check_prd(path, root)
            got = {f.code for f in prd.findings}
            want = set(codes)
            errors_got = {f.code for f in prd.findings if f.is_error}
            if want - got:
                failures.append(f"{label}: expected {sorted(want - got)} to fire; got {sorted(got)}")
            if not want and errors_got:
                failures.append(f"{label}: expected a clean run; got errors {sorted(errors_got)}: "
                                + "; ".join(f.message for f in prd.findings if f.is_error))
            if label == "conformant" and got:
                failures.append(f"{label}: expected no findings at all; got {sorted(got)}: "
                                + "; ".join(f.message for f in prd.findings))

    expect("conformant", GOOD_PRD, [])
    expect("missing status", GOOD_PRD.replace("**Status**: PROPOSED (2026-11-01).", "Status: proposed."), ["E03"])
    expect("unknown state", GOOD_PRD.replace("PROPOSED (2026-11-01)", "DRAFT (2026-11-01)"), ["E03"])
    expect("bad title", GOOD_PRD.replace("— Product Requirements", "— PRD"), ["E02"])
    expect("no scope", GOOD_PRD.replace("**Scope**:", "Scope:"), ["E04"])
    expect("no related", GOOD_PRD.replace("**Related**:", "Related:"), ["E05"])
    expect("no prefix decl", GOOD_PRD.replace("Requirement IDs use the `WDG-` prefix.", ""), ["E06"])
    expect("prefix collision", GOOD_PRD.replace("`WDG-`", "`DSG-`").replace("WDG-", "DSG-"), ["E06"])
    expect("foreign id", GOOD_PRD.replace("**WDG-LD-002.**", "**ZZZ-LD-002.**"), ["E07"])
    expect("duplicate id", GOOD_PRD.replace("**WDG-LD-002.**", "**WDG-LD-001.**"), ["E07"])
    expect("two-part id", GOOD_PRD.replace("**WDG-LD-002.**", "**WDG-002.**"), ["E07"])
    expect("missing section", GOOD_PRD.replace("## 8. Metrics", "## 8. Numbers"), ["E08"])
    expect("phase without exit", GOOD_PRD.replace("Exit: FNV scenario passes.", "Done when it works."), ["E09"])
    expect("no phases", GOOD_PRD.replace("**Phase 0", "Phase 0").replace("**Phase 1", "Phase 1"), ["E09"])
    expect("empty decisions", GOOD_PRD.replace("1. **Q1 — Default radius.** Recommended: 3. Alternative: 5.", "Later."), ["E10"])
    expect("none-open decisions", GOOD_PRD.replace("1. **Q1 — Default radius.** Recommended: 3. Alternative: 5.", "No decisions required."), [])
    expect("broken file link", GOOD_PRD.replace("[Design](../engine/design.md)", "[Design](../engine/nope.md)"), ["E11"])
    expect("broken anchor", GOOD_PRD.replace("design.md#1-overview", "design.md#overview"), ["E11"])
    expect("broken own anchor", GOOD_PRD.replace("Why this exists.", "Why this exists ([metrics](#metrics))."), ["E11"])
    expect("good own anchor", GOOD_PRD.replace("Why this exists.", "Why this exists ([metrics](#8-metrics))."), [])
    expect("today column undated", GOOD_PRD.replace("dated at HEAD on 2026-11-01", "undated"), ["E12"])
    expect("not indexed", GOOD_PRD, ["E13"], index_text="# Index\n")
    expect("log header wrong", GOOD_PRD.replace("| Rev | Date | Change | Review |", "| Version | Date | Change | Review |"), ["E14"])
    expect("log date mismatch", GOOD_PRD.replace("| 1 | 2026-11-01 | First draft | — |", "| 1 | 2026-10-31 | First draft | — |"), ["E14"])
    expect("log bad verdict", GOOD_PRD.replace("| 1 | 2026-11-01 | First draft | — |", "| 1 | 2026-11-01 | First draft | owner: fine |"), ["E14"])
    expect("approved without verdict", GOOD_PRD.replace("PROPOSED (2026-11-01)", "APPROVED (2026-11-01)"), ["E15"])
    approved = (GOOD_PRD.replace("PROPOSED (2026-11-01)", "APPROVED (2026-11-02)")
                .replace("| 1 | 2026-11-01 | First draft | — |",
                         "| 1 | 2026-11-01 | First draft | — |\n| 2 | 2026-11-02 | Q1 decided | owner: approved |")
                .replace("Recommended: 3. Alternative: 5.", "Recommended: 3. Alternative: 5. **Decided** (2026-11-02): 3."))
    expect("approved clean", approved, [])
    expect("approved with open decision", approved.replace(" **Decided** (2026-11-02): 3.", ""), ["E15"])
    expect("active without tracking", approved.replace("APPROVED (2026-11-02)", "ACTIVE (2026-11-02)"), ["E15"])
    expect("active with tracking", approved.replace("APPROVED (2026-11-02)", "ACTIVE (2026-11-02)")
           .replace("**Scope**:", "**Tracking**: #1234 (parent).\n\n**Scope**:"), [])
    expect("withdrawn without reason", GOOD_PRD.replace("PROPOSED (2026-11-01)", "WITHDRAWN (2026-11-01)"), ["E15"])
    expect("template token", GOOD_PRD.replace("Why this exists.", "Why <Feature name> exists."), ["E16"])
    expect("misnamed file", GOOD_PRD, ["E01", "E13"], name="widget-streaming-prd.md")
    expect("wording warning", GOOD_PRD.replace("A widget SHOULD unload when far.", "A widget unloads when far."), ["W01"])
    expect("withdrawn id no warning", GOOD_PRD.replace("A widget SHOULD unload when far.", "(withdrawn, rev 2) Folded into WDG-LD-001."), [])
    expect("tbd warning", GOOD_PRD.replace("Why this exists.", "Why this exists. TBD."), ["W02"])
    expect("no recommendation warning", GOOD_PRD.replace("Recommended: 3. Alternative: 5.", "3 or 5."), ["W04"])

    # Fenced code and HTML comments must be invisible to every check.
    fenced = GOOD_PRD.replace("Why this exists.", "Why this exists.\n\n```\n## 99. Fake heading\n[broken](nope.md)\n- **ZZZ-X-001.** fake\n```")
    expect("fenced content ignored", fenced, [])
    commented = GOOD_PRD.replace("Why this exists.", "Why this exists.\n\n<!--\n**Tracking**: #NNNN\n## 99. Fake heading\n- **ZZZ-X-001.** fake TBD\n-->\n<!-- [broken](nope.md) --> still here")
    expect("html comments ignored", commented, [])

    # The slug rule against the shapes real headings take.
    slug_cases = {
        "5.1 Identity, profile and order (`LMC-ID`)": "51-identity-profile-and-order-lmc-id",
        "2.1 Engine behaviour: absent by construction is a claim, not an assumption": "21-engine-behaviour-absent-by-construction-is-a-claim-not-an-assumption",
        "DependencyResolver & Conflict Resolution": "dependencyresolver--conflict-resolution",
        "16. C and C++ SDK requirements": "16-c-and-c-sdk-requirements",
        "Per-game support detail": "per-game-support-detail",
    }
    for heading, want in slug_cases.items():
        got = github_slug(heading)
        if got != want:
            failures.append(f"slug({heading!r}) = {got!r}, want {want!r}")

    # The real template, with its placeholders filled the way `/prd new`
    # fills them, must satisfy every structural rule. Only link targets
    # (E11), the index row (E13) and nothing else may be missing.
    template = Path(__file__).resolve().parent.parent / "docs" / "prd" / "TEMPLATE.md"
    if template.exists():
        filled = (template.read_text(encoding="utf-8")
                  .replace("<Feature name>", "Widget Streaming")
                  .replace("YYYY-MM-DD", "2026-11-01")
                  .replace("XXX-", "WDG-")
                  .replace("<slug>", "widget-streaming"))
        _FAMILY_CACHE.clear()
        _SLUG_CACHE.clear()
        with tempfile.TemporaryDirectory(prefix="prd-selftest-") as tmp:
            root = Path(tmp)
            path = _write_tree(root, filled, GOOD_INDEX)
            prd = check_prd(path, root)
            structural = sorted({f.code for f in prd.findings if f.is_error and f.code not in ("E11", "E13")})
            if structural:
                failures.append("docs/prd/TEMPLATE.md drifted from the checker: " + ", ".join(structural) + " — "
                                + "; ".join(f.message for f in prd.findings if f.is_error and f.code not in ("E11", "E13")))

    if failures:
        print("check-prd-conformance: self-test FAILED", file=sys.stderr)
        for f in failures:
            print(f"  {f}", file=sys.stderr)
        return 1
    print("check-prd-conformance: self-test passed")
    return 0


# ── Entry point ───────────────────────────────────────────────────────────


def main(argv: List[str]) -> int:
    ap = argparse.ArgumentParser(description=(__doc__ or "").split("\n\n")[0])
    ap.add_argument("paths", nargs="*", type=Path, help="PRD files to check (default: every docs/prd/*.md)")
    ap.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent, help="repository root")
    ap.add_argument("--strict", action="store_true", help="treat warnings as errors (manual-review mode)")
    ap.add_argument("--list", action="store_true", help="print the PRD registry from the headers and exit")
    ap.add_argument("--self-test", action="store_true", help="run the built-in fixture suite and exit")
    args = ap.parse_args(argv)
    if args.self_test:
        return self_test()
    root = args.root.resolve()
    if args.list:
        return list_registry(root)
    paths = [p if p.is_absolute() else (Path.cwd() / p) for p in args.paths]
    return run(root, paths, args.strict)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
