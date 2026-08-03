# NIF-D6-01: parse_particle_system's modifier_refs bypasses allocate_vec, duplicating bound-check logic

URL: https://github.com/matiaszanolli/ByroRedux/issues/2282
Labels: bug, nif-parser, low, tech-debt, nif

**Severity**: LOW
**Dimension**: Allocation Hygiene
**Game Affected**: all (any NIF with a `NiParticleSystem`/`BSStripParticleSystem` block)
**Location**: `crates/nif/src/blocks/particle.rs:1130-1139`

## Description

Every other file-driven `Vec<T>` allocation in the crate routes through `NifStream::allocate_vec` (bound-check + `with_capacity` in one `#[must_use]`-pinned call, per #831/#408). This one site instead hand-rolls the same pattern: `stream.check_alloc(...)?` followed by `reserve_exact(...)` and a manual push loop — precisely the pre-`#831` idiom the rest of the codebase (including all three `ragdoll.rs` sites) migrated away from. Appears to have been missed when the `allocate_vec` migration swept through `particle.rs`.

## Evidence

```rust
// crates/nif/src/blocks/particle.rs:1130-1139
let num_modifiers = stream.read_u32_le()?;
stream.check_alloc((num_modifiers as usize).saturating_mul(4))?;
modifier_refs.reserve_exact(num_modifiers as usize);
for _ in 0..num_modifiers {
    modifier_refs.push(stream.read_block_ref()?);
}
```

(The other `check_alloc`-only sites at `particle.rs:682`, `:1075`, `:1393` are read-and-discard skip loops with no backing `Vec` — not instances of this pattern.)

## Impact

None measurable — `check_alloc` + `reserve_exact` is allocation-equivalent to `allocate_vec`. Pure duplication/consistency issue, flagged because it's the one remaining template a future contributor could copy to reintroduce the old idiom elsewhere.

## Suggested Fix

Replace with `stream.allocate_vec::<BlockRef>(num_modifiers)?` followed by the same push loop, matching `read_block_ref_list` and every other bulk-ref site in the crate.

## Completeness Checks
- [ ] **SIBLING**: Confirm no other hand-rolled `check_alloc` + `reserve_exact` + push-loop sites remain elsewhere in `crates/nif/src/blocks/`
- [ ] **TESTS**: A regression test (or existing coverage) exercises `modifier_refs` allocation via the rewritten `allocate_vec` path with an oversized/junk count to confirm the bound-check still rejects it

