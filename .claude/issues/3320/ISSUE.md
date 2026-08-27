# FNV-2026-08-26-D1-02

**Issue**: #3320
**Filed**: 2026-08-26 from `docs/audits/AUDIT_FNV_2026-08-26.md`

---

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
