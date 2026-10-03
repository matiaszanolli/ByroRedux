# #5175: EXT-D3-2026-10-02-03: `placeable_fold` hashes records k and k+64 into the same bit, and the frame path allocates a Vec again

**Labels**: low,terrain-exterior,performance,bug
**Source**: docs/audits/AUDIT_EXTERIOR_2026-10-02.md

- **Severity**: LOW
- **Dimension**: Ground-cover pipeline
- **Location**: `byroredux/src/render/groundcover.rs:871-888`
- **Status**: NEW. The allocation half partly undoes #4922.
- **Tier Violated**: n/a
- **Game Affected**: any load order with more than 64 placeable GRAS records (vanilla Oblivion alone has 108)
- **Description**:
  - **(a) Aliasing.** The key fold is `h = rotl(h, 1) ^ p` over up to 128 bools in a `u64`.
    - Record k contributes bit `(n−1−k) mod 64`, so records k and k+64 alias.
    - If a frame's change in placeability flips both records of an aliased pair (both load, or one loads as the other fails), the XORs cancel. The key stays equal and the table is not re-derived.
    - The newly placeable records then keep zero weight and place nothing until some later, unrelated change.
    - With exactly 128 records, the all-false and all-true folds are equal.
  - **(b) Allocation.** `vec![false; count_ahead]` is allocated every frame before the key check. #4922 had just removed per-frame Vecs from this path.
- **Evidence**: Reasoning from the code above. The fold arithmetic is in the description.
- **Impact**:
  - (a): A record can silently draw nothing in Oblivion-scale load orders. It depends on streaming timing, so it is hard to reproduce.
  - (b): One small heap allocation per frame.
- **Suggested Fix**:
  - Key on the exact bitset: `u128`, since `GROUNDCOVER_MODEL_MAX_RECORDS` is 128. A const-assert ties the two together.
  - Build it without allocating, or into persistent scratch.

**Source report**: `docs/audits/AUDIT_EXTERIOR_2026-10-02.md` (HEAD `c9f95283a`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other games sharing the decoder / other consumers of the same field / other copies of the stale text)
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the EXAL/WATAL `translate_*` (or `spatial_units::normalize`) boundary — never pushed into shaders/renderer, never re-derived at render time
- [ ] **TESTS**: A regression test pins this specific fix
