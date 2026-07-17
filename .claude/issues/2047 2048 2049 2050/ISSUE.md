# Issues 2047, 2048, 2049, 2050

All four are documentation-only tech-debt fixes (stale docs / audit-finding rot).

## #2047 — TD3-101 (MEDIUM): feature-matrix.md still lists NPC AI/behavior as entirely unstarted
**Location**: `docs/feature-matrix.md:73,172`

Seven M42 AI-package procedure runtimes shipped (Sandbox/Wander/Travel/Follow/Escort/Guard/Patrol,
opt-in via `BYRO_*` env flags) since the doc was last touched. The "AI / behavior" row still reads
`✗ | ✗ | ✗ | ✗`; "What Doesn't Work Yet" still lists the whole category as a live gap.

**Fix**: Update both rows to `~` (partial) with a footnote naming the opt-in-gated procedures and
v0 scope limits (spawn-time-only selection, no per-frame re-evaluation); name the still-genuinely-
missing ~10 procedures in the gaps table instead of the whole category.

## #2048 — TD4-001 (MEDIUM): audit-audio SKILL.md's latest-report pointer is 4 reports stale
**Location**: `.claude/commands/audit-audio/SKILL.md:78-80`

Hardcodes "the latest is `_2026-07-02.md`" — four newer reports now exist (`_07-03.md`, `_07-14.md`,
`_07-16.md`). Contradicts the very next sentence's "sort by date, do not hardcode" instruction.

**Fix**: Delete the hardcoded filename/supersession list; keep only the "sort by date" instruction.

## #2049 — TD4-002 (MEDIUM): audit-audio SKILL.md cites #1859 as still open — closed 2026-07-15
**Location**: `.claude/commands/audit-audio/SKILL.md:85-93`

Tells the next audit agent a `SoundCache` docstring path issue is "still open" as AUD-2026-07-02-01
/ #1859. Fixed in `37394005` (2026-07-14), closed on GitHub 2026-07-15, confirmed FIXED in
`AUDIT_AUDIO_2026-07-16.md`.

**Fix**: Replace with a closed-regression-guard note, only re-flag if the path drifts again.

## #2050 — TD4-004 (MEDIUM): audit-save SKILL.md claims no prior save audit exists
**Location**: `.claude/commands/audit-save/SKILL.md:107-109`

States this would be the first save audit; `AUDIT_SAVE_2026-06-23.md`, `_07-02.md`, `_07-03.md`,
`_07-16.md` all exist.

**Fix**: Replace with the standard "read most recent, diff direction" instruction.

## Domain classification
All four: documentation-only, no crate, no test surface.
