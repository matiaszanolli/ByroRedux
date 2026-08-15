# CHAR-D6-04: docs/feature-matrix.md has no character/progression rows and ROADMAP.md never mentions CHARAL

- **Issue**: [#2961](https://github.com/matiaszanolli/ByroRedux/issues/2961)
- **Finding ID**: `CHAR-D6-04`
- **Labels**: `medium,legacy-compat,documentation`
- **Source report**: [`docs/audits/AUDIT_CHARACTER_2026-08-15.md`](../../../docs/audits/AUDIT_CHARACTER_2026-08-15.md)
- **Run**: `/audit-character` (first audit of this subsystem), 2026-08-15, HEAD `c25f61e6`

> Immutable snapshot of the issue *as filed* (TD10-001 / #1156). GitHub is
> authoritative for current state — query `gh issue view 2961 --json state`.

---

- **Severity**: MEDIUM
- **Dimension**: Coverage & Doctrine
- **Game**: all
- **Location**: `docs/feature-matrix.md` (no such section; the gap table at
  "## What Doesn't Work Yet (live gaps as of 2026-08-12)")
- **Status**: NEW
- **Source**: `docs/engine/charal.md` §8 (the rollout order the matrix would mirror);
  `docs/engine/charal-fnv-fo3-ruleset.md` / `charal-fo4-ruleset.md` (the two games
  whose rulesets are wired and therefore matrix-reportable today)
- **Description**: `docs/feature-matrix.md` is documented as the living per-game
  runtime-status document and as lagging the code — so a lag is reportable doc rot,
  which is exactly what this is. It carries sections for Cell Loading, Rendering, NPC
  Spawning, Animation, Audio, Physics, Scripting, Quests, UI and Starfield-specifics.
  It has **zero** rows for character stats, actor values, derived stats, leveling, or
  progression: `grep -ci charal docs/feature-matrix.md` → 0, and no row mentions
  `ActorValues`. Its NPC Spawning table (the closest home) covers spawn, skeleton,
  FaceGen, equipment, inventory, skinning and AI — but not whether the spawned actor
  has stats. Nor does the gap table carry a CHARAL row, so the two most consequential
  live gaps — Oblivion/Skyrim rulesets built but unwired, and both CHARAL tick systems
  inert — appear in no planning document at all. This is the same shape as the CLOSED
  precedents `#2417` (no Quests/M43 section despite two sessions of work) and `#2047`
  (NPC AI listed as unstarted despite seven shipped runtimes).
- **Evidence**: `grep -c -i "charal" ROADMAP.md HISTORY.md docs/feature-matrix.md` →
  `ROADMAP.md:0`, `HISTORY.md:9`, `docs/feature-matrix.md:0`. Section headers in
  `docs/feature-matrix.md` are Cell Loading / Rendering / NPC Spawning / Animation /
  Audio / Physics / Scripting / Quests / UI / Starfield-Specific / What Doesn't Work
  Yet — no character or progression heading. `README.md` mentions CHARAL twice; the
  matrix and `ROADMAP.md` never do.
- **Impact**: The document a reader consults to answer "does FNV have working
  character stats?" cannot answer it, in either direction. Worse for planning: the
  wiring gap the matrix above identifies as CHARAL's dominant cost is recorded nowhere
  a milestone planner looks. `ROADMAP.md` has no CHARAL row either, so `HISTORY.md`
  and the capture documents are the only trace.
- **Related**: `CHAR-D6-03` (the §8 rollout omission this would mirror); `#2417`,
  `#2047` (CLOSED precedents, same shape, different subsystem).
- **Suggested Fix**: Add a "Character / Progression (CHARAL)" section with one row per
  matrix column above (ruleset wired, derived stats, leveling, regen, affliction)
  across the seven game columns, and one gap-table row for "Oblivion/Skyrim rulesets
  built but unwired; regen + affliction ticks inert".

---

## Completeness Checks
- [ ] **SIBLING**: The same drift class is swept across the other capture documents / docstrings, not just the row cited
- [ ] **TESTS**: A regression test pins this specific fix (`cargo test -p byroredux-core character`)

---

*Filed by `/audit-publish` from [`docs/audits/AUDIT_CHARACTER_2026-08-15.md`](docs/audits/AUDIT_CHARACTER_2026-08-15.md) — `/audit-character`, 2026-08-15, HEAD `c25f61e6`. First audit of this subsystem. Verified CONFIRMED against current code at publish time.*
