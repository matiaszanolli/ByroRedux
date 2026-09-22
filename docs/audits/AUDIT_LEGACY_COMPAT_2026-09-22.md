**HEAD**: `ee6d3fb39` · **Baseline**: `docs/audits/AUDIT_LEGACY_COMPAT_2026-09-11.md` (HEAD `b3db49fa`) · **Audited**: Dims 1, 2, 3 (all three had commits since the baseline touching their listed paths) · **Unchanged since baseline (skimmed)**: none — see per-dimension notes for which sub-areas within each dimension had zero commits and were guard-spot-checked rather than re-derived

# Legacy Compatibility Audit — 2026-09-22

## Scope note — skill restructuring since the baseline

The baseline report (2026-09-11) audited **7 dimensions** via 6 parallel
sub-agents. `.claude/commands/audit-legacy-compat/SKILL.md` was rewritten in
`a43b19603` (2026-09-19), **after** that baseline, and now keeps only
**3 dimensions** — coordinate/placement fidelity, legacy-subsystem coverage,
and the cross-game translation-pattern spot-check. The former NIFAL,
material-boundary, PHYSAL and EXAL dimensions were explicitly re-homed to
their layer owners (`/audit-nifal`, `/audit-physics`, `/audit-exterior`).
This run follows the current skill: 3 dimensions, no sub-agents (per the
orchestrator's explicit instruction), each analysed serially with scratch
notes at `/tmp/audit/legacy-compat/dim_{1,2,3}.md`.

**Method.** For each dimension: re-ran the skill's `First step:` grep/log
commands fresh against HEAD, re-traced every regression guard named in the
skill by reading the current code (not carried forward from the baseline
report's citations), cross-referenced the same-week sibling reports
(`AUDIT_NIFAL_2026-09-21.md`, `AUDIT_NIFAL_2026-09-21b.md`, `AUDIT_NIF_2026-09-21.md`,
`AUDIT_ESM_2026-09-21.md`, `AUDIT_GAMEPLAY_2026-09-21.md`) so findings already
owned by another audit's charter are cited, not re-derived, and deduped
every candidate finding against a freshly refreshed
`gh issue list --state all` (4,519 issues) plus `docs/audits/`. No source
file, test, doc or GitHub issue was modified; no engine binary was launched.

## Executive Summary

| Severity | Count (NEW + regression) |
|---|---:|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 0 |
| LOW | 1 |
| **Total** | **1** |

Matched to existing open issues (re-verified, not re-filed): **2** direct
(`#4126` COORD-01, `#4128` SUBSYS-01) plus **5** cited from same-week sibling
reports' overlapping charters (2 from `AUDIT_NIFAL_2026-09-21{,b}.md`, 3 from
`AUDIT_ESM_2026-09-21.md`'s Pattern-C-shaped findings) — see per-dimension
detail below.

**Headline: no new mapping-fidelity regression.** All three dimensions'
named regression guards (single `(x, z, -y)` producer, `EXTERIOR_CELL_UNITS`
single source, strip de-stitch, REFR-placement dispatcher routing, property→
pipeline dispatch, `AnimationController`/KFM non-wiring, string-interning
case-fold, zero raw `bsver()` literals, zero `GameVariant` trait, zero
scattered new `if game ==`) were re-verified against current code and hold.
The one substantive code change in scope this window, `#4549`
(`crates/nif/src/rotation.rs`, 2026-09-21) plus its animation-side
counterpart `#4396`/`#4397` (`crates/core/src/math/coord.rs`, 2026-09-21), is
a real fix — NaN/±inf now collapse to identity at the static-`NiTransform`
rotation, translation and scale gates before reaching `GlobalTransform` — not
a regression.

**The one new finding is a documentation-only doc-rot escalation of an
already-open issue**: `docs/engine/coordinate-system.md` was edited on
2026-09-13 (`d0dac91b1`) to claim the `--rotation-mode` CLI's out-of-range
safety fallback already works, citing `#4126` as if resolved — but `#4126`
is still open and the code (`byroredux/src/boot/mod.rs:301`) still clamps
(`mode.min(3)`) before the library's own fallback arm can fire. The doc is
now actively wrong in the direction of hiding the open bug, which is worse
than the baseline's structurally similar COORD-02 (stale-but-harmless doc).

---

## Dimension 1 — Coordinate + placement fidelity (Z-up → Y-up)

Commits since baseline: `d53be91be` (2026-09-21, `coord.rs` doc-only
leniency comment, part of the #4396/#4397 rotation-sanitizer unification),
`3e798cacd` (2026-09-21, `crates/nif/src/rotation.rs` — #4549 NaN/inf gates
on the static-transform boundary), and `d0dac91b1` (2026-09-13, doc-only
edit to `docs/engine/coordinate-system.md`).

**Re-verified clean:**
- Single position-swap producer `zup_to_yup_pos` — zero inline `(x, z, -y)`
  re-implementations anywhere in `byroredux/src`/`crates` (all hits are
  comments/docstrings).
- `EXTERIOR_CELL_UNITS = 4096.0` remains the sole cell-grid-math source;
  every other `4096.0` literal is an unrelated distance/test-fixture value.
- Strip de-stitch and the REFR-placement dispatcher: zero commits in range,
  unchanged.
- `#4549` (`3e798cacd`) and its animation-side sibling `#4396`/`#4397`
  (`d53be91be`): verified as a clean, well-tested fix, not a regression.
  `sanitize_rotation` now head-gates non-finite cells to identity;
  `repair_rotation_svd_or_identity` gates both its `max_sv` check and output
  cells for finiteness; the new `sanitize_transform_translation_and_scale`
  is confirmed wired at **both** `stream.rs::read_ni_transform` and
  `read_ni_transform_struct` call sites (`crates/nif/src/stream.rs:753,780`).

### COORD-03: `coordinate-system.md`'s `--rotation-mode` fallback claim is false — the code still clamps, contradicting the doc's own `#4126` citation
- **Severity**: LOW
- **Dimension**: 1 — Coordinate-system correctness (documentation)
- **Location**: `docs/engine/coordinate-system.md:182-186` vs. `byroredux/src/boot/mod.rs:294-303`
- **Status**: NEW
- **Description**: Commit `d0dac91b1` (2026-09-13, "Improved documentation
  for coordinate system handling and rotation modes") rewrote this doc
  paragraph from the previously-accurate "clamped to `0..=3`" to: "any value
  outside `0..=3` is passed through unclamped to
  `euler_zup_to_quat_yup_mode`'s own `_ =>` arm, which falls back to the
  safe shipping formula (mode 1) rather than silently producing garbage
  placement (`#4126`)." That is not what the code does, and hasn't changed
  since the 2026-09-11 baseline filed `#4126` (still OPEN, unfixed as of
  this run) for exactly this gap: `boot/mod.rs:301` still calls
  `crate::cell_loader::set_refr_rotation_mode_diag(mode.min(3))`, pre-clamping
  any out-of-range value to mode **3** (one of the two explicitly-wrong
  diagnostic conventions — CCW + XYZ-product) before
  `euler_zup_to_quat_yup_mode`'s own `_ =>` fallback arm ever sees it.
- **Evidence**:
  ```rust
  // byroredux/src/boot/mod.rs:299-302
  if let Some(idx) = args.iter().position(|a| a == "--rotation-mode") {
      if let Some(mode) = args.get(idx + 1).and_then(|v| v.parse::<u8>().ok()) {
          crate::cell_loader::set_refr_rotation_mode_diag(mode.min(3));
          log::info!("--rotation-mode {} active", mode.min(3));
  ```
  vs. the doc's claim that out-of-range values are "passed through
  unclamped." The doc cites `#4126` as if the described (safe) behavior is
  what's shipped; `#4126` is the open issue tracking the fact that it
  isn't.
- **Impact**: Low direct impact — same narrow blast radius as `#4126` itself
  (opt-in diagnostic flag only, never the default game path). But it
  compounds `#4126`: a contributor reading the authoritative reference doc
  now sees the bug described as already fixed, which risks `#4126` being
  dismissed as stale/already-resolved without checking the code, or a "fix"
  that only touches the doc.
- **Related**: `#4126` (COORD-01, the underlying still-open code bug). Note
  (incidental, not part of this finding): the same commit `d0dac91b1`
  *correctly* rewrote the adjacent XCLL paragraph to describe
  `xcll_direction_yup`, matching what `#4127`/COORD-02 asked for — worth a
  close-check by whoever triages `#4127` next, since the doc-vs-code gap it
  named appears resolved.
- **Suggested Fix**: Either fix the code to match the doc (drop the
  `.min(3)` clamp per `#4126`'s own suggested fix, letting the library's
  `_ =>` arm provide the fallback), which then makes the doc accurate; or,
  if `#4126` stays open a while longer, revert this paragraph to the
  previously accurate "clamped to `0..=3`" wording until the code fix
  lands.

---

## Dimension 2 — Legacy subsystem coverage

Heavy commit volume since baseline on this dimension's listed paths, but
nearly all of it (texture-role precedence, BGSM/greyscale merge, particle
emitter birth-rate/budget/orientation, NIFAL boundary fields) is the
material-translation / NIFAL boundary's charter, explicitly re-homed away
from this skill in the 2026-09-19 rewrite. That churn is already covered in
depth by the concurrent `AUDIT_NIFAL_2026-09-21.md` / `AUDIT_NIFAL_2026-09-21b.md`
(9 dimensions each) and `AUDIT_NIF_2026-09-21.md` — cited, not re-derived.
Relevant already-open findings from those reports, touching the same
non-finite-boundary work Dimension 1 also verified: `NIFAL-D2-2026-09-21-01`
(HIGH, the static-transform boundary — independently fixed here by `#4549`
during this same window) and `NIFAL-D2-2026-09-21b-01` (MEDIUM, `#4549`'s
gate is `is_finite()`-only on *inputs*, downstream arithmetic can still
manufacture `±inf` before the TLAS instance build — open, not re-filed).

**Re-verified against this dimension's own charter** (zero regressions):

- **Property → pipeline**: all 12 `NiProperty` types still trace to a
  landing site; the only commits touching `legacy_properties.rs` since
  baseline are texture-set precedence fixes that don't touch the
  flag-property dispatch block. `NiFogProperty`'s deliberate skip unchanged.
- **Animation model / state machine (AR-09)**: `AnimationController`
  remains deleted; `parse_kfm` still has zero non-test consumers.
  **`SUBSYS-01` is already tracked as `#4128` (OPEN)** — re-verified
  against current code, matches the baseline finding verbatim, correctly
  not re-filed.
- **Scene graph** (`world_bound.rs`) and **string interning**
  (`crates/core/src/string/`, `name_lookup.rs`): zero commits since
  baseline on either — guards spot-checked, unchanged.

One non-NIFAL commit in range (`774560dce`, "derive NPC walk speed from the
authored stride and split walk clips by body class") was spot-checked and is
an AI/gameplay locomotion feature (M42.10), not a legacy-subsystem mapping
gap — out of this dimension's scope per the skill's own "confirm against
gameplay scope" instruction; `AUDIT_GAMEPLAY_2026-09-21.md` is the
appropriate owner if not already covered there.

**No new findings.**

---

## Dimension 3 — Cross-game translation-pattern spot-check

Both listed paths have heavy commit volume since baseline (~35 commits on
`crates/plugin/src/esm/records/` alone: new LSCR/leveled-loot/consumable-
effect/river-water-current records and profiles).

**Re-run first-step checks, fresh against HEAD:**
- Zero production `bsver()` raw-literal threshold hits (Pattern A) — matches
  the skill's pinned "0 hits at 2026-09-19" baseline exactly.
- All `game ==`/`GameKind::` hits in `cell_loader`/`scene` are table-shaped
  per-game providers (`LodBandLadder::for_game`, `object_lod_scheme`,
  `DefaultLandTexture::for_game`) or one single, self-documented scoped `if`
  (`byroredux/src/scene/world_setup.rs:1037`, MQ101-demo-only, `#3536`,
  already pointed at a "promote to table if a second arm appears" plan) —
  not a scattered-branch finding.
- `crates/nif/src/version.rs`: `NifVariant` still a flat enum, zero
  feature-flag predicates on it (those live on the separate, documented
  `impl NifVersion` split); "raw-bsver doctrine" comment intact. The three
  commits touching this file since baseline are closed-issue NIF-audit
  fixes (#4148-#4161), spot-checked clean.

**Pattern C (divergent per-game/era decode of one wire structure):** this is
exactly `/audit-esm`'s depth charter, and `docs/audits/AUDIT_ESM_2026-09-21.md`
(HEAD `f97775ca8`) ran a full 8-dimension sweep over `crates/plugin/src/`
**yesterday**. Confirmed fully current: `git log f97775ca8..ee6d3fb39 --
crates/plugin/src/esm/records/ crates/plugin/src/esm/reader.rs
crates/nif/src/version.rs` returns **zero commits** — nothing changed on
these exact paths since that report ran. Its Pattern-C-shaped findings are
cited, not re-derived:
- **MEDIUM** `ESM-2026-09-21-D2-01` — Oblivion's legacy 8-byte `LVLO` rows
  silently dropped, `LVLD` 0x80 flag misread (cross-era `LVLO` shape
  divergence with no per-era branch).
- **MEDIUM** `ESM-2026-09-21-D3-01` — Starfield `TES4` master-flags decoded
  with the Skyrim/FO4 bit layout (small-master bit 0x100 ignored, 0x200
  misread as ESL) — the textbook Pattern C shape this dimension names.
- **LOW** `ESM-2026-09-21-D1-01` — FO76 `HEDR` version drift (279.0 shipped
  vs. 266.0 pinned in code/docs/skill).

**No new findings.**

---

## Dimensions skimmed as unchanged (sub-areas with zero commits in range)

- Dimension 1: strip de-stitch (`crates/nif/src/blocks/strip.rs`), REFR
  Euler dispatcher (`byroredux/src/cell_loader/euler.rs`), camera projection
  (`crates/core/src/ecs/components/camera.rs`) — zero commits since
  baseline, guards spot-checked via fresh grep, not re-traced line-by-line.
- Dimension 2: scene-graph field decomposition (`world_bound.rs`), string
  interning (`crates/core/src/string/`, `name_lookup.rs`) — zero commits,
  guards spot-checked.
- Dimension 3: `crates/plugin/src/esm/reader.rs` (`GameKind`) — zero commits
  since baseline.

## Files Reviewed

`crates/core/src/math/coord.rs`, `crates/nif/src/rotation.rs`,
`crates/nif/src/stream.rs`, `docs/engine/coordinate-system.md`,
`byroredux/src/boot/mod.rs`, `byroredux/src/cell_loader/euler.rs`,
`crates/core/src/animation/mod.rs`, `docs/engine/animation.md`,
`crates/nif/src/blocks/properties.rs`,
`crates/nif/src/import/material/legacy_properties.rs`,
`crates/core/src/ecs/components/world_bound.rs`,
`crates/core/src/string/mod.rs`, `byroredux/src/name_lookup.rs`,
`crates/nif/src/version.rs`, `byroredux/src/scene/world_setup.rs`,
`byroredux/src/cell_loader/{placement_lod,lod_bands,object_lod,terrain,terrain_lod}.rs`,
plus `docs/audits/{AUDIT_LEGACY_COMPAT_2026-09-11,AUDIT_NIFAL_2026-09-21,AUDIT_NIFAL_2026-09-21b,AUDIT_NIF_2026-09-21,AUDIT_ESM_2026-09-21,AUDIT_GAMEPLAY_2026-09-21}.md`
and `.claude/commands/{audit-legacy-compat/SKILL.md,_audit-common.md,_audit-severity.md,_audit-owners.md}`.

## Suggested Next Step

```
/audit-publish docs/audits/AUDIT_LEGACY_COMPAT_2026-09-22.md
```
