# NIF-PERF-03: allocate_vec misused as bound-check at 9 sites — double-allocates per skin partition

## Description

Nine call sites use the pattern:

```rust
stream.allocate_vec::<u16>(num_bones as u32)?;        // result discarded!
let bones = stream.read_u16_array(num_bones as usize)?;
```

The first call is intended only for its bound-check side effect — but `allocate_vec` is implemented as `Ok(Vec::with_capacity(count as usize))` (`stream.rs:203`). For nonzero counts this **reserves heap capacity, then immediately drops the empty Vec on the next semicolon**. The downstream `read_u16_array` then does its own `check_alloc` (so the bound-check is even redundant) PLUS its own allocation.

Net effect: every nonzero call site allocates twice and drops once, when only one allocation is needed.

This appears to be a misreading of the `allocate_vec` helper purpose — its docstring says it's for replacing `Vec::with_capacity(count as usize)` in the *bind* position. Here the binding is missing, so the helper degrades to a no-op-with-allocation. Partial regression of #408 (the blanket `allocate_vec` sweep).

`check_alloc` (also exposed `pub` on `NifStream`) is the right tool when you want bound validation without allocation.

## Locations (all 9 sites)

- `crates/nif/src/blocks/skin.rs:266, 273, 279, 288, 299, 318`
- `crates/nif/src/blocks/tri_shape.rs:1441, 1455`
- `crates/nif/src/blocks/controller/morph.rs:222`

## Evidence (skin.rs:266-281, partition loop)

```rust
// #388: `num_bones` is a file-driven u16; bound through allocate_vec.
stream.allocate_vec::<u16>(num_bones as u32)?;
let bones = stream.read_u16_array(num_bones as usize)?;

// Vertex map (conditional on has_vertex_map for v >= 10.1.0.0).
let vertex_map = if has_conditionals {
    let has = stream.read_byte_bool()?;
    if has {
        stream.allocate_vec::<u16>(num_vertices as u32)?;
        stream.read_u16_array(num_vertices as usize)?
    } else { Vec::new() }
} else {
    stream.allocate_vec::<u16>(num_vertices as u32)?;
    stream.read_u16_array(num_vertices as usize)?
};
```

## Impact

NiSkinPartition is on every NPC body / creature mesh. Skyrim SE NPCs have 6-12 partitions per NiSkinPartition, each running 6 redundant allocations on 100-500 bones / vertices / triangles.

That's 36-72 throwaway `Vec::with_capacity` allocations per NPC mesh × ~50 NPCs in a typical Whiterun load = **~2000-3500 redundant heap allocations per cell**. ~0.2-0.5 ms per cell load on the parser, plus heap fragmentation that compounds across the load.

NiTriStripsData (tri_shape.rs:1441/1455) extends the same pattern to legacy Morrowind / Oblivion content.

## Suggested Fix

Replace each bare `stream.allocate_vec::<T>(n)?;` with `stream.check_alloc(n as usize * std::mem::size_of::<T>())?` (or just delete it — `read_*_array` already calls `check_alloc` internally before allocating). The bound is enforced at the same point with no temporary allocation.

Add `#[must_use]` to `allocate_vec`'s return, which would have caught this at the original change.

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Grep `allocate_vec` across all of `crates/nif/src/blocks/` to catch any 10th site missed by the audit
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Add `#[must_use]` to `allocate_vec` declaration; the compiler will fail-fast on regression. Existing parse tests cover correctness.

## Related

- #408 (allocate_vec blanket sweep — this is a partial regression)

## Source Audit

`docs/audits/AUDIT_PERFORMANCE_2026-05-04_DIM5.md` — NIF-PERF-03