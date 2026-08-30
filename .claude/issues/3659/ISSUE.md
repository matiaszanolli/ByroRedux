# CONC-D7-2026-08-30-01: `Ba2Archive::extract` holds its `Mutex<File>` across zlib/LZ4 inflate; the BSA sibling drops it first

**Issue**: #3659
**Labels**: bug, import-pipeline, medium, performance, concurrency
**Filed**: 2026-08-30
**Source report**: `docs/audits/AUDIT_CONCURRENCY_2026-08-30.md`

---

Source: `docs/audits/AUDIT_CONCURRENCY_2026-08-30.md` — CONC-D7-2026-08-30-01 (MEDIUM, D7 · Worker Threads & Thread-Safety Bounds).

**Location**: `crates/bsa/src/ba2.rs:403-445` (guard acquisition + both dispatch arms), with the decompression inside `extract_general` (`:841-858`) and `extract_dx10` (`:873-895`). **Sibling that does it right**: `crates/bsa/src/archive/extract.rs:39-132`.

## Description

`BsaArchive::extract` deliberately releases the file guard **before** inflating, with the rationale spelled out in place:

> "Drop the lock before the decompression CPU work — the file handle isn't needed for decompression and other extracts shouldn't have to wait." (`crates/bsa/src/archive/extract.rs:128-132`)

**`Ba2Archive::extract` never adopted that.** It binds `let mut file = ...lock()` at `:403` and passes `&mut *file` straight into `extract_general` / `extract_dx10`, both of which do `seek -> read_exact -> decompress_chunk` with the guard still alive; `extract_dx10` does it **once per mip chunk** in a loop. The guard is only released when `extract` returns — i.e. after the whole texture/mesh has been inflated. The file handle is not needed for any of that work: `decompress_chunk` operates on the already-read `packed: Vec<u8>`.

## Evidence

```
crates/bsa/src/ba2.rs
403        let mut file = match self.file.lock() {
...
414        match entry {
419            } => extract_general(
420                &mut *file,                      // guard still held
...
433            } => extract_dx10(
434                &mut *file,                      // guard still held
...
841 fn extract_general<R: Read + Seek>(
849     reader.seek(SeekFrom::Start(offset))?;
855         let mut packed = vec![0u8; packed_size as usize];
856         reader.read_exact(&mut packed)?;
857         decompress_chunk(&packed, unpacked_size as usize, compression)   // <-- under the lock
...
873 fn extract_dx10<R: Read + Seek>(
881     for chunk in chunks {
882         reader.seek(SeekFrom::Start(chunk.offset))?;
...
890             let buf = decompress_chunk(&packed, chunk.unpacked_size as usize, compression)?;  // <-- per chunk, under the lock
```

Contrast, `crates/bsa/src/archive/extract.rs`:
```
128            // Drop the lock before the decompression CPU work — the file
129            // handle isn't needed for decompression and other extracts
130            // shouldn't have to wait.
132            drop(file);
```

## Trigger Conditions

Any BA2-backed game (**FO4 / FO76 / Starfield**) in exterior streaming. The cell-stream worker is inside Phase 1's serial extract loop (`byroredux/src/streaming.rs:1358-1364`) on a compressed GNRL mesh or a multi-chunk DX10 entry; concurrently the main thread calls `extract_mesh` on the *same* `Arc<TextureProvider>` — the LOD-band reconcile (`byroredux/src/cell_loader/terrain_lod_btr.rs:218`, `object_lod.rs:272`), the sync REFR loader (`cell_loader/references/synth_child.rs:519`), or resumable NPC spawn (`npc_spawn/resumable.rs:644,684,849,981,1068,1158,1195,1243`).

Both go through the one `Vec<Archive>` in `TextureProvider.mesh_archives` (`asset_provider/texture.rs:7-10, 57-65`) and therefore the one `Mutex<File>` per archive.

## Prior-art note (why this is NEW, not a duplicate)

Observed in passing by `docs/audits/AUDIT_CONCURRENCY_2026-05-13.md:206` inside an `Existing: #877` entry — *"BA2 path is worse than BSA-compressed (which drops the lock before zlib/LZ4)"*. **#877 is CLOSED**, and its fix was the two-phase serial-extract/parallel-parse split *inside* the worker; it never touched this lock span and does not cover the main-thread<->worker case. No open issue covers it.

## Verification Path

`cargo test` cannot see it (no timing assertion exists). Confirming signal is a **timing measurement**, not a validation layer: run `--game fo4 --grid <x>,<y> --radius 3 --bench-frames 300 --bench-hold` and compare `StreamingTelemetry`'s apply-slice percentiles / `CpuFrameTimings` against a build with the guard released before `decompress_chunk`. The static half is already verifiable by reading the two files side by side.

## Impact

**Priority inversion — the main thread waits behind a background worker.** The main-thread callers above run inside the per-frame `STREAMING_APPLY_BUDGET` / LOD reconcile budget, so the stall lands directly in frame time as a hitch whose length is a whole macro-mesh or DX10 mip chain's inflate — precisely the class of work Starfield's `.bto`/`.btr` and DX10 texture entries make large.

Not a correctness bug: the `Mutex` still serialises every `seek`+`read_exact` pair correctly and nothing is torn. **FNV / FO3 / Oblivion / Skyrim(-SE) are unaffected** (BSA path already correct); **FO4 / FO76 / Starfield are.**

## Related

#877 (closed — the intra-worker two-phase split that sidesteps *rayon* contention but not main<->worker contention); `docs/audits/AUDIT_CONCURRENCY_2026-05-13.md:204-206`; #1170 (poison recovery, which touched the same two functions and kept them asymmetric); `byroredux/src/streaming.rs:1330-1345` (the comment block that reasons about this mutex and correctly describes the BSA behaviour, but does not distinguish the BA2 case).

## Suggested Fix

Restructure `extract_general` / `extract_dx10` so the **read** half takes the `&mut File` and returns the packed bytes, and the **decompress** half runs after `Ba2Archive::extract` drops the guard — mirroring `archive/extract.rs:132` exactly, so the two backends stop diverging. For `extract_dx10` that means reading every chunk's packed bytes under one guard hold, then inflating and concatenating outside it.

## Completeness Checks
- [ ] **SIBLING**: The BSA and BA2 backends end up with the *same* lock discipline — divergence between them is the root cause here
- [ ] **LOCK_ORDER**: Poison recovery (#1170) still behaves correctly across the split guard scope
- [ ] **TESTS**: A source-shape or timing pin so the guard cannot silently re-widen; `byroredux/src/streaming.rs:1330-1345`'s comment block updated to describe the BA2 case
