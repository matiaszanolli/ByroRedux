# #3198 — FNV-2026-08-20-D1-02: all 78 vanilla FNV WATR records classify WaterKind::Calm — the token set is Skyrim vocabulary, so WaterFlow and the entire WATAL current half are unreachable on the reference title

**Source**: `docs/audits/AUDIT_FNV_2026-08-20.md`
**Filed**: 2026-08-20 · **HEAD**: `bb0b92f2`
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/3198

---

- **Severity**: MEDIUM
- **Dimension**: FNV Dim 1 — Cell Loading End-to-End (WATAL flow arm)
- **Location**: `byroredux/src/env_translate.rs:899-947` (the `WaterKind` classifier inside `resolve_water_material`), consumers at `:961-1044` (flow synthesis + scroll bias) and `byroredux/src/cell_loader/water.rs:531` / `:858` (the `if let Some(flow) = flow` inserts)
- **Status**: NEW

## Description

A cell's water becomes `Rapids` / `River` — and therefore gets a `WaterFlow` component, `foam_strength > 0`, and a flow-biased UV scroll — only if one of five signals fires:

```rust
lowered.contains("rapid")                  // EDID token
lowered.contains("waterfall") | "falls" | "river" | "stream"
rec.flow_noise_texture_path_is_enabled()   // NAM5 — Skyrim SE+
has_authored_linear_flow                   // NAM0 — FO76/Starfield
authored_flow_speed >= WaterFlow::SPEED_RAPIDS
```

**None of the five can fire on FNV.** The token list is Skyrim vocabulary; FNV names its moving water `Creek*`, `Potomac*`, `*Spill`, `*Fountain`. And FNV's `WATR` sub-record set does not contain `NAM0` or `NAM5` at all.

## Evidence

Full sub-record census over all 78 `FalloutNV.esm` `WATR` records (independent byte-level GRUP walk):

```
EDID 78 · NNAM 78 · ANAM 78 · FNAM 78 · MNAM 78 · XNAM 73 · DATA 78 · DNAM 70 · GNAM 78 · SNAM 32 · FULL 40
NAM0: 0     NAM1: 0     NAM2/3/4: 0     NAM5: 0
```

The complete EditorID roster contains **zero** occurrences of `rapid` / `waterfall` / `falls` / `river` / `stream`. The flowing-water records FNV actually ships are:

`CreekWater01`, `CreekWater02nv`, `CreekWater02AVGnv`, `CreekWater02nvbetter`, `RockCreekEstatesWater`, `Potomac`, `PotomacNRShallow`, `DLC03TBPotomacWater`, `ToxicSpillPuddle`, `TenPenWaterFountain`, `VStripULFountain` — **all classify `Calm`.**

`Potomac` (`00030009`) is the **`WRLD` `NAM2` default water for WastelandNV** and nine other worldspaces, so the exterior grid the engine benches on is classified `Calm` too.

## Impact

**A large share of this delta's WATAL work cannot execute on the game the engine is tuned against.** Three shipped WATAL behaviours are dead on the reference title and cannot regress visibly there:

1. the physics current (`WaterFlow` → `WaterCurrentVolume` → buoyancy drift, `crates/physics/src/water.rs`) — **the entire WATAL current half is unreachable on FNV**;
2. the flow-biased shader scroll (`mat.scroll_a/b/c` keep their static defaults);
3. `foam_strength` (stays at the `WaterMaterial` default instead of `0.20` / `0.85`).

Because FNV is the title with the bench-of-record and the only non-Skyrim smoke gate, the flow half of WATAL is exercised by **no** FNV path. That is also why the `wind_direction` radians unit error (#3144) is currently invisible here: the value is read only inside the `!matches!(kind, WaterKind::Calm)` block.

**Ordering hazard:** fixing this classifier *without first* fixing #3144 turns a dormant bug into a live one — every FNV river would get a flow direction of `(cos 90 rad, sin 90 rad) = (-0.448, 0.894)`, a fixed ~117° heading error.

## Related

- #3144 — the `wind_direction` degrees-into-radians error this masks. **Fix #3144 first, or gate `WaterFlow` synthesis on a finite, converted direction.**
- #3154 (`LC-D5-02`) — the *other* `WaterKind` classifier, `material_translate.rs::water_kind_from_mesh_name`, has a third token set including `canal`; but it is called only from `byroredux/src/scene/nif_loader.rs:1028` (the loose-NIF path), so it never runs during an FNV cell load and does not compensate.
- #3184 (`NIFAL-D1-02`) — the `WaterKind` → `foam_strength` literal divergence on the same enum.

## Suggested Fix

Add the FO3/FNV vocabulary to the shared token list — at minimum `creek`, `canal` (already in the mesh-side list) and `spill`. Better: hoist the two divergent token sets into one shared classifier as #3154 proposes, so a token added for one producer reaches both.

Do **not** land it before the #3144 radians conversion is fixed. Pin with a fixture asserting `CreekWater02nv` → `WaterKind::River`.

---
*Filed from `docs/audits/AUDIT_FNV_2026-08-20.md` (Dim 1). Verified against HEAD `bb0b92f2` — the five-signal classifier is live at `env_translate.rs:924-947`.*

## Completeness Checks
- [ ] **SIBLING**: the mesh-side classifier (`water_kind_from_mesh_name`) gets the same token set, or both collapse to one function
- [ ] **CANONICAL-BOUNDARY**: classification stays at the ESM→`WaterMaterial` boundary — no per-game token branch in the renderer or `water.frag`. See `/audit-nifal`.
- [ ] **TESTS**: a regression test pins `CreekWater02nv` → `River` **and** asserts that at least one vanilla FNV record classifies non-`Calm`, so the arm cannot go dark again
- [ ] **ORDER**: #3144 landed (or `WaterFlow` synthesis gated on a converted direction) before this classifier is widened
