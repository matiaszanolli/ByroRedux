# Issue #3512: CsgArchive::chunk_bytes inflates a .csg chunk into an unbounded Vec and range-checks it only afterwards

**Filed**: 2026-08-27 · **Source**: `docs/audits/AUDIT_FO4_2026-08-27.md`

- **Severity**: MEDIUM
- **Dimension**: 1 (M49 precombined geometry — CSG reader) ∩ safety
- **Location**: `crates/bsa/src/csg.rs:274-290` (`CsgArchive::chunk_bytes`)
- **Source**: `docs/audits/AUDIT_FO4_2026-08-27.md` — finding `FO4-2026-08-27-D1-02`

## Description

`chunk_bytes` caps the **compressed** read via `checked_chunk_size(entry.compressed_size, …)` (bound `MAX_CHUNK_BYTES = 1 GiB`, `crates/bsa/src/safety.rs:36`) but places no bound on the *decompressed* size — it inflates into a `Vec` that only had `CHUNK_SIZE` reserved and then rejects the result after the fact:

```rust
let mut raw = Vec::with_capacity(CSG_CHUNK_SIZE);
ZlibDecoder::new(&comp[..]).read_to_end(&mut raw)?;
if raw.len() > CSG_CHUNK_SIZE {
    return Err(io::Error::new(
        io::ErrorKind::InvalidData,
        format!("CSG chunk {idx} inflated to {} > {CSG_CHUNK_SIZE}", raw.len()),
    ));
}
```

`read_to_end` grows the buffer until the deflate stream ends, so the guard can only fire once the whole thing is already resident. Every other attacker-controlled inflate in `crates/bsa` at least pre-caps the target size (`decompress_chunk`'s `checked_chunk_size_usize` on `unpacked_size`, `ba2.rs:719`), and the module docstring for `safety.rs` names decompressed payload sizes as one of the three classes it exists to bound (`safety.rs:5-9`) — this call site was simply not wired to it.

## Evidence

Deflate's maximum expansion ratio is ~1032:1. A crafted 64 MiB `.csg` chunk — well inside the 1 GiB compressed cap, and small enough to ship inside an ordinary mod archive — inflates to ~66 GiB before `raw.len() > CSG_CHUNK_SIZE` is ever evaluated. The path is reached from `read_psg` (`csg.rs:241`), which `build_precombine_meshes` calls for every precombine object of every FO4 cell (`byroredux/src/cell_loader/precombined.rs:736`), and the `.csg` is opened from the load order by name, so a mod supplying `<Plugin> - Geometry.csg` reaches it with no further gate.

## Impact

OOM-kill / allocator abort during an FO4 cell load on a hostile or merely corrupt `.csg`. `.csg` is FO4-only, and modded FO4 is precisely the case where third-party `<Plugin> - Geometry.csg` blobs appear, so the input is genuinely untrusted. No memory-safety violation (this is `flate2`'s safe API into a `Vec`) — the failure is resource exhaustion, not corruption, which is why this is MEDIUM and not HIGH.

## Related

- `#586` / FO4-DIM2-01 — the BA2-side allocation-safety sweep that produced `safety.rs`
- `#1986` / FO4-D1-01 — the *short* interior-chunk guard immediately below (`csg.rs:292-300`), added for exactly the same "reject rather than return wrong bytes" reason but only in the under-size direction
- The sibling `Ba2Compression::Zlib` arm at `ba2.rs:721-727`, which has the same `read_to_end` shape but does at least pre-validate its declared `unpacked_size`
- `#3410` (SKY-2026-08-27b-D5-01) — the same unbounded-`read_to_end` shape on the BSA extraction path; a shared `take(cap + 1)` helper would close both
- `#3399` (ESM-2026-08-27-D1-02) — the ESM compressed-record sibling of the same allocation-ceiling class

## Suggested Fix

Bound the reader instead of the result:

```rust
ZlibDecoder::new(&comp[..]).take(CSG_CHUNK_SIZE as u64 + 1).read_to_end(&mut raw)?;
```

Keeps the existing `> CSG_CHUNK_SIZE` check as the error path (the `+1` makes an over-long stream still trip it) and caps the allocation at 64 KiB + 1. One line, no behaviour change on any well-formed archive.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — the BSA `read_to_end` (`#3410`) and the ESM compressed-record path (`#3399`) share the shape
- [ ] **TESTS**: A regression test pins this specific fix (a zip-bomb `.csg` fixture that must error without allocating)
