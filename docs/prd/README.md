# Product Requirements Documents — Standard

**Status**: ACTIVE (2026-11-01). Applies to every file under `docs/prd/`
except this one and [TEMPLATE.md](TEMPLATE.md). The first PRD written to it,
and the structure it was derived from, is
[legacy-mod-compatibility.md](legacy-mod-compatibility.md).

**Why**: the project is approaching a million lines. Code quality is gated
by tests, clippy, the lock-order proof and the audit suite; *intent* had no
gate at all. A feature could reach its first implementation issue with its
scope, its per-game promise and its acceptance criteria living in a chat
transcript. A PRD puts that intent in one reviewed file, before code, and
the same file becomes the acceptance criteria the issues and commits cite.

**What a PRD is not**: a design. A PRD states the *promise* (what the
engine will do, for whom, measured how); the designs under
[docs/engine/](../engine/index.md) own the *how* and keep their own
requirement IDs. One PRD is usually the umbrella over several designs. A
PRD is also not a plan: its phases name exit criteria, the tracking issues
carry the work.

---

## 1. When a PRD is required

A PRD is written, reviewed and **APPROVED before the first implementation
issue is filed** when the work meets any one of these triggers:

| # | Trigger | Example |
|---|---|---|
| T1 | A new ROADMAP milestone or tier entry, or a change to what an existing milestone promises | a new M-number; "M46 now also loads loose roots" |
| T2 | A new abstraction layer (`*AL`), a new crate, or a new or changed host-facing contract: CLI / boot-request surface, debug protocol, SDK hostcalls, save format (`FORMAT_MAJOR`), launcher profiles | a `MODAL` layer; a new `--profile` schema |
| T3 | A change to what the engine promises per game: a compatibility-matrix row, a support tier, a mod mechanism's disposition | "Oblivion exteriors supported"; "DAR conditions served natively" |
| T4 | Work expected to span three or more crates, or to need five or more issues | the exterior readiness epic |
| T5 | The owner asks for one | — |

Not required: bug fixes, audit findings, refactors and file splits,
single-issue features, tuning passes, and design notes that detail how to
build a promise already approved. Design documents under `docs/engine/`
that carry `Status: PROPOSED` today stay designs; they get a PRD only when
their scope grows into a trigger.

**Waiver**: the owner can waive the PRD for a triggered change by writing
`no PRD: <one line of why>` on the tracking issue. The automatic gate cannot
see waivers; the manual review of the issue does.

---

## 2. Where PRDs live and how they are named

- Path: `docs/prd/<slug>.md`. The slug is the feature name in kebab-case
  (`legacy-mod-compatibility`), without a `-prd` suffix — the directory is
  the type. The gate fails any `*-prd.md` elsewhere under `docs/`.
- Registered in [docs/engine/index.md](../engine/index.md) with a row that
  links `../prd/<slug>.md` and repeats the status word and date.
- Requirement IDs are `PREFIX-AREA-NNN` (`LMC-ID-005`). The prefix is two to
  six capitals, declared once in the header sentence `Requirement IDs use
  the \`PREFIX-\` prefix`, and unique across every PRD and every design
  document's ID family (`FIX-`, `SEC-`, `LINK-`, …). The gate checks the
  collision.
- Links from a PRD to a design are relative (`../engine/<design>.md#anchor`);
  the gate resolves every file and anchor.

---

## 3. Lifecycle

| State | Meaning | Entry condition | Who moves it |
|---|---|---|---|
| **PROPOSED** | Drafted from the template; under review | Passes the automatic gate with zero errors | Author |
| **APPROVED** | The promise is agreed; implementation issues may be filed | A review-log row with verdict `approved`; every §10 decision carries **Decided**; gate passes | Owner |
| **ACTIVE** | Work is under way | `**Tracking**:` header line naming the parent issue (and phase issues); Phase 0's **Phase 0: verify** items are closed or filed as issues | Author, after approval |
| **SHIPPED** | Every phase exit met; metrics published in ROADMAP | Last phase's tracking issue closed; the PRD is retained as history | Author |
| **WITHDRAWN** | Will not be built | `**Reason**:` header line | Owner |
| **SUPERSEDED** | Replaced by another PRD | `**Superseded by**:` header line linking it | Author |

Every transition and every content revision adds one row to §12 of the PRD
and updates the status line in the same commit; the two dates are the same
date. Commit messages follow `docs(prd): <slug> — <what changed>` for
revisions and `docs(prd): <slug> → <STATE>` for transitions.

Nothing in this standard blocks a spike: an experiment in a worktree needs
no PRD. Filing the issues that turn a spike into a feature does.

---

## 4. Required structure

The template is [TEMPLATE.md](TEMPLATE.md); its section numbers are the
convention, not a rule. The gate matches headings by keyword (third column)
at H2 or H3, in this order (order is a warning, presence is an error).

| § | Section | Heading contains | Must hold |
|---|---|---|---|
| — | Header block | — | H1 `<Feature> — Product Requirements`; `**Status**: STATE (YYYY-MM-DD)`; `**Scope**:`; `**Related**:` with the owning-design table; the normative-terms sentence with the ID prefix |
| 1 | Product thesis | `thesis` | Why, for whom, the owner's framing where it exists, and the one corollary that shapes the requirements |
| 2 | Terminology | `terminolog` | Every term the requirements lean on, in a table, defined once |
| 3 | Core model | (free) | The closed vocabulary the requirements are phrased in (classes, tiers, dispositions); any inventory table with a `Today` column states `at HEAD on YYYY-MM-DD` |
| 4 | Goals and non-goals | `goal`, `non-goal` | `PREFIX-G-NNN` goals that §8 can count; `PREFIX-NG-NNN` exclusions with the reason or the design that decided them |
| 5 | Requirements by area | `requirements` | One H3 per area with its own ID segment; one observable per ID; MUST / SHOULD / MAY; state at HEAD in the sentence; **Phase 0: verify** on unverified claims |
| 6 | Deliberate deviations and risks | (free) | Each departure from original behaviour, what it makes redundant, the risk it creates, and the requirement that handles it |
| 7 | Per-game scope and acceptance | `per-game`, `acceptance` | All seven titles in / out / n/a with a reason; runnable acceptance scenarios that name their data |
| 8 | Metrics | `metric` | Definitions and granularity only; the numbers live in ROADMAP |
| 9 | Delivery sequence | `delivery` | `**Phase N — …**` paragraphs, Phase 0 without engine code, each ending in `Exit:` plus a measurement |
| 10 | Decisions required | `decision` | `N. **Qn — …**` items, recommendation first, alternatives with consequences; `**Decided** (date): …` once taken; or "No decisions required" |
| 11 | Document map | `document map` | Where each need is answered; every design cited more than once |
| 12 | Revision and review log | `revision` | `\| Rev \| Date \| Change \| Review \|` table; one row per revision or transition; review cell `—` or `<who>: revise / approved / withdrawn` |

---

## 5. Writing rules

- **R1 — One observable per ID.** A requirement names one thing a test, a
  command or a reader can check: a value, a report line, a file, a number.
  "Works correctly" and "is handled" are not requirements. Never reuse a
  number after review; retire an ID in place with "(withdrawn, rev N)".
- **R2 — RFC 2119 words.** MUST, SHOULD and MAY carry their RFC 2119
  meanings and are written in capitals. A requirement without one is
  flagged by the gate as a warning (W01); the manual review decides whether
  it is a requirement or a note that belongs in the model section.
- **R3 — Nothing is guessed.** A statement about how an original engine, a
  file format, a tool or a mod ecosystem behaves cites an external source
  (UESP and the CK / GECK wikis, including the local dumps under
  `/mnt/data/src/reference/`, xEdit / LOOT / Wrye Bash documentation and
  source, extender sources, format specifications). This repository and its
  issues are not a source for original-engine behaviour. What cannot be
  verified is written as **Phase 0: verify** with what would settle it, not
  as fact and not as `TBD`.
- **R4 — State at HEAD is dated and true.** Anything that describes current
  engine behaviour names the file, test or issue that proves it and the date
  it was checked (`at HEAD on YYYY-MM-DD`). A hand-written "Today" column
  states what replaces it (a generated catalog, a test) and when.
- **R5 — Per-game scope is explicit.** Every one of the seven titles is in,
  out or not applicable, with a reason. No requirement implies a per-game
  branch past the NIFAL / EXAL translation boundary (CLAUDE.md, Architecture
  Invariants); a per-game difference belongs in the translate step and is
  stated as such.
- **R6 — Link, never copy.** Numbers that have a home elsewhere (parse rates,
  test counts, the compatibility matrix, the fix catalog) are linked, not
  restated. A PRD restates a design's requirement only to cite its ID.
- **R7 — Phases end in measurements.** An exit criterion is a pass rate, a
  checked-in artefact, a published number, a passing scenario. Phase 0 has
  no engine code: it is the catalog, the corpus, the verify list and the
  decisions.
- **R8 — Decisions are the owner's.** Each decision has a recommendation
  first, at least one alternative, and the consequence of each. A decision
  another document already made is cited, not reopened.
- **R9 — Tables for enumerations.** Inventories, scopes, metrics, phases
  with many items, corpora and logs are tables; numbers do not appear in
  prose.
- **R10 — Names stay inside the repository's vocabulary.** Terms come from
  the related designs or §2; a PRD does not coin a second name for a thing
  the code already names.

---

## 6. The review process

A PRD passes two reviews. The automatic one (§7) checks shape and runs in
CI; the manual one (§8) checks judgment and is recorded in the PRD. Both
must pass before the status becomes APPROVED.

1. **Draft.** `/prd new <slug> "<Feature name>" <PREFIX>` copies the
   template, fills the header and adds the index row. Write it. Run
   `python3 scripts/check-prd-conformance.py docs/prd/<slug>.md --strict`
   until the errors are gone and every remaining warning is one you can
   defend.
2. **Review round.** `/prd review <slug>` runs the gate and walks the §8
   rubric, re-checking the PRD's claims about HEAD against the code and its
   claims about original engines against sources. Findings come back as a
   list keyed by rubric item and requirement ID. Any reviewer (a person or
   that pass) may file a round; each round is one log row with verdict
   `revise`.
3. **Fold in.** The author revises, bumps the revision, adds the row.
   Rounds repeat until the owner is satisfied.
4. **Approve.** The owner records `owner: approved` in the log, every §10
   item gets its **Decided** line, the status line flips. The gate enforces
   both. Implementation issues may now be filed (§9).

---

## 7. The automatic gate

```bash
python3 scripts/check-prd-conformance.py                # every PRD under docs/prd/ (what CI runs)
python3 scripts/check-prd-conformance.py docs/prd/<slug>.md --strict   # one PRD, warnings listed and fatal
python3 scripts/check-prd-conformance.py --list         # the registry: state, date, prefix, ids, decisions, tracking
python3 scripts/check-prd-conformance.py --self-test    # the fixture suite CI runs first
```

CI runs the self-test and then the check in the `source-integrity` job
(no toolchain needed). Errors fail the job; warnings are printed aggregated
by code, and `--strict` lists every one and makes them fatal, which is the
mode a review round uses.

| Code | Rule |
|---|---|
| E01 | File is `docs/prd/<kebab-slug>.md`; no `-prd` suffix; no `*-prd.md` anywhere else under `docs/` |
| E02 | First line is an H1 ending in `— Product Requirements` |
| E03 | `**Status**: STATE (YYYY-MM-DD)` opens the header; STATE is one of PROPOSED, APPROVED, ACTIVE, SHIPPED, WITHDRAWN, SUPERSEDED |
| E04 | `**Scope**:` line present |
| E05 | `**Related**:` line present |
| E06 | The prefix is declared (`Requirement IDs use the \`PREFIX-\` prefix`), two to six capitals, and no other PRD or design document defines IDs in that family |
| E07 | Every defined ID (a bold ID opening a list item or table cell) is `PREFIX-AREA-NNN`, uses the declared prefix, and is defined once; at least one exists |
| E08 | Every required section of §4 is present (by heading keyword, H2 or H3) |
| E09 | The delivery sequence has at least one `**Phase N …**` paragraph and each one contains `Exit:` |
| E10 | The decisions section has at least one `N. **Qn — …**` item or says there are none |
| E11 | Every relative link resolves to an existing file; every `#anchor` into a Markdown file matches one of its headings (GitHub slug rule); same-document anchors likewise |
| E12 | A table with a `Today` column is accompanied by `at HEAD on YYYY-MM-DD` |
| E13 | `docs/engine/index.md` links `../prd/<slug>.md` |
| E14 | The revision log is a `Rev / Date / Change / Review` table; revisions run 1, 2, 3 …; dates are ISO; review cells are `—` or end in a known verdict; the last row's date equals the status date |
| E15 | State rules: APPROVED / ACTIVE / SHIPPED need an `approved` verdict and every decision **Decided**; ACTIVE / SHIPPED need `**Tracking**:` with at least one `#NNNN`; WITHDRAWN / SUPERSEDED need `**Reason**:` or `**Superseded by**:` |
| E16 | No template placeholder survives (`XXX-`, `YYYY-MM-DD`, `<slug>`, `<Feature name>`) |
| W01 | A requirement in the requirements section states no MUST / SHOULD / MAY |
| W02 | `TBD` or `TODO` in the text (write **Phase 0: verify** with what settles it, or decide) |
| W03 | Required sections are not in template order |
| W04 | A decision names no recommendation |

Fenced code and HTML comments are invisible to every check, so the
template's guidance comments cost nothing. The self-test fills the real
template's placeholders and asserts it still satisfies every structural
rule, so the template and the checker cannot drift apart silently.

---

## 8. Manual review

The rubric below is what a reviewer answers, in order, for every round.
Each item says what passes and which automatic check already covers its
shape, so the reviewer spends the round on judgment. A finding cites the
rubric item and the requirement ID or section (`M6 / LMC-AST-002: …`).

| # | Item | Passes when | Gate covers |
|---|---|---|---|
| M1 | Thesis | Says who the feature is for and ends with the corollary that shapes the requirements; a reader can state what the engine will *not* do | — |
| M2 | Scope and owners | Scope names what is in and out; every promised mechanism has an owning design in Related, or Phase 0 writes the missing design | E04, E05 |
| M3 | Core model | The vocabulary is closed and defined once; every requirement is phrased in it; the user-facing reporting of each class is stated | — |
| M4 | Testable requirements | Each ID names one observable; none says "correctly", "properly", "handled" without saying how that is seen | E07, W01 |
| M5 | Nothing guessed | Every original-engine, format, tool or ecosystem claim cites an external source; the rest is **Phase 0: verify** with what settles it (R3) | W02 |
| M6 | HEAD is true | Spot-check at least three "Today" or "shipped" claims against the code at HEAD; one wrong claim fails the round (premise rule from the audit-hygiene practice) | E12 |
| M7 | Per-game scope | All seven titles classified with reasons; waves follow the engine tiers; no requirement implies a per-game branch past the translation boundary (R5) | E08 (`per-game`) |
| M8 | Deviations and risks | Every deliberate departure from original behaviour names what becomes redundant and the risk it creates, pointing at the requirement that handles it | — |
| M9 | Runnable acceptance | Scenarios can be driven headless (`cargo test`, `--bench-hold` + `byro-dbg`, a `docs/smoke-tests/` script, or a corpus with a pass rule) and name their data; nothing redistributable is implied | E08 (`acceptance`) |
| M10 | Metrics have one home | Each metric has a definition and a granularity; the PRD says which ROADMAP table will hold the number | E08 (`metric`) |
| M11 | Phases | Ordered by what they unblock; Phase 0 has no engine code; every exit is a measurement; milestone dependencies named | E09 |
| M12 | Decisions | Recommendation first, alternatives with consequences; nothing reopened that another document decided; the questions are the owner's to answer, not the author's | E10, W04 |
| M13 | Invariants | Nothing contradicts CLAUDE.md's architecture invariants, ROADMAP's keep list or anti-scope; where it must, the PRD says so in a decision | — |
| M14 | Links, not copies | No state owned elsewhere is restated (R6); the document map covers every design cited more than once; every link resolves | E11, E13 |
| M15 | Readability | Enumerations are tables; numbers are in tables; any requirement is findable from its ID in under a minute | W03 |

**Recording.** One log row per round (`<who>: revise`). The findings
themselves travel in the review message, not the PRD; the author folds
them into the next revision and the row's Change cell names the topics.
Only the owner's `approved` moves the status. A round that finds nothing
still adds its row.

---

## 9. From approval to issues

1. **Parent issue.** On APPROVED → ACTIVE, file one parent issue titled
   after the PRD and one issue per phase; the parent lists the phases and
   links the PRD; the PRD's `**Tracking**:` line lists the parent and the
   phase issues.
2. **Acceptance criteria by ID.** Each issue carries `## Acceptance
   Criteria`, one bullet per criterion, each quoting the requirement IDs it
   satisfies. That is the shape `scripts/check-acceptance-coverage.py`
   scores and `/fix-issue` reads.
3. **Commits cite IDs.** A commit that satisfies a requirement names it in
   the body (`Satisfies LMC-ID-001`), beside the `Fix #N` line, so `git log
   --grep` answers "where was this requirement met".
4. **Phase exits move numbers, not prose.** Meeting an exit publishes its
   metric in ROADMAP (one home) and closes the phase issue; the PRD gains a
   log row only on content revisions and state changes.
5. **Scope changes go through the log.** A requirement added, withdrawn or
   reworded after approval is a revision row; a change to the promise
   itself (a goal, a non-goal, a disposition) reopens review for that
   change and is marked `revise` until the owner approves it again.

---

## 10. Document map

| Need | Go to |
|---|---|
| The template to copy | [TEMPLATE.md](TEMPLATE.md) |
| The worked example | [legacy-mod-compatibility.md](legacy-mod-compatibility.md) |
| The gate's source and fixture suite | `scripts/check-prd-conformance.py` |
| Scaffold or review from the CLI | `/prd new …`, `/prd review <slug>`, `/prd list` (`.claude/commands/prd/SKILL.md`) |
| Where the gate runs | `source-integrity` job in `.github/workflows/ci.yml`; [contributing.md § CI](../contributing.md#ci) |
| Designs the PRDs point at | [docs/engine/index.md](../engine/index.md) |
| Architecture invariants a PRD must respect | [CLAUDE.md](../../CLAUDE.md), [ROADMAP.md § Architecture Decisions](../../ROADMAP.md#architecture-decisions) |
