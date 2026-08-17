# SF-D9-01: 1,639 vanilla Starfield shader properties point at .bgsm/.bgem files in no archive

**Issue**: #3053
**Severity**: HIGH
**Labels**: `high,nif-parser,legacy-compat,bug`
**Source report**: `docs/audits/AUDIT_STARFIELD_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_STARFIELD_2026-08-16.md` (Dimension 9 — BGSM/BGEM external material flow).

**Location**: `byroredux/src/asset_provider/material.rs`:973 (the `.mat`-suffix CDB gate) and :1073-1075 (the silent BGSM miss)

## Description

Starfield's stub rule for `BSLightingShaderProperty` is `!name.is_empty()`, so a `.bgsm`-named property becomes a `material_reference` stub and `apply_bs_lighting_shader` returns at `crates/nif/src/import/material/dedicated_shader.rs`:112-114 **before copying any inline field** — every remaining value is a parser placeholder.

The mesh then reaches `merge_external_material`, where the CDB arm is gated on:

```rust
if path.ends_with(".mat") && provider.has_starfield_cdb() {
    material.is_pbr = true;
```

so a `.bgsm` path **never receives the `is_pbr = true` Disney-BSDF routing** its `.mat` neighbours on the same mesh do. Dispatch falls to the BGSM arm, `resolve_bgsm` returns `None` because the file does not exist, and the function does `return MergeOutcome::Unresolved` **with no log statement** — `unresolved_material_warning` is only reached by the unknown-extension arm.

## Evidence

Across all **129 Starfield archives there are 0 `.bgsm` and 0 `.bgem` files** (only 20 loose `.mat`, all mod-authored).

Across the 89,276 vanilla NIFs there are **1,639 `.bgsm`/`.bgem` shader-property references spanning 234 distinct paths**, all on shipped content:

```
materials\common\metal\metalgenericpaintedwhite02.bgsm          ×241
materials\shared\t_metal_clean_white.bgsm                       ×73
materials\ships\discovery\sciencestation1.bgsm                  ×48
materials\architecture\hangar\hangar_metalsteel01default01.bgsm ×47
materials\landscape\caves\mine\caveminewall01.bgsm              ×27
materials\weapons\beowulf\beowulf_receiver.bgsm                 ×15
materials\architecture\city\newatlantis\glowwhite.bgem          ×12
```

Host NIFs include `hangarext_wallc02.nif`, `hangarext_floormid01.nif`, `na_lobbyu_chunksext_walla_str01x02_003.nif`.

Re-verified 2026-08-17: the `.mat`-suffix gate at :973 is present and unchanged.

## Impact

1,639 vanilla shader properties get **neither inline values** (the stub rule discarded them) **nor external values** (the file does not exist) **nor PBR routing** (the gate keys on `.mat`). They render from parser placeholders.

The failure is **completely silent** — the unresolved arm returns without logging, so `tex.missing`-style diagnostics do not surface it either. This hits shipped architecture, ships and weapons in New Atlantis and the hangar sets.

## Suggested Fix

Two separable changes:

1. **Route by capability, not by suffix.** On Starfield, a `.bgsm`/`.bgem` reference should still take the CDB/PBR path — the extension is an authoring artefact, not a format statement, since no such files ship.
2. **Make the miss loud.** `MergeOutcome::Unresolved` on a *named* material should reach `unresolved_material_warning` regardless of which arm produced it.

Whether the 234 paths have CDB equivalents needs measuring before choosing a mapping — do not guess a name transform.

## Related

- #2708 (SF-2026-08-12-D9-02 — the REFR-overlay resolver that knows only `.bgsm`/`.bgem`; the mirror image of this gate)
- #2707 (SF-2026-08-12-D8-01 — fabricated PBR pairs on Starfield meshes)
- #3057 (SF-D8-01 — the slot table's zero Starfield coverage, same import path)

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: The routing decision stays at the parser→`Material` boundary, never re-derived at render time
- [ ] **NOT-SILENT**: Every `Unresolved` on a named material logs once
- [ ] **NO-GUESSING**: Any `.bgsm`→CDB name mapping is measured against the archives, not inferred
- [ ] **SIBLING**: #2708's REFR-overlay path fixed consistently with this one
- [ ] **TESTS**: A regression test asserts a `.bgsm`-named Starfield property gets PBR routing or a warning

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3053 --json state` when live state is needed.*
