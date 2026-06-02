# Issues 1398 + 1399

## #1398 MEM-01: NifImportRegistry unlimited by default
**File:** `byroredux/src/cell_loader/nif_import_registry.rs:107`
**Fix:**
- Change `unwrap_or(0)` → `unwrap_or(2048)` so the default caps at 2048 entries
- Add `log::warn!` when `max_entries == 0` (explicit unlimited via BYRO_NIF_CACHE_MAX=0)
- Update struct doc comment to reflect the new default

## #1399 MEM-02: MeshRegistry len() cast to u32 has no overflow guard
**File:** `crates/renderer/src/mesh.rs:284` and `:430`
**Fix:**
- Add `MAX_MESH_SLOTS: u32` module constant (1<<24 = 16M)
- Guard both `self.meshes.len() as u32` casts with a bounds check that bails on overflow
