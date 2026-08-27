## #3158 — SCR-D6-2026-08-20-01: #2940's HasPerk fix reads a component the player never gets and only FO4+ NPCs ever get — still structurally 0.0 on Skyrim, FO3 and FNV
State: OPEN
Labels: bug medium scripting game:fnv game:fo3 game:fo4 game:skyrim 

- **Severity**: MEDIUM
- **Dimension**: 6 — Scripting Runtime Systems
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/condition.rs`:692-703 (the read) · `byroredux/src/npc_spawn.rs`:204-215 (the only production writer) · `crates/plugin/src/esm/records/actor/mod.rs`:1082-1086 (the `PRKR` parse arm) · `crates/plugin/src/esm/reader.rs`:236-238 (the gate) · `byroredux/src/scene.rs`:1170-1220 (the player-entity component set)
- **Status**: NEW

## Description

`a605ee93` (Fix #2940) correctly repointed `ConditionFunction::HasPerk` from the
dead `PerkList` projection to the canonical `byroredux_core::character::Perks`,
and the FormID spaces line up — `Perks::perk_form_id` is written through
`remap_fid` and `param_1` is load-order remapped by `remap_condition_form_ids`
for indices 448/449, so the comparison is apples-to-apples.

What the fix did **not** change is *who writes `Perks`*.

There is exactly one production writer, `spawn_npc_entity`, fed by
`NpcRecord::perks`, which is populated only inside the
`captures_av_props = game.uses_actor_value_properties()` gate — i.e.
**`Fallout4 | Fallout76 | Starfield` only**.

Separately, the **player** entity (`scene.rs`, the `PlayerEntity` body) is given
`Transform`, `GlobalTransform`, a character controller, `CollisionShape`,
`RigidBodyData` and a `FormIdComponent`, and nothing else from the CHARAL family
— no `Perks`, no `ActorValues`.

`HasPerk`'s own doc-comment (`condition.rs`:142) claims indices **449 (FO3/FNV)**
and **448 (Skyrim)**. For neither of those families, and for the player in *any*
game including FO4, can the `world.get::<Perks>()` at `condition.rs`:697 ever
return `Some`.

## Evidence

```rust
// crates/scripting/src/condition.rs:696-698 — the read
use byroredux_core::character::Perks;
let Some(perks) = world.get::<Perks>(entity) else {
    return 0.0;
};
```

```rust
// byroredux/src/npc_spawn.rs:204-208 — the only writer
// Perks (FO4+ `PRKR`) — skip the component entirely when the NPC has none.
if !npc.perks.is_empty() {
    world.insert(placement_root, Perks { .. });
}
```

```rust
// crates/plugin/src/esm/reader.rs:236-238 — the gate on the only producer of npc.perks
pub fn uses_actor_value_properties(self) -> bool {
    matches!(self, Self::Fallout4 | Self::Fallout76 | Self::Starfield)
}
```

`crates/plugin/src/esm/records/actor/mod.rs`:360 says it outright: *"Populates a
`Perks` component at spawn. **Empty for pre-FO4 NPCs.** Gated on …"*.

`grep -rn "Perks" byroredux/src crates` outside `crates/core/src/character`
returns those two sites plus `condition.rs` and two save-registry notes — **no
player-side insert anywhere**.

## Impact

Perk-gated dialogue, quest and package CTDAs silently evaluate **false** for the
player in every game, and for every NPC outside FO4 / FO76 / Starfield.

This is the *same observable behaviour* CHAR-D3-01 (#2940) described and was
closed for, so the closed issue reads as resolved while the user-visible symptom
is unchanged for the reference title (Skyrim) and for the reference-of-record
(FNV).

A condition returning `0.0` is the Bethesda-correct safe default in isolation,
which is exactly why it is silent: there is no log, no telemetry and no test that
distinguishes *"this actor genuinely lacks the perk"* from *"no actor in this
game can ever have one"*.

## Related

- **#2940 (CLOSED)** — the fix is correct as far as it goes; this is the
  untouched half upstream of it
- #2947, #2944 — sibling CHARAL perk findings
- The ESM-side question *"does Skyrim `NPC_` carry `PRKZ`/`PRKR`, and if so
  should `uses_actor_value_properties` gate it?"* belongs to `/audit-esm` Dim 4.
  **This finding deliberately does not assert the Skyrim wire format.**

## Suggested Fix

Two independent halves:

**(a)** Give the player entity a `Perks` component (empty is fine) at spawn, so
the distinction between "checked and absent" and "unrepresentable" exists at all,
and so a future `AddPerk` effect has somewhere to write.

**(b)** Either widen the `PRKR` parse gate past `uses_actor_value_properties` for
the games whose `NPC_` actually carries it, **or** add a one-line `log::debug!`
at the `else` arm of `condition.rs`:697 naming the game, so the structural zero is
at least diagnosable.

A regression test asserting `HasPerk` is non-zero for a Skyrim-parsed NPC would
pin whichever choice is made.

---
**Source**: `docs/audits/AUDIT_SCRIPTING_2026-08-20.md` (finding `SCR-D6-2026-08-20-01`)

## Completeness Checks
- [ ] **SIBLING**: The other CHARAL components with the same single-writer/`uses_actor_value_properties` shape (`ActorValues`, `CharacterLevel`, `Background`) checked for the same reachability hole, on the player especially
- [ ] **TESTS**: A regression test pins this specific fix — one that would go RED if `Perks` stopped being written for the game family the fix targets


## #3284 — FNV-2026-08-24-D1-01: FNV WaterKind vocabulary partially repaired - creek added, spill/potomac/fountain still missing
State: OPEN
Labels: bug low legacy-compat game:fnv water 

## Description
The 2026-08-20 finding (FNV-2026-08-20-D1-02, never filed as a GitHub issue) censused all 78 vanilla FNV `WATR` EditorIDs and found zero matches against the then-current token list, meaning `WaterFlow` was unreachable on the reference title even for records FNV itself names as moving water (`CreekWater01`, `CreekWater02nv`, etc). Since then `canal` and `creek` were added via the `LC-D5-02` shared-token-list hoist — this closes the primary evidenced case, every `Creek*` FNV water record now classifies `River`. `spill` was not added. Re-running the same EditorID roster: `ToxicSpillPuddle` and the `Potomac`/`PotomacNRShallow`/`DLC03TBPotomacWater` family still classify `Calm` — notably `Potomac` (`00030009`) is still the `WRLD` `NAM2` default water for `WastelandNV` and nine other worldspaces, so FNV's default exterior water is still `Calm`.

## Location
`byroredux/src/material_translate.rs:189-204` (`water_kind_from_name`, the shared token classifier reached by both the CELL/WATR path and the mesh-name path)

## Evidence
```rust
fn water_kind_from_name(name: &str) -> WaterKind {
    let lowered = name.to_ascii_lowercase();
    if lowered.contains("rapid") {
        WaterKind::Rapids
    } else if lowered.contains("waterfall") || lowered.contains("falls") {
        WaterKind::Waterfall
    } else if lowered.contains("river")
        || lowered.contains("stream")
        || lowered.contains("canal")
        || lowered.contains("creek")
    {
        WaterKind::River
    } else {
        WaterKind::Calm
    }
}
```
No `spill`, `potomac`, or `fountain` tokens present. Confirmed live at HEAD.

## Impact
Reduced from the prior HIGH-adjacent framing (the `Creek*` family is now fixed) to a narrow residual gap. `ToxicSpillPuddle` is plausibly correctly `Calm` (a puddle, not a current); `Potomac` as WastelandNV's worldspace-default water is the one item worth another look, since it backs every exterior cell that doesn't author its own `XCWT` override.

## Related
`LC-D5-02` (the token-list unification this fix already completed).

## Suggested Fix
Add `potomac` (and optionally `spill`) to `water_kind_from_name`'s token list if a design decision confirms FNV's Potomac River water should carry current; otherwise document the `Calm`-by-default classification as intentional rather than leaving it as an open question with no test pinning either way.

## Completeness Checks
- [ ] **TESTS**: A regression test asserting `Potomac`'s classification, whichever way the design decision lands

_Source: AUDIT_FNV_2026-08-24.md, finding FNV-2026-08-24-D1-01 (partial fix of the never-filed FNV-2026-08-20-D1-02)._

## #3320 — FNV-2026-08-26-D1-02: interior water-plane textures are resolved *after* the cell load's only `flush_pending_uploads`, so all 21 FNV interior water cells render their normal map as the magenta checkerboard
State: OPEN
Labels: bug renderer medium legacy-compat game:fnv water 

**Severity**: MEDIUM
**Dimension**: 1 — Cell Loading End-to-End
**Status**: NEW
**Source**: `docs/audits/AUDIT_FNV_2026-08-26.md` (audit HEAD `d6e16c90`)


**File**: `byroredux/src/cell_loader/load.rs:436-475` vs `byroredux/src/cell_loader/references/complete.rs:370`

**Premise verified**: `resolve_texture` does not upload — it reserves a bindless slot
and *points its descriptor at the fallback checkerboard* until a later batched flush:

```rust
// crates/renderer/src/texture_registry.rs:816-827 (enqueue_dds_for_view)
if view_kind == TextureViewKind::D2 && matches!(outcome, EnqueueOutcome::Reserved(_)) {
    let fallback_idx = self.fallback_handle as usize;
    ...  self.apply_descriptor_write(device, handle, 0, image_view, sampler);
}
```

There are exactly **two** flush call sites in the whole engine
(`grep -rn flush_pending_uploads byroredux/src crates/renderer/src`):
- `byroredux/src/cell_loader/references/mod.rs:727`, reached from
  `references/complete.rs:370` — the tail of `load_references`;
- `byroredux/src/streaming_helpers.rs:339` (`flush_pending_lod_textures`), reached
  only from `streaming_helpers.rs:125`, i.e. the **exterior** streaming reconcile.

There is no per-frame flush in `VulkanContext::draw_frame`.

In `load_cell_with_masters` the interior water plane is spawned *after*
`load_references` has already returned (and already flushed):

```
load.rs:424   let result = load_references(...);          // <- flushes at its tail
load.rs:436   if let Some(water_height) = cell.water_height {
load.rs:445       water::spawn_water_plane(...)           // <- resolve_texture(NNAM) here
```

`spawn_water_plane` (`water.rs:437-441`) calls `resolve_texture` on the WATR normal
path, then binds the returned handle into `material.normal_map_index` (`water.rs:453`)
and onto `NormalMapHandle` (`water.rs:512`). The exterior route is safe by accident —
`ExteriorCellApplyJob::begin` spawns terrain+water first and `advance`'s
`load_references` flushes afterwards.

**Evidence** — real-data scope. Probing `FalloutNV.esm`:

```
interior CELL records: 388
with XCLW:             388     (321 carry the #INT_MIN# no-water sentinel — correctly filtered)
non-sentinel:           47
finite (real) water:    39
   ... of those with XCWT: 21
```

and every FNV `WATR` authors a texture:

```
0x1009ca 'NVCleanWater'      NNAM='Data\Textures\Water\WastelandWaterPotomac.dds'
0x15b8a9 'NVCleanWater02'    NNAM='Data\Textures\Water\TestWaterNoiseGrant.dds'
0x15f8b2 'RadioactiveWater'  NNAM='Data\Textures\Water\WastelandWaterPotomac.dds'
...  (0 of ~60 FNV WATR records have an empty NNAM except 'testWater'/'ReflectingPoolWaterType')
```

Affected cells include `OVCentralSewers01/02`, `OVWestSewers02/03/03b/03c/03d`,
`OVSleepCell02`, `CampGuardianCaves`/`Caves2`, `HooverDamIntIntakeTower01`,
`RatCaveINT`, `SLGoodspringsCaveINT`, `SLBasincreekINT`.

Re-entry does not heal it: cell unload drops the handle to refcount 0
(`unload.rs:410 push_tex_drop` → `drop_textures`) and purges the path map, so the
next load re-reserves a fresh unflushed slot.

**Impact**: On any `--cell <flooded interior>` session — the exact shape of the
Prospector-Saloon bench invocation — the water surface samples the diagnostic
magenta checkerboard as a *tangent-space normal map*. That is not a subtle tint: it
feeds `(1,0,1)`-ish normals into the water pipeline's reflection/refraction ray
setup, so the whole surface reads as broken chrome-ish noise rather than water.
Cross-game: identical for Oblivion/FO3/Skyrim/FO4 interiors, FNV just has the
largest measured surface.

**Fix sketch**: Call `references::flush_pending_cell_textures(ctx)` once more at the
end of `load_cell_with_masters`, after the water spawn (it early-outs at zero
pending, so it is free on the common no-water path). Better: move the interior water
spawn *above* `load_references` so both routes share one flush boundary, and add a
unit assertion that `pending_dds_upload_count() == 0` when `load_cell_with_masters`
returns.

---

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix


## #3321 — FNV-2026-08-26-D1-03: FNV ships a systematic distant-object-LOD family that no code path consumes — the "FO3/FNV ship neither LOD scheme" premise in `exal.md` and `placement_lod.rs` is falsified by the archive
State: OPEN
Labels: bug medium legacy-compat game:fnv terrain-exterior 

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


