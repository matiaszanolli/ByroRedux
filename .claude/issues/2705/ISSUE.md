# #2705: SF-2026-08-12-D3-01 - The 105 MB `materialsbeta.cdb` is fully decompressed and immediately discarded on every `build_material_provider` call

- **Severity**: MEDIUM
- **Dimension**: 3 — CDB material database
- **Location**: `byroredux/src/asset_provider/material.rs:44-52` (`discover_starfield_cdbs`), `:352-384` (`register_starfield_cdb`)
- **Status**: NEW
- **Description**: `discover_starfield_cdbs` calls `archive.extract(&path)` for every
  discovered CDB, which for a BA2 GNRL entry runs the full zlib inflate into an owned
  `Vec<u8>`. `register_starfield_cdb` then reads exactly the 4-byte magic and the
  12-byte header (`probe_header`), bumps a counter, and the `Vec` is dropped at the end
  of the loop iteration. Nothing retains the bytes. Phase 1 needs 16 bytes; it pays
  105 MB of inflate + allocation for them, per CDB, per provider build.
- **Evidence**: Measured — `materials\materialsbeta.cdb` extracts to 105,037,616 bytes
  from the 17.6 MB `Starfield - Materials.ba2`. `MaterialProvider` has no field holding
  CDB bytes (only `sf_cdb_count: usize`, `material.rs:277`).
  `build_material_provider` runs fresh at boot (`scene.rs:355,395`,
  `byroredux/src/scene/nif_loader.rs:78`), at every door/cell transition (`app_step.rs:514,575`),
  at save-load (`save_io.rs:913`), and at debug-load (`debug_load.rs:125,283,370`) —
  the same call-site set #2615 was filed against, so the CDB extract is now the
  dominant remaining cost of that rebuild.
- **Impact**: ~105 MB transient allocation + a multi-hundred-ms inflate on every cell
  transition on Starfield, for a presence bit. Same class as the (fixed) SF-D3-03
  full-archive sniff, one layer in.
- **Related**: #2615 (SF-D3-03, fixed sibling), #2039 / PERF-D7-02 (provider-rebuild
  caching note in `app_step.rs:445-460`), #2359 (Phase 2 — which *will* need the bytes,
  so the fix should be a cache, not a narrower read).
- **Suggested Fix**: Either (a) add a bounded `Vec<u8>`/`Arc<[u8]>` hold keyed by
  archive+path so the Phase-2 parse and cross-cell rebuilds reuse it — the shape the
  `csg_cache` next to it already uses — or (b) short-circuit discovery when the same
  (archive path, CDB path) pair was already registered this session.

---
**Source**: `docs/audits/AUDIT_STARFIELD_2026-08-12.md` (finding `SF-D3-01`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs`, per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

