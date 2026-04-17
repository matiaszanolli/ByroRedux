# FO4-D5-H3: 73 unchecked Vec::with_capacity(u32_from_file) sites across 12 files — sweep all

**Issue**: #408 — https://github.com/matiaszanolli/ByroRedux/issues/408
**Labels**: bug, nif-parser, high, safety

---

## Finding

Sister issue to #388 (Oblivion OOM). The Oblivion audit flagged `interpolator.rs:432`; the FO4 audit swept the entire `crates/nif/src/` tree and found **73 unchecked `Vec::with_capacity(count_from_stream)` sites across 12 files**.

Distribution:

| File | Sites |
|---|---|
| `blocks/collision.rs` | 17 |
| `blocks/skin.rs` | 13 |
| `blocks/legacy_particle.rs` | 7 |
| `blocks/interpolator.rs` | 6 (includes the original #388) |
| `blocks/tri_shape.rs` | 3 |
| `blocks/kfm.rs`, `blocks/extra_data.rs`, `blocks/texture.rs`, `blocks/controller.rs`, `blocks/palette.rs`, `blocks/shader.rs`, `anim.rs` | remainder |

Pattern:

```rust
let num = stream.read_u32_le()?;             // arbitrary u32 from file
let mut items = Vec::with_capacity(num as usize);  // no check_alloc
for _ in 0..num { ... }
```

## Why check_alloc doesn't help

`NifStream::read_bytes` enforces `MAX_SINGLE_ALLOC_BYTES = 256 MB` via `check_alloc` and cross-checks remaining stream bytes. All 73 sites **bypass both guards** because they allocate the `Vec` *before* consuming any bytes — `Vec::with_capacity(u32::MAX)` asks for 4 billion × `size_of::<T>()` bytes long before `read_exact` can fail.

## Evidence

- FO4 vendor content doesn't trip it (226,009 NIFs × 0 OOMs in the Dim 5 sweep).
- Oblivion does (see #388): `memory allocation of 135,822,034,912 bytes failed` on `clutter\upperclass\upperclassdisplaycaseblue01.nif`. The file supplied `num_text_keys ≈ 0xFD12EEFF`.
- Any malformed mod NIF or fuzzer input can request a 128 GB allocation.

## Fix

Introduce a helper on `NifStream`:

```rust
impl NifStream {
    pub fn allocate_vec<T>(&mut self, count: u32) -> io::Result<Vec<T>> {
        let bytes = (count as usize)
            .checked_mul(std::mem::size_of::<T>())
            .ok_or(io::Error::new(io::ErrorKind::InvalidData, "count overflow"))?;
        self.check_alloc(bytes)?;
        Ok(Vec::with_capacity(count as usize))
    }
}
```

Then mechanically replace all 73 sites:

```rust
// before
let mut items = Vec::with_capacity(num as usize);
// after
let mut items: Vec<T> = stream.allocate_vec(num)?;
```

## Dependency ordering with #388

Either fix this as a single sweep issue (close #388 in the same PR), or land #388's targeted `interpolator.rs:432` fix first and follow with this sweep. Single sweep is more efficient — one helper, one review, 73 mechanical call-site changes.

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Closes/subsumes #388. Verify no sibling `Vec::with_capacity(something_else as usize)` patterns in `crates/bsa/`, `crates/plugin/` that have the same shape.
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: For each file touched, add one fuzzer-style test feeding `u32::MAX` for the count and asserting `Err`, not panic. Add a `#[cfg(debug_assertions)]` lint to catch `Vec::with_capacity(N as usize)` where N is stream-derived.

## Source

Audit: `docs/audits/AUDIT_FO4_2026-04-17.md`, Dim 5 H-3. Extends Oblivion audit #388.
