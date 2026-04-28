# #764 — NIF-D3-09: read_block_ref_list + 6 reserve sites bypass allocate_vec

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/764
- **Severity**: MEDIUM
- **Labels**: bug, medium, nif-parser, safety
- **Source**: docs/audits/AUDIT_NIF_2026-04-28.md (NIF-D3-09 + NIF-D3-10)

## Summary

`NifStream::read_block_ref_list` and 6 sibling sites use raw `Vec::with_capacity(count as usize)` / `Vec::reserve(count as usize)` on file-driven `u32` lengths, bypassing the `allocate_vec` budget guard (256 MB cap + count ≤ remaining bytes) that every other size-prefixed reader uses. Untrusted-NIF parsing can OOM-panic before any read happens.

## Locations

- Primary: [crates/nif/src/stream.rs:439-446](crates/nif/src/stream.rs#L439-L446)
- Siblings:
  - [crates/nif/src/blocks/shader.rs:443/447/902/906/1502/1506](crates/nif/src/blocks/shader.rs)
  - [crates/nif/src/blocks/interpolator.rs:304](crates/nif/src/blocks/interpolator.rs#L304)
  - [crates/nif/src/blocks/bs_geometry.rs:465](crates/nif/src/blocks/bs_geometry.rs#L465)

## Fix sketch

Route each count through `stream.allocate_vec(count)?`. Mechanical replacement; no signature changes.

## Test plan

- Malformed-NIF fixture with `count = 0xFFFFFFFF` for `NiObjectNETData.extra_data_refs` → assert `Err(InvalidData)` rather than abort.
- Grep sweep for remaining `with_capacity` / `reserve` on file-driven counts.
