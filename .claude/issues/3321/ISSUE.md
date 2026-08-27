# FNV-2026-08-26-D1-03

**Issue**: #3321
**Filed**: 2026-08-26 from `docs/audits/AUDIT_FNV_2026-08-26.md`

---

**Severity**: MEDIUM
**Dimension**: 1 — Cell Loading End-to-End
**Status**: NEW
**Source**: `docs/audits/AUDIT_FNV_2026-08-26.md` (audit HEAD `d6e16c90`)

> **Orchestrator note**: the source dimension reported 295 LOD quads corpus-wide; independent re-verification at publish time confirmed the *premise* (FNV does ship a systematic object-LOD family) by listing 52 entries under `meshes\landscape\lod\wastelandnv\blocks\` in `Fallout - Meshes.bsa` alone. The exact corpus-wide total should be re-derived as the first step of any fix — treat 295 as unconfirmed.


**File**: `docs/engine/exal.md:364-379`, `byroredux/src/cell_loader/placement_lod.rs:288-300`, `byroredux/src/cell_loader/object_lod.rs:105-107`, `byroredux/src/cell_loader/lod_bands.rs:140-150`

**Premise verified**: both gates are live and correctly reflect what the docs claim.
`LodBandLadder::for_game` returns `None` for anything but Skyrim/FO4
(`lod_bands.rs:141-145`), so `stream_object_lod_blocks` returns immediately on FNV;
`stream_placement_lod_blocks` is gated to `GameKind::Oblivion` only. The checked-in
justification is:

> `exal.md:364` — **FO3/FNV ship neither LOD scheme for distant objects.** #2086 probed
> every vanilla FO3/FNV archive … and found zero `distantlod\` entries;
> `Fallout - Meshes.bsa` carries only 2 `_far.nif` files total (one-off landmark
> assets, not a systematic scheme).

**Evidence** — re-probing `Fallout - Meshes.bsa` (v104, 19,587 entries) confirms the
first half and falsifies the second and the conclusion:

```
_far.nif entries in FNV "Fallout - Meshes.bsa":   0     (doc says 2 — that count is FO3's)
distantlod\ entries:                              0     (doc correct)
meshes\landscape\lod\  entries:                2663
```

Splitting `meshes\landscape\lod\wastelandnv\` (1,655 entries) by subfolder shows two
*distinct* families, not one:

```
root  (terrain LOD): 1360   level4=1024  level8=256  level16=64  level32=16
blocks\ (object LOD): 295   level4=295
```

The 1,360 root NIFs pair 1:1 with the 1,360 baked terrain textures the engine
already names correctly (`env_translate.rs:134`):
`textures\landscape\lod\wastelandnv\{diffuse,normals}\wastelandnv.n.level<L>.x<qx>.y<qy>.dds`
— verified present, same 1024/256/64/16 level split.

The `blocks\` family is object LOD. Extracted and decoded
`meshes\landscape\lod\wastelandnv\blocks\wastelandnv.level4.x24.y-12.nif`:

```
Gamebryo File Format, Version 20.2.0.7
BSMultiBoundNode / BSSegmentedTriShape / BSShaderPPLightingProperty / BSShaderTextureSet
Data\Textures\Landscape\LOD\WastelandNV\Blocks\WastelandNV.Buildings.dds
Data\Textures\Landscape\LOD\WastelandNV\Blocks\WastelandNV.Buildings_n.dds
```

i.e. a combined per-quad building mesh against a single shared world atlas —
`textures\landscape\lod\wastelandnv\blocks\wastelandnv.buildings[_n].dds` are both
present in `Fallout - Textures*.bsa`. 295 level-4 quads covering the whole
worldspace is systematic by any definition, and it is a clean sibling directory of
the terrain quads the engine already resolves.

`#2086` (CLOSED) reached the opposite conclusion by guessing
("suggesting Bethesda folded landmark-object LOD into the terrain-LOD block system")
without opening a `blocks\` NIF; `exal.md:374-379` then encoded the guess as the
recorded design rationale, pointing at FO3's `washmontop`/`dcworld03` landmark
sub-folders — which are a different (FO3-only) thing.

Not a duplicate of any open issue: `/tmp/audit/fnv/open_issues.txt` contains only
`#3307` (active VWD full-model culling) and `#3142` (VWD reconcile lock churn) in
this area; neither is "FNV object LOD is unconsumed", and `#2086` is closed.

**Impact**: On the WastelandNV 7×7 grid every distant building silhouette — the
Lucky 38, the Strip skyline, REPCONN, HELIOS One, Vegas ruins — is absent beyond
`radius_unload`, on the reference title, while the assets sit in the archive the
engine already has open. Terrain LOD renders (synth heights + baked diffuse), so the
horizon reads as bare geometry where the game shows a skyline.

**Fix sketch**: Add a `FalloutLegacy` object-LOD arm keyed on
`meshes\landscape\lod\<world>\blocks\<world>.level4.x<qx>.y<qy>.nif` (single level,
so it needs no `LodBandLadder`), reusing `object_lod.rs`'s existing quad
residency/eviction shape and resolving the shared `blocks\<world>.buildings[_n].dds`
atlas once per worldspace. Correct `exal.md:364-379` and the `placement_lod.rs:294`
comment first — as written they will re-close any future report of this gap.

---

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
