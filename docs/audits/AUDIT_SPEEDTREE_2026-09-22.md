# SpeedTree Subsystem Audit — 2026-09-22

**HEAD**: `ee6d3fb39` · **Baseline**: [AUDIT_SPEEDTREE_2026-09-11.md](AUDIT_SPEEDTREE_2026-09-11.md) (11 days, ~85 commits touching the wiring cross-cut files, 2 touching `crates/spt/` itself) · **Audited**: Dimensions 1–6, delta-first · **Unchanged since baseline (skimmed)**: Dimension 5 (`crates/spt/src/tag.rs`, `crates/spt/docs/format-notes.md` — zero commits; guard spot-checked only)

**Scope**: `crates/spt/` (`byroredux-spt`) — the `.spt` TLV parameter-section walker
(`parser.rs`, `stream.rs`, `tag.rs`, `version.rs`, `scene.rs`) and the placeholder-billboard
importer (`crates/spt/src/import/mod.rs`) — plus the cross-cut wiring:
`byroredux/src/cell_loader/references/{synth_child,import}.rs`,
`byroredux/src/cell_loader/nif_import_registry.rs`,
`byroredux/src/cell_loader/spawn/mesh_instance.rs`, `byroredux/src/scene/nif_loader.rs`,
`crates/plugin/src/esm/records/tree.rs`, `byroredux/src/systems/billboard.rs`,
`byroredux/src/material_translate.rs`.

**Execution**: single-pass, solo, in-context, no sub-agents dispatched (required for this
run). Scratch notes and evidence for every dimension are in
`/tmp/audit/speedtree/dim_1.md` … `dim_6.md`.

**Depth**: `deep` — the on-disk corpus harness was re-run for all three games (not skipped),
in addition to the source-level review.

**Method**: refreshed `/tmp/audit/issues.json` (4,506 issues, open+closed) and read the
current `docs/audits/AUDIT_EXTERIOR_2026-09-21.md` for cross-cutting tree-LOD findings
before starting. Ran `git log --since=2026-09-11` per-file (not per-directory — several of
this subsystem's directories carry heavy non-SPT traffic) for every `Paths:` entry in the
SKILL, diffed every hunk that touched an SPT-relevant identifier, then read each changed
region in full at current HEAD (not just the diff) against that dimension's checklist.
Dimensions with zero qualifying commits (5) got a guard spot-check only, per delta-first
scoping.

- `cargo test -p byroredux-spt -j 4` → **54/54 pass** (default lane, unchanged from
  baseline's 54/54).
- `cargo check -p byroredux-spt --features recon --examples -j 4` → clean.
- `cargo test -p byroredux-spt --features recon --lib -j 4` → **59/59 pass**.
- `cargo test -p byroredux parse_and_import_spt` → 2/2 pass.
- `cargo test -p byroredux vanilla_tree` (real archive data, no `--ignored` needed — these
  two are unconditional) → 2/2 pass (`vanilla_tree_models_all_resolve`,
  `vanilla_tree_icons_all_resolve`).
- **Deep-only corpus re-run**, release, one game at a time (`-j 4`):
  `BYROREDUX_OBL_DATA=… cargo test -p byroredux-spt --release --test parse_real_spt -- --ignored parse_rate_oblivion_spt --nocapture`
  and the `_FO3_DATA` / `_FNV_DATA` siblings → **Oblivion 113/113 files, 96.46% coverage**
  (same 4 unknown-tag files as baseline: `treems14willowoakyoungsu`, `treecottonwoodsu`,
  `treems14canvasfreesu`, `shrubms14boxwood`, all tag 768), **FO3 10/10, 100.00%**, **FNV
  10/10, 100.00%** — all three exactly match the 2026-08-30/2026-09-11 baseline numbers, no
  drift.

---

## Dedup pass (mandatory)

Fresh `gh issue list --repo matiaszanolli/ByroRedux --limit 6000 --state all` this run →
`/tmp/audit/issues.json` (4,506 rows). All three findings from
`AUDIT_SPEEDTREE_2026-09-11.md` map to closed issues with fixes verified in place at HEAD:

| Issue | 09-11 finding | Fix commit | Verified at HEAD |
|---|---|---|---|
| #4118 | D3-01 (`import.rs` still asserted the disproved CNAM/BNAM premises, LOW) | `31fa4596b` | Both premises corrected at `import.rs:515-537`; wording now matches `crates/spt/src/import/mod.rs` and `crates/plugin/src/esm/records/tree.rs` exactly — re-read directly, no residual stale text found anywhere in the cross-cut file list. |
| #4120 | D1-01 (three files still described a "geometry tail," LOW) | `31fa4596b` | `parser.rs`'s module doc, `import/mod.rs`'s "Beyond the placeholder" section, and all three `import.rs` comments reworded to the corrected `#3808` framing — confirmed by direct diff read, not just the commit message. |
| #4122 | D1-02 (46%-of-corpus `tail_offset` desync, MEDIUM) | *(not a fix — a tracking issue)* | Still **OPEN**. Filed 2026-09-19, after the 09-11 report explicitly said no tracking issue existed. This audit's corpus re-run reproduces the identical coverage numbers, consistent with no code change to the desync since either report. Carried forward, not re-reported as new (see Dimension 1). |

No open issue in the fresh search covers the one new finding below (`overlay`, `recompute`,
`4229`, `wood`, `boxwood`, `elderberry` all searched; nearest hits are `#4229` itself,
CLOSED, and `#1819`, CLOSED, both cited as *Related* below, not duplicates).

### Cross-reference: `docs/audits/AUDIT_EXTERIOR_2026-09-21.md`
That report's HIGH finding **EXT-D3-2026-09-21-01** (`WindField.direction` has no defined
frame; four consumers disagree) explicitly names `byroredux/src/systems/billboard.rs:230,240`
(SpeedTree's wind-lean axis) as one of the four divergent readings, and explicitly co-owns
it to `/audit-speedtree`. Verified the citation against current source (both line numbers
match exactly). **Not re-reported here** — see Dimension 2. That report's comparison table
also independently confirms `#4122` OPEN and notes distant trees still route through `.bto`
(EXAL object-LOD, out of this audit's scope per the SKILL's own "Not here" note).

---

## Findings

### SPT-2026-09-22-D6-01: `translate_material`'s #4229 overlay-divergence recompute treats every non-BGSM `metalness_override`/`roughness_override` as keyword-classifier output, reopening the #1819 foliage substring-collision class for the SpeedTree placeholder

- **Severity**: HIGH
- **Dimension**: NIFAL Material Translation for Placeholders
- **Location**: `byroredux/src/material_translate.rs:562-583` (`overlay_changed_base_color` /
  `recomputed_pbr`), `:159-175` (`ResolvedPaths::source_base_color` doc), `:3298-3398`
  (`overlay_pbr_divergence_tests`, the #4229 regression module);
  `crates/spt/src/import/mod.rs:395-397` (`metalness_override: Some(0.0)`,
  `roughness_override: Some(0.85)`)
- **Status**: NEW (introduced by `a5a6407d4` / #4229, 2026-09-12 — one day after the
  2026-09-11 baseline, so not caught by any prior SpeedTree audit cycle; reopens the defect
  *class* closed by **#1819** (CLOSED) — #1819's own fix is intact and not itself touched)
- **Description**: `translate_material` resolves `Material.metalness`/`roughness` as
  `recomputed_pbr.or(source.metalness_override).unwrap_or(NAN)` (roughness identically).
  `recomputed_pbr` is computed — silently overriding whatever `source.metalness_override`
  already holds — whenever `overlay_changed_base_color && !source.bgsm_pbr_scalars_authored
  && source.metalness_override.is_some()`. Both the `ResolvedPaths::source_base_color` field
  doc and the `translate_material` call-site comment justify the `metalness_override.is_some()`
  half of that guard by asserting it "mirrors `classify_legacy_pbr`'s own... gate... it's
  `Some` exactly when the classifier's result was stored at import time" — true for every
  `crates/nif` mesh extractor, but **false** for `crates/spt`'s placeholder importer.
  `placeholder_billboard_mesh` sets `metalness_override: Some(0.0)` /
  `roughness_override: Some(0.85)` directly, not via the keyword classifier — these are the
  deliberate anti-collision overrides **#1819** added specifically because
  `classify_pbr_keyword`'s unbounded substring matching mis-tags foliage: `"wood"` inside
  `ShrubBoxwoodLeaves*.dds`, and `"ic"+"e"` across the `generIC`/`Elderberry` word seam in
  `ShrubGenericElderberryLeaves*.dds` (both vanilla Oblivion tree species, per #1819's own
  evidence — the same two names this SKILL's own Dimension-6 checklist cites verbatim). The
  #4229 guard excludes only the BGSM-authored case; it cannot distinguish a
  keyword-classified `Some` from an explicitly-set-for-a-different-reason `Some`, so a
  SpeedTree placeholder whose overlay-resolved base-color path diverges from its own
  un-overlaid path has its `Some(0.0)`/`Some(0.85)` protection discarded and replaced with a
  fresh `classify_pbr_keyword` run against the new path — reopening the exact wood/glass
  collision #1819 closed, now gated behind an overlay swap rather than unconditional.
- **Evidence**:
  ```rust
  // material_translate.rs:713-722
  metalness: recomputed_pbr.as_ref().map(|p| p.metalness).or(source.metalness_override).unwrap_or(f32::NAN),
  roughness: recomputed_pbr.as_ref().map(|p| p.roughness).or(source.roughness_override).unwrap_or(f32::NAN),
  ```
  The #4229 regression test `overlay_swap_recomputes_pbr_from_the_effective_path`
  (`:3321-3342`) proves the mechanism *replaces* a `Some(0.9)/Some(0.55)` override with a
  freshly classified value whenever the overlay path diverges and
  `bgsm_pbr_scalars_authored` is `false` — exactly `placeholder_billboard_mesh`'s
  configuration (`ImportedMaterial::default()` gives `bgsm_pbr_scalars_authored: false`,
  `crates/nif/src/import/types.rs:862`, and the placeholder never sets it otherwise). The
  three existing tests in that module (no-divergence pass-through, divergence-recomputes,
  BGSM-authored-protected) leave an untested gap for exactly this case: a *non-keyword*
  `Some` override under a diverging overlay.
- **Impact**: Reachability is narrow but real, not purely theoretical, and per
  `_audit-severity.md` severity is scored on impact, not likelihood. The trigger requires
  `build_refr_texture_overlay` (`byroredux/src/cell_loader/refr.rs`) to actually populate
  `alt_texture_ref`/`land_texture_ref`/`texture_slot_swaps` for a TREE REFR from an
  XATO/XTNM/XTXR/XMSP sub-record. Those are FO4-vocabulary fields; TREE/`.spt` exists only
  on Oblivion/FO3/FNV, and `refr.rs:1026-1040`'s own documented `FO3-D3-001`/`#1887`
  provenance caveat confirms none of those three games natively author a *successful*
  overlay this way (FNV's `XATO` tag means something else there — an Activation-Prompt
  string misread as a garbage FormID that near-certainly misses `texture_sets`, i.e. an
  inert empty overlay, not a real swap). So **vanilla content on all three `.spt`-bearing
  games cannot reach this today**. What can: hand-authored mod content adding one of those
  sub-records directly to a TREE REFR (no record-type gate blocks it in
  `build_refr_texture_overlay`), or any future engine change that starts authoring
  per-REFR texture overrides more broadly for these games. When it fires, the effect
  matches #1819's own documented impact: visual-only, but the Elderberry-class collision
  (the `"ice"/"glass"` arm, roughness 0.1) crosses the RT-reflection threshold
  (`roughness < 0.6` in `triangle.frag`), rendering the affected leaf billboard
  mirror-smooth. Per the severity matrix's unconditional rule ("wrong/divergent `Material`
  out of NIFAL `translate_material` → at least HIGH") and #1819's own precedent for the
  identical defect class, this is scored HIGH rather than downgraded for the narrow
  trigger.
- **Related**: #1819 (CLOSED — the original substring-collision bug this reopens a path
  to; its own fix is intact, not regressed), #4229 (CLOSED — introduced this gap), #3529
  (the sibling NaN-transparency fix on the same override fields, same defensive-in-depth
  pattern this finding argues for extending).
- **Suggested Fix**: Narrow the `recomputed_pbr` guard so it fires only for materials the
  keyword classifier actually produced. Cheapest correct fix: add an explicit
  `pbr_classified_at_import: bool` to `ImportedMaterial` (default `false`), set `true` only
  by `crates/nif`'s `classify_legacy_pbr` call sites, and gate `recomputed_pbr` on that flag
  instead of `!bgsm_pbr_scalars_authored`. As a stopgap, `crates/spt/src/import/mod.rs`
  could set `bgsm_pbr_scalars_authored: true` on the placeholder material (the existing
  BGSM-authored-scalars guard already treats that as "authoritative, do not
  reclassify" — exactly SpeedTree's situation) — functionally correct today but a
  misleading field name for a producer that ships no BGSM.

---

## Dimension summary (every dimension enumerated)

| Dimension | Findings | Notes |
|---|---:|---|
| 1 — Walker Byte-Accounting | **0 new** | Only source change since baseline is the `#4118`/`#4120` comment-only sweep + one test rename (`31fa4596b`); full re-read of `parser.rs`/`stream.rs` confirms every checklist item (tag-kind sizing, `MaybeStringElseBare` re-sync incl. zero-length/EOF edge cases, 64 KiB byte-count caps, five enumerated fatal `Err` conditions, unconditional LE reads) still holds. Corpus re-run reproduces baseline's exact coverage numbers. The carried-forward MEDIUM (tail-offset desync on 46% of the corpus) is tracked as **#4122** (OPEN since 2026-09-19) — not re-reported. |
| 2 — Placeholder Fallback | **0 new** | `import_spt_scene` still unconditional (no `Err` path); all six named guards present and passing; OBND→BNAM→MODB→default precedence, `-Z`/CCW winding, `clamp_billboard_extent`'s finite-gate, and `BsRotateAboutUp`'s world-Y lock all re-verified against current source. `billboard.rs` has zero commits since baseline; its one live defect (`WindField` frame sign, lines 230/240) is already reported as EXT-D3-2026-09-21-01 in the exterior audit — cited, not duplicated. |
| 3 — TREE→Billboard Wiring | **0 new** | Highest commit traffic of any dimension (9 commits to `synth_child.rs` alone), but a per-hunk grep for every SPT-specific identifier across the full delta window found zero touching the dispatch/cache-key/resolve logic. Full re-read of `resolve_spt_model_path`, `resolve_tree_icon_path`, `spt_cache_key`, `parse_and_import_spt`'s `CachedNifImport` literal, and `mesh_instance.rs`'s mesh-level billboard/`SpeedTreeWind` attach confirms every checklist item; the one baseline finding (#4118) verified fixed with no residual stale text anywhere. The `phantom_bounds`/`root_transform` fields a water feature landed alongside this wiring only feed the `mesh_water` branch, never reached by SpeedTree placeholders. |
| 4 — Per-Game Variants & Route Divergence | **0 new** | `version.rs` unchanged; of `nif_loader.rs`'s 12 commits, only 3 touch SPT-adjacent lines, none touching the `.spt`-branch logic itself. `detect_variant` confirmed still diagnostic-only at both of its exactly two production call sites (both feed only `.tag()` into a log line). The documented `SptImportParams::default()` gap on the loose `--tree` route is unchanged. |
| 5 — Tag Dictionary | **0 new** | Zero commits to `tag.rs` or `format-notes.md` since baseline — guard spot-checked only (`cargo test -p byroredux-spt "tag::"`, 8/8 pass), plus the corpus re-run's identical coverage numbers and identical 4-file unknown-tag set as the strongest available no-drift signal. |
| 6 — NIFAL Material Translation | **1 new (HIGH)** | `crates/spt/src/import/mod.rs`'s placeholder defaults unchanged and correct. `byroredux/src/material_translate.rs` had 13 commits since baseline — the most of any file in scope — and one (`a5a6407d4`, #4229, 2026-09-12) lands inside the PBR-resolution code this dimension owns and reopens a path to the #1819 foliage-classifier substring-collision class for any TREE REFR that picks up a texture-slot overlay. See SPT-2026-09-22-D6-01. |

**Totals**: 6 dimensions, **1 new finding** — **1 HIGH**, 0 CRITICAL, 0 MEDIUM, 0 LOW.
Dimensions **1, 2, 3, 4 and 5 produced no new findings.** One prior finding (#4122, MEDIUM)
remains open and tracked; two prior findings (#4118, #4120) verified fixed.

---

## Summary

Every finding from `AUDIT_SPEEDTREE_2026-09-11.md` is accounted for: `#4118` and `#4120`
are fixed and verified in place with no residual stale text anywhere in the cross-cut file
list (the prior cycle's own pattern — a sweep landing in some but not all files carrying a
claim — did not recur this time), and `#4122` (the `tail_offset` desync) now has the
dedicated tracking issue the 09-11 report asked for, still correctly OPEN with no fix
landed and no behavioural drift (the corpus re-run reproduces identical coverage numbers).

The wiring cross-cut (Dimensions 1-5's `Paths:`) saw heavy commit traffic since the
baseline — ~85 commits touched the listed directories — but a disciplined per-file,
per-hunk delta scope (rather than trusting directory-level `git log`) found that none of it
actually touched SpeedTree-specific logic; every dimension's re-read confirms the baseline's
"holds" verdict rather than finding new drift.

The one new finding is in Dimension 6, the dimension explicitly flagged by the SKILL as
"flows through the single NIFAL boundary" and therefore in scope even though the change
that caused it (`material_translate.rs`) is mostly owned by `/audit-nifal`/`/audit-renderer`
territory. `#4229` (landed the day after the last SpeedTree audit) fixed a real bug for its
own target case (a REFR overlay swap onto a *keyword-classified* NIF material) but its
guard condition is broader than its own justifying comment claims, and the one production
code path outside `crates/nif` that also populates `metalness_override`/`roughness_override`
— this subsystem's placeholder billboard — falls into the gap. Reachability from vanilla
content is blocked by a separately-documented, unrelated game-format mismatch
(`FO3-D3-001`/`#1887`), so this is not an active vanilla-content bug today, but the severity
floor for a wrong/divergent NIFAL `Material` is explicit and impact-based, matching how the
original #1819 collision (fixed by the very overrides this gap can now bypass) was itself
scored.

### Suggested next step

`/audit-publish docs/audits/AUDIT_SPEEDTREE_2026-09-22.md`

Labels: `speedtree` + `nifal` (domain) + `high` (severity) + `bug` (type) on
SPT-2026-09-22-D6-01; no `game:*` label (the defect is per-mechanism, not per-game — it
would apply identically on Oblivion, FO3 or FNV once reachable, and is not reachable on any
of them from vanilla content today).
