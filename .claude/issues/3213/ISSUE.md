# #3213 — SKY-2026-08-20-D6-01: water.frag's new blend-normals gate (1158a916) discards every authored water normal layer but the first on 34/34 vanilla Skyrim.esm WATR records

**Issue**: #3213 — https://github.com/matiaszanolli/ByroRedux/issues/3213
**Finding ID**: `SKY-2026-08-20-D6-01`
**Severity**: HIGH
**Dimension**: 6 — real-data rendering (delta-introduced)
**Audit**: `/audit-skyrim` — `docs/audits/AUDIT_SKYRIM_2026-08-20.md` (HEAD `bb0b92f2`, 2026-08-20 comprehensive suite)
**Labels**: high, legacy-compat, renderer, bug
**Filed**: 2026-08-20 · `/audit-publish`

---

**Audit**: `/audit-skyrim` — `docs/audits/AUDIT_SKYRIM_2026-08-20.md` (Dim 6 — real-data rendering), HEAD `bb0b92f2`
**Finding ID**: `SKY-2026-08-20-D6-01`

- **Severity**: HIGH
- **Status**: **NEW — regression introduced inside this delta by `1158a916`** ("feat(water): honor Skyrim normal blend flag", 2026-08-20)

## Location

- `crates/renderer/shaders/water.frag:701` (`blendAuthoredNormals`), `:703` (the third-layer branch) and **`:723-725`** — the unconditional `if (!blendAuthoredNormals) { nMix = nA; }`
- Gate source: `crates/plugin/src/esm/records/misc/water.rs:1309` — `out.blend_normals = Some(sub.data[0] & 0x10 != 0);` (Skyrim-only arm)
- Carrier: `byroredux/src/render/water.rs:287-291` (`noise_falloff`)

## Description

`1158a916` added a gate on Skyrim `WATR.FNAM` bit `0x10`:

```glsl
bool blendAuthoredNormals = push.noise_falloff.y > 0.5;
bool hasAuthoredThirdLayer = noiseMapC != noiseMapA && noiseMapC != noiseMapB;
if (blendAuthoredNormals && (kind == WATER_RAPIDS || hasAuthoredThirdLayer)) {
    ... nMix = normalize(nA + nB + nC * thirdWeight);
} else {
    nMix = normalize(nA + nB);
}
if (!blendAuthoredNormals) {
    nMix = nA;          // <-- discards layer B unconditionally
}
```

Pre-commit the surface was always `normalize(nA + nB)` (or `+ nC`). The trailing statement is **not a blend-mode switch** — it deletes the second normal layer outright. The in-code comment claims "legacy records use the canonical `1.0` default and retain the layered path", which is true for every game *except* Skyrim: Skyrim is the only game that supplies the flag at all, and it supplies it **clear**.

## Evidence

`WATR.FNAM` first byte over every installed master (independent Python ESM walk — all records, no sampling):

```
Skyrim.esm      34 records   FNAM = 0x00 on 34/34
Update.esm      38 records   0x00 x34, 0x08 x2, 0x18 x2
Dawnguard.esm   11 records   0x00 x11
Dragonborn.esm   7 records   0x00 x5,  0x01 x2
----------------------------------------------------
TOTAL           90 records   bit 0x10 SET on 2  (2.2 %)
```

So `blend_normals = Some(false)` on **34/34 of `Skyrim.esm`** and 88/90 across the whole install, and `nMix = nA` fires on essentially all Skyrim water.

`nA` and `nB` are **not** redundant samples even when the three `NAM2`/`NAM3`/`NAM4` slots collapse to one texture (only 3/34 `Skyrim.esm` records author all three; **0/34** author three *distinct* ones). They are sampled with the separately-authored per-layer scroll and UV scale that this same delta went to considerable length to decode:

- `DNAM[100/104/108]` wind directions — 26 / 25 / 25 distinct values across 34 records
- `DNAM[112/116/120]` wind speeds — 26 / 21 / 27 distinct
- `DNAM[172/176/180]` UV scales — 21 / 24 / 23 distinct

Layer B's entire authored parameter set is now dead on Skyrim.

## Impact

100 % of vanilla Skyrim water surfaces render a single-layer normal instead of the two-to-three-layer stack — visibly coarser, more repetitive, more obviously tiled, and a straight visual regression against the pre-delta build. Blast radius is every Skyrim exterior and every water interior. `WATR.FNAM` is decoded Skyrim-only (`water.rs:1309`), so no other title is touched.

**This compounds with #3104.** Skyrim water is currently wrong from two independent directions at once: #3104 flattens every canonical noise amplitude by ~20x (the `normal_magnitude` / Displacement-Starting-Size one-field slip, `0.05` bit-identically on 34/34 records), and this finding then discards all but one of the layers that remain. Fixing either alone leaves Skyrim water visibly wrong; they should be verified together against the same cell.

## Related

- **#3104** (HIGH) — `normal_magnitude` reads the Displacement Starting Size; same 34 records, compounding effect. **Fix and verify together.**
- **#3105** — rain/displacement start-size swap across Oblivion / FO3-FNV / Skyrim.
- **#3109** — the test fixtures that pin the swap.
- **#3152** (`LC-D2-01`) — the *same defect class* on the NIF path: an unsourced flag bit inverting a canonical default. Note its **DO-NOT-FIX-BY-ANALOGY** box.
- `f933ecbf` / `71c694b0` and the rest of the Skyrim DNAM-tail work whose decoded per-layer parameters this gate strands.

## ⚠️ DO NOT FIX BY ANALOGY

The **WATR-side `FNAM` bit `0x10` decode is CORRECT** and empirically supported: `Update.esm` ships the paired EditorIDs `DefaultWaterFlow` (`FNAM=0x08`) / `DefaultWaterFlowBlend` (`FNAM=0x18`), and `RiverWaterFlow` / `RiverWaterFlowBlend` with the identical `0x08 -> 0x18` delta — the data itself names bit `0x10` "Blend". The **NIF-side `blend_normals` bit 16** (#3152) is the undefined one. Different bits, different formats. Do not "fix" the parser here.

## Suggested Fix

Two separable steps.

1. **Delete the trailing `if (!blendAuthoredNormals) { nMix = nA; }`** (`water.frag:723-725`). The gate already has a legitimate effect through the third-layer branch, and no source says "blend normals off" means "one layer".
2. **Source the bit before re-adding any semantic.** The strongest available evidence is the EDID pairing above: *both* members of each pair carry `NAM5` flow maps, so on this evidence "Blend" reads as *blend the flow-map normal with the noise normals* — a different switch entirely, touching only flow water.

## Completeness Checks
- [ ] **SIBLING**: check the other authored-normal consumers in `water.frag` (`nC` / flow-map path) for the same "gate deletes data" shape
- [ ] **CANONICAL-BOUNDARY**: the semantic decision stays at the parser->`WaterMaterial` boundary; the shader must not re-derive per-game meaning from a raw flag bit
- [ ] **SHADER-SYNC**: if the `noise_falloff` push-constant layout changes, `byroredux/src/render/water.rs` and `include/bindings.glsl` move in lockstep, and `water.frag.spv` is recompiled with plain `-V`
- [ ] **TESTS**: a regression test pins that a `FNAM = 0x00` Skyrim record still reaches the multi-layer path
- [ ] **VERIFY-WITH**: re-check against #3104 in the same cell — both defects are live on the same 34 records
