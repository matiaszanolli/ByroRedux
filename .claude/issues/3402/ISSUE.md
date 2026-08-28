# #3402 — RT-2026-08-27-01: 23 Skyrim skinned body meshes reach MeshRegistry::upload with zero indices and are silently dropped before the GPU

Labels: high, renderer, memory, game:skyrim, bug
Filed: 2026-08-27 by `/audit-publish docs/audits/AUDIT_RUNTIME_2026-08-27.md`
Source report: `docs/audits/AUDIT_RUNTIME_2026-08-27.md`

---

Source: `docs/audits/AUDIT_RUNTIME_2026-08-27.md` — RT-2026-08-27-01 (live headless runs at `969d81c8`).

- **Severity**: HIGH
- **Dimension**: runtime telemetry → renderer mesh upload
- **Game**: `skyrim_se` · **Cell**: `WhiterunDragonsreach`
- **Location**: `crates/renderer/src/mesh.rs` (`MeshRegistry::upload`, ~:491-517); `crates/renderer/src/vulkan/buffer.rs` (`create_index_buffer` ~:790-817, `create_device_local_buffer` ~:1326-1330); `byroredux/src/scene/nif_loader.rs:826-837`

## Description

Twenty-three skinned NPC meshes in `WhiterunDragonsreach` arrive at `MeshRegistry::upload` with a non-empty vertex slice and an **empty index slice**. `create_vertex_buffer` succeeds; `create_index_buffer` computes `size = std::mem::size_of_val(data)` = 0, hands it to `create_device_local_buffer` → `create_staging_buffer(device, allocator, 0, "buffer_staging")`, and `gpu_allocator` rejects the allocation outright (`if size == 0 || !alignment.is_power_of_two() { return Err(InvalidAllocationCreateDesc) }`, `gpu-allocator-0.28.0/src/vulkan/mod.rs:799`). The `?` in `upload` propagates, `nif_loader.rs:836` logs a `warn!` and `continue`s, and the mesh renders nothing.

## Evidence

An instrumented release build (throwaway worktree, one `log::error!` inserted between the two buffer creations in `upload`) on `--game skyrim_se --cell WhiterunDragonsreach`:

```
23 AUDIT-PROBE hits, all of the form vertices=N indices=0:
     6 vertices=992  indices=0        6 vertices=218  indices=0
     5 vertices=417  indices=0        3 vertices=174  indices=0
     1 vertices=676  indices=0        1 vertices=850  indices=0
     1 vertices=872  indices=0
Failed to upload NIF mesh : 23        GpuBuffer dropped : 23
```

The 23 failures are 1:1 with the 23 mesh names logged by the uninstrumented run, and they are exactly the humanoid skin/underwear set:

```
6x 'BODY'   5x 'MaleUnderwear_1'   5x 'FootMale_Big'   3x 'Feet'
1x 'HandMaleBig3rd'  1x 'HandFemale3rd'  1x 'FootFemale_Big'  1x 'FemaleUnderwear'
```

VRAM exhaustion is disproved by the same log: `GPU memory: 1294.3 MB allocated / 1755.5 MB reserved` on a 12 GB device. `mesh_cache_failed_count` is **0** — the NIFs parse cleanly; the geometry that comes out of the decode has vertices and no triangles.

## Impact

Named Whiterun NPCs (Balgruuf, Hrongar, Farengar, Proventus…) lose torso / hands / feet geometry at render time. This directly negates `e0d5ec18` (#3357), whose stated purpose was to make exactly these `NakedTorso` / `NakedHands` / `NakedFeet` addons resolve: they now resolve and then fail to upload. Pre-existing but amplified — a probe at `fa71f1a2` (`e0d5ec18^`) records **9** such failures against **23** at HEAD, so #3357 multiplied the affected mesh count 2.6×.

## Related

Adjacent to, but distinct from, the concurrently-filed "351/351 Skyrim vanilla creature-race NPCs lose their body mesh" — that is a race/ARMA *resolution* failure; this is a *geometry-decode + upload* failure on humanoid actors whose meshes resolve correctly. Amplified by #3357 (`e0d5ec18`). See also RT-2026-08-27-02 for the resource-lifetime defects on the same error path.

## Suggested Fix

Two layers. (1) Find why the SSE skinned decode emits a zero-triangle partition for these shapes — `crates/nif/src/import/mesh/skin.rs` and `sse_recon.rs` are the candidates, and `07ca5979` (#3355/#3360, "SSE SkinPartition triangles are global indices") is the most recent change to that decode, though the failure predates it. (2) Reject the mesh at the import boundary rather than at the allocator, so a zero-triangle shape is never queued for upload.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix
