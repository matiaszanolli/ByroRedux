# OBL-D5-C1: OOM abort on NiTextKeyExtraData — unchecked Vec::with_capacity

**Issue**: #388 — https://github.com/matiaszanolli/ByroRedux/issues/388
**Labels**: bug, nif-parser, critical, safety

---

## Finding

`NiTextKeyExtraData::parse` at `crates/nif/src/blocks/interpolator.rs:432` calls `Vec::with_capacity(num_text_keys as usize)` where `num_text_keys` is a raw `u32` read from the file. When the upstream stream position drifts, this u32 can be any value up to `0xFFFFFFFF` and the `Vec::with_capacity` immediately allocates slots for it, bypassing `MAX_SINGLE_ALLOC_BYTES` at `crates/nif/src/stream.rs:184` (which only gates `check_alloc`).

## Evidence

Running `nif_stats --release` on `Oblivion - Meshes.bsa` aborts with:

```
memory allocation of 135,822,034,912 bytes failed
stack backtrace:
  10: byroredux_nif::blocks::interpolator::NiTextKeyExtraData::parse
  11: byroredux_nif::blocks::parse_block
  12: byroredux_nif::parse_nif_with_options
```

Reproducer: `meshes\clutter\upperclass\upperclassdisplaycaseblue01.nif` (218,209 bytes, NIF v20.0.0.4).

The 135.8 GB figure = 4,244,438,591 × 32 bytes ≈ u32::MAX × size_of tuple layout.

## Scope — same pattern elsewhere

`rg -n 'Vec::with_capacity\([a-z_]+ as usize\)' crates/nif/src/` finds the same class of bug in ~40 sites:
- `blocks/collision.rs`, `blocks/skin.rs`, `blocks/palette.rs`, `blocks/texture.rs`, `blocks/controller.rs`
- `anim.rs`, `kfm.rs`, `header.rs`, `import/mesh.rs`

## Impact

- **Critical**: any Oblivion content sweep aborts the engine process.
- **Critical**: `parse_rate_oblivion` integration test at `crates/nif/tests/parse_real_nifs.rs` crashes before any `assert!` reports.
- **CVE-adjacent**: a crafted NIF in a mod's BSA DoSes anyone who loads that cell.

## Fix

Add `stream.check_alloc(num_text_keys.checked_mul(SIZE).ok_or(...)?)` before `with_capacity`. Better: introduce a helper on `NifStream`:

```rust
pub fn allocate_vec<T>(&mut self, count: u32) -> io::Result<Vec<T>> {
    let bytes = (count as usize).checked_mul(std::mem::size_of::<T>())
        .ok_or(io::Error::new(io::ErrorKind::InvalidData, "count overflow"))?;
    self.check_alloc(bytes)?;
    Ok(Vec::with_capacity(count as usize))
}
```

Then sweep the ~40 call sites. This also fixes H-3 symptoms (garbage KeyType / NiStringPalette reads) since the bogus-u32-allocation class is the same shape.

## Completeness Checks
- [ ] **UNSAFE**: N/A (no unsafe)
- [ ] **SIBLING**: Same pattern audited in `blocks/{collision,skin,palette,texture,controller}.rs`, `anim.rs`, `kfm.rs`, `header.rs`, `import/mesh.rs`
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Regression test parsing `upperclassdisplaycaseblue01.nif` returns `Err` not abort; `parse_rate_oblivion` completes without panic.

## Source

Audit: `docs/audits/AUDIT_OBLIVION_2026-04-17.md`, Dim 5 C-1.
