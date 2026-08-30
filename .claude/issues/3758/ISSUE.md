# #3758 — SAFE-2026-08-30-D2-01: `CsgArchive::read_psg` pre-allocates from a caller length that three unvalidated `u32`s off an FO4 `_oc.nif` control — ~200 GB worst case, `handle_alloc_error` abort

**Labels**: bug, import-pipeline, high, safety, game:fo4

---

- **Severity**: HIGH
- **Dimension**: 2 — Memory corruption / UB (untrusted-input allocation)
- **Location**: `crates/bsa/src/csg.rs` — `CsgArchive::read_psg` (the allocation) ← `byroredux/src/cell_loader/precombined.rs` (the length)
- **Source**: `docs/audits/AUDIT_SAFETY_2026-08-30.md` (`SAFE-D2-01`), HEAD `64f64480`

## Description

`CsgArchive::read_psg` opens with `let mut out = Vec::with_capacity(len);` **before** any
of its careful per-chunk EOF checks run, and `len` is a plain `usize` parameter — it is
**the one size in the CSG reader that never passes through `crates/bsa/src/safety.rs`**.
Its single production caller computes it from three raw `u32` fields read straight out of
an FO4 `_oc.nif` with no ceiling anywhere on the path:

```rust
// byroredux/src/cell_loader/precombined.rs
let tri_start = (lod_off_idx / 3) as usize;
let need = geom.num_verts * stride + (tri_start + lod_count) * 6;
...
let psg = match csg.read_psg(geom.data_offset as u64, need) {
```

`PrecombineGeomRef.num_verts` / `.lod_counts` / `.lod_offsets`
(`crates/nif/src/import/precombine.rs`) are copied verbatim from
`BsPackedSharedGeomData`'s `num_verts: u32` / `tri_count_lod*: u32` / `tri_offset_lod*: u32`
(`crates/nif/src/blocks/extra_data.rs`), which `collect_precombine_geom_refs` forwards
without validation. Unlike the *non*-shared `BsPackedGeomData` variant — whose payload is
in-file and so is bounded by `NifStream::check_alloc` — the shared variant's arrays live
in the external `.csg`, so **nothing in the NIF parser ever has to reconcile these counts
against a byte budget**. With `tri_count_lod0` alone at `u32::MAX`, `need` reaches ~26 GB;
with `num_verts` at `u32::MAX` and a 48-byte stride it reaches **~200 GB**.

## Evidence

```rust
// crates/bsa/src/csg.rs
pub fn read_psg(&self, offset: u64, len: usize) -> io::Result<Vec<u8>> {
    let mut out = Vec::with_capacity(len);   // ← unbounded, runs first
    let mut remaining = len;
    let mut pos = offset;
    while remaining > 0 {
        let idx = (pos / CSG_CHUNK_SIZE as u64) as u32;
        ...
        if idx as usize >= self.chunks.len() { return Err(...UnexpectedEof...) }
```

Every other file-controlled size in this same file **is** bounded —
`checked_entry_count(num_chunks_raw, "CSG chunk")`, `checked_chunk_size(entry.compressed_size, …)`,
and `inflate_bounded`. `read_psg`'s `len` is the sole exception, and it is the one value
that originates outside this crate. (Re-verified at HEAD: `crates/bsa/src/safety.rs`
exports `checked_entry_count`, `checked_chunk_size`, `checked_chunk_size_usize`,
`inflate_bounded`, `checked_chunk_total` — `read_psg` calls none of them.)

## Impact

A corrupt or hostile FO4 `_oc.nif` — **ordinary mod-distribution content, and the
precombine path runs on every FO4 cell load** — drives a multi-gigabyte
`Vec::with_capacity` before a single bounds check executes. On allocation failure Rust
calls `handle_alloc_error`, which **aborts the process**: not an `Err` any caller can
handle, and not interceptable by `catch_unwind`. **The two `continue`-on-`Err` arms in
`precombined.rs` that make every other failure in this loop a skipped object are
unreachable for this one.**

On Linux with heuristic overcommit a mid-range value may instead succeed as a pure virtual
reservation, so the observable symptom ranges from a silent multi-GB VA spike to a hard
abort depending on the declared size and the host's overcommit policy — which is exactly
why the ceiling belongs at the reader, not at the allocator.

This is the same class the project has already closed everywhere else it appears:
**#388** (NIF, CRITICAL), **#408** (73-site NIF sweep, HIGH), **#2614** (Starfield CDB,
HIGH), **#3011** (HKX, HIGH), **#3399** (ESM compressed records, HIGH), **#3410**
(BSA `inflate_bounded`, HIGH). Rated HIGH to match those; the amplification here (three
independent `u32`s, one multiplied by a stride) is larger than in the still-open MEDIUM
#3512.

## Related

- **#3512** (OPEN, same file, `chunk_bytes`) — *different site, different mechanism*
  (up-front capacity vs. decompression bomb), so this does not duplicate it. **Note:
  #3512's own premise now reads stale** — `chunk_bytes` was routed through
  `crate::safety::inflate_bounded` by #3410's sweep; it should be re-checked before it is
  worked.
- **#3410** — the `inflate_bounded` helper whose posture this should reuse.
- **#1533** (CLOSED) — the sibling *index*-bounds check `decode_shared_geom_object` already
  performs on the same fields, in `crates/nif/src/import/precombine.rs`: it validates
  triangle indices against `num_verts` but only **after** the buffer has been allocated and
  read.

## Suggested Fix

Route `need` through `byroredux_bsa::safety::checked_chunk_size_usize` at the call site,
and make `read_psg` defensive on its own account by clamping the initial
`Vec::with_capacity` to `len.min(self.psg_len()? as usize)` (or simply to
`MAX_CHUNK_BYTES`) so the reader cannot be made to pre-allocate more than the PSG space it
actually owns. Use `saturating_mul` / `checked_mul` for the `need` arithmetic while there,
so a wrapped product cannot silently under-read instead.

> Note the label: `import-pipeline` is applied because the repo has **no `bsa` label**.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files — the second `read_psg` call site in `precombined.rs`, and any other reader taking a caller-supplied `len`
- [ ] **TESTS**: A regression test pins this specific fix — a synthetic `_oc.nif` fixture with an oversized `num_verts` must return `Err`, not abort
