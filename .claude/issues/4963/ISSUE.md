# #4963: REN-D5-2026-09-27-04: The streaming texture-prefetch store (256 MiB host RAM cap) has no memory-budget.md row

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4963
- **Labels**: low,renderer,memory,documentation,doc-rot

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-27.md` (full renderer audit, all 12 dimensions, audited `main` @ `7e9da5dcc`; code at the finding's location unchanged at `fe80f4d76`). Finding ID: **REN-D5-2026-09-27-04**._

- **Severity**: LOW
- **Dimension**: Memory/Lifecycle
- **Location**: `byroredux/src/asset_provider/texture_prefetch.rs` (`STAGED_BYTE_CAP`, `PrefetchStore::reserve` / `finish` / `clear`); `docs/engine/memory-budget.md` (no entry; `grep -i prefetch` returns nothing)
- **Status**: NEW (the Dim 5 "First step" requires a ledger row for every new resource owner; this one landed in `a3632909a`)
- **Description**: The store keeps up to 256 MiB of extracted DDS bytes (measured peak 111 MiB) until the cell apply completes, is cancelled, or is dropped. The cap is checked against *ready* bytes only. Reads still in flight (`Slot::Running`) each hold a full `Vec<u8>` on a stream-pool thread until `finish`, and a read that overshoots is fully extracted and then dropped. The transient can therefore exceed the cap by up to (stream-pool threads × largest texture).
  - memory-budget.md does ledger CPU-side owners (EsmIndex, CDB, `SwfPlayer::pixel_buffer`).
  - Only `docs/engine/archives.md` §"Texture prefetch" mentions this store.
- **Impact**: A 256 MiB-class host allocation in the streaming path that the RAM budget does not show.
- **Suggested Fix**: Add a CPU-side row to memory-budget.md: cap, clear points, the in-flight overshoot bound, and the `tex_prefetch_*` telemetry that reports it.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders, other producers/consumers, other games)
- [ ] **TESTS**: A regression test pins this specific fix (and fails when the guarded code is deleted — mutation-check it)
