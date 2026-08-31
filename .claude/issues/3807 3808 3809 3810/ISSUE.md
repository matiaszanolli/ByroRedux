# #3807, #3808, #3809, #3810 — EX-14/15 split follow-ups (from #2369)

All four are epic-scale research spikes / multi-phase features split off
`#2369` per `docs/engine/exterior-readiness-plan.md`. None closes in a
single session — user chose to run a **deep spike on #3809 and #3810**
this session (both are pure corpus byte-analysis, no engine/renderer
surface, real FO4 data available on disk); #3807 and #3808 were left
untouched.

## #3807 — EX-14/15 item A: ground-cover density streaming (GRAS/REGN)
**Status: untouched this session.** Phase 0 (canonical types, LTEX map,
palette) already landed 2026-08-12. Everything past that — the §11.1
blocking terrain-sampling benchmark, scatter compute shader, LOD chain,
RT proxy, real GRAS decode — is unstarted. First required step is a
`--bench-hold` benchmark against real terrain, per the design doc's own
gate.

## #3808 — EX-14/15 item B: full SpeedTree geometry rendering
**Status: untouched this session.** Billboard-only fallback confirmed
still in place (`crates/spt/src/import/mod.rs`). Phase 2.1 (geometry-tail
byte-layout dissection against real `.spt` samples) is the required first
step; genuinely unstarted research-spike work.

## #3809 — EX-14/15 item C4: FO4 precombine collision (Havok BhkSystemBinary)
**Status: deep spike completed, container cracked; payload still blocked.**
See `INVESTIGATION.md` for the full byte-level derivation. Summary:
- The raw `bhkPhysicsSystem`/`bhkRagdollSystem` blob is a classic Havok
  packfile (`57 E0 E0 57 10 C0 C0 10` magic), `file_version=11`,
  `contents_version="hk_2014.1.0-r1"` — byte-identical across 30+ real
  `_physics.nif` samples.
- New decoder: `crates/nif/src/blocks/collision/havok_packfile.rs`
  (`parse_havok_packfile`) — decodes the header + 3-entry section table
  (`__classnames__` / `__types__` / `__data__`) + the class-name list.
  5 unit tests against a hand-built synthetic fixture (no real game
  bytes shipped in the repo).
- Confirms FO4 physics uses the `hknp` (Havok Next-gen Physics) class
  family (`hknpPhysicsSystemData`, `hknpCompressedMeshShape`,
  `hknpCompressedMeshShapeData`, `hknpBSMaterialProperties`), not the
  older `hkp` rigid-body pipeline.
- **Real remaining blocker, now precisely scoped**: `__types__` is
  empty in every sampled blob — no embedded reflection metadata, so the
  `__data__` section's `hknpCompressedMeshShapeData` payload isn't
  self-describing. Decoding it needs Havok's own proprietary bit-packed
  mesh-shape encoding (confirmed high-entropy binary from byte offset
  ~0x1c0 onward) — this is still genuinely greenfield format work, not
  smaller than originally scoped, but the surface area is now much
  better bounded (container fully understood; only the mesh-shape codec
  itself is unknown).
- No leaked Havok SDK source was consulted (user declined that path) —
  every field was derived from real-corpus byte-offset arithmetic and
  cross-checked for internal self-consistency.

## #3810 — EX-14/15 item C3: FO4 previs/occlusion (.uvd) format decode
**Status: deep spike completed, envelope extended; payload still blocked.**
See `INVESTIGATION.md` for the full byte-level derivation. Summary:
- New decoder: `crates/bsa/src/uvd.rs` (`parse_uvd_header`) — extends the
  2026-08-23 partial crack (magic / self-size / tile-size / debug string)
  with two new confirmed fields:
  - `table_offset` (byte `0x30`): **byte-identical (`336`) across all 30
    sampled files regardless of size** (3.4 KB to 2.4 MB) — a fixed
    header length / first-variable-table start pointer.
  - `entry_count` (byte `0x38`): scales with file complexity (1 to 305
    across the corpus) — very likely an object/visibility-entry count.
  - `bounds` (bytes `0x14..0x28`, 5 `f32`s): scale-consistent with FO4
    exterior world-space coordinates; likely an axis-aligned bounding
    volume, axis order not confirmed.
- **Real remaining blocker, now precisely scoped**: the payload from
  `table_offset` (`0x150`) onward is itself high-entropy, evidently
  bit-packed binary — comparable difficulty to the Havok mesh-shape
  problem in #3809, not a simple flat struct. Confirming `bounds`' exact
  semantics and decoding the bitstream both need further work (the
  former via cross-referencing real parsed CELL bounds from the ESM,
  the latter via bit-level corpus analysis) — left for follow-up.

## Scratch tooling (kept, per this repo's existing `_tmp_*` convention)
- `crates/nif/examples/_tmp_a0831_havok_blob.rs` — samples `_physics.nif`
  across a BA2, dumps `BhkSystemBinary` blobs, exercises
  `parse_havok_packfile` against real data.
- `crates/bsa/examples/_tmp_a0831_uvd_header.rs` — samples `.uvd` files
  across a BA2, dumps header bytes, exercises `parse_uvd_header` against
  real data.
