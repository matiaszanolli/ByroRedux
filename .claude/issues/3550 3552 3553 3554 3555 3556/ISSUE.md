# Batch: runtime-audit baseline/gate fixes (RT-4, RT-6, RT-7, RT-8, RT-9, RT-10)

Source: `docs/audits/AUDIT_RUNTIME_2026-08-30.md`. All six issues are follow-ups
from the same runtime-telemetry audit sweep and share the
`.claude/audit-baselines/runtime/*.tsv` + `.claude/commands/audit-runtime/SKILL.md`
contract, so they are fixed together.

## #3550 — RT-4: `tex_missing_unique_paths` baseline contract broken by #3349

`#3349` widened `tex.missing` from base-color-only to the full 26-role
`MaterialTextureHandles` walk; every baseline predates that. Naive diff
produces 4 false HIGH regressions (fnv/fo3/fo4/skyrim_se) and masks the 1 real
one (oblivion, tracked as RT-9). Fix: split the metric into
`tex_missing_base_color` (strict gate) and `tex_missing_all_slots`
(informational).

## #3552 — RT-6: P2 gate 5 fails on a healthy build (regex anchor bug)

`docs/smoke-tests/p2-melee-core.sh:334` uses `grep -Eq '^  Inventory: ...'`
against `byro-dbg`'s single-line, `\n`-escaped JSON output — the `^` anchor
can never match. Fix: drop the anchor.

## #3553 — RT-7: `skin_pool_live` grew on 3/5 games

fnv 206→217, skyrim_se 83→133, fo4 248→299. `skin_pool_overflow_attempts`
stays 0 and `skin_pool_max` stays 1364 on all five (no cap pressure) — benign
creep per the documented pattern. Fix: regen fnv/fo4 rows; hold skyrim_se
stale (coupled to RT-8/#3554); treat `skin_pool_live` as advisory relative to
the hard `skin_pool_max` / `skin_pool_overflow_attempts` pair.

## #3554 — RT-8: `entities_total` left the ±2% band on 3/5 games

skyrim_se +15.2% (coupled to the now-resolved RT-2 draw-split bug, not
independently explained — held stale), fo4 +6.26% (benign: cmds moved only
+1.3%), fnv +2.34% (barely past the band). Fix: regen fnv/fo4; hold
skyrim_se stale pending its own bisect.

## #3555 — RT-9: Oblivion gained one genuine texture miss — `earshuman.dds`

Verified against the real archives (not guessed): there is no `facegen\`
top-level folder in any Oblivion BSA. The actual defect is upstream — three
vanilla NIFs (`earshuman.nif`, `earshighelf.nif`, `earswoodelf.nif`) author a
FaceGen-tool export path (`facegen\ears\human\EarsHuman.dds`) instead of the
real archive path (`textures\characters\imperial\earshuman.dds`). Fix: a
basename-fallback texture lookup gated on the `textures\facegen\` prefix.

## #3556 — RT-10: `light_count_directional` baseline is unfalsifiable

The old row was derived from mere presence of a `CellLightingRes` block (always
1, could never fail — matches the closed #3424 finding). The engine now
parses the real per-emitter dump. Fix: regen with the measured
`light_count_directional` (0 on 4 games, 2 on oblivion) plus a new
`light_count_point` row.

## Domain

`renderer`/`esm` mixed — the RT-9 fix lands in
`byroredux-renderer`'s asset-provider path
(`byroredux/src/asset_provider/{archive,texture}.rs`, part of the `byroredux`
binary crate); the rest is baseline-data + skill-doc bookkeeping, not
crate-scoped source.
