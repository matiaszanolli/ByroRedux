# NIF-D6-2026-09-21-01: The #4157 amplification class survives on other fixed-size elements; worst is BSSkin::BoneData (68x), and #4156's new CInfo2014 path copied the loose pattern

**Issue**: #4623
**Filed**: 2026-09-22 (audit-publish, AUDIT_NIF_2026-09-21.md)

**Severity**: MEDIUM, the same class and rating as #4157.
**Dimension**: Allocation Hygiene
**Game Affected**: FO4 / FO76 / Starfield (`BSSkin::BoneData`, CInfo2014); every era for the other BlockRef lists.
**Location**:
- `crates/nif/src/blocks/skin.rs:497`: `allocate_vec(num_bones)` for a 68-byte `BsSkinBoneTrans`, *before* `read_f32_array(num_bones * 17)`.
- `crates/nif/src/blocks/collision/rigid_body.rs:375` (new this window), and its siblings `:181` and `:266`.
- Other loose fixed-size sites: `skin.rs:45`, `:119`, `:448`; `collision/shape_compound.rs:54`; `controller/sequence.rs:20`, `:45`, `:456`; `particle.rs:1164`; `collision/compressed_mesh.rs:117`, `:126`, `:143`, `:160`.

## Description
`allocate_vec` (`crates/nif/src/stream.rs:278`) bounds `count` at 1 byte per element and never goes through `check_alloc`, so the 256 MB `MAX_SINGLE_ALLOC_BYTES` cap does not apply. The function's own doc on the sibling `allocate_vec_sized` says to prefer that variant for scalar and array structs.

`BsSkinBoneData` reserves 68 bytes per claimed bone (`size_of::<BsSkinBoneTrans>()`: 4 bsphere + 9 rotation + 3 translation + 1 scale = 17 f32s = 68 bytes) before the capped bulk read can reject a forged count. Simply reordering those two calls, or switching to `allocate_vec_sized`, would close the gap.

The constraint-ref lists amplify only 4× (`BlockRef` is 4 bytes vs. the 1-byte floor), but the new CInfo2014 path (`rigid_body.rs:375`) diverges from its own siblings: `collision/shape_compound.rs:171`, `collision/shape_mesh.rs:44` and `controller/mod.rs:490/594/633` already use `allocate_vec_sized::<BlockRef>`, while all three `rigid_body.rs` constraint-ref sites (`:181`, `:266`, `:375`) still use the unsized `allocate_vec`.

Confirmed at HEAD `ee6d3fb39`: all cited sites still call `allocate_vec` (not `allocate_vec_sized`), verified by grep against the current tree.

## Impact
A crafted NIF can request up to 68 × its own size in capacity. Under Linux overcommit those pages stay untouched; it becomes an abort only when the request exceeds RAM + swap. So this is a hygiene regression rather than a live exploit.

## Related
#2523, #4157, #408, #3918 (all closed — established the `allocate_vec`/`allocate_vec_sized` distinction and the amplification class).

## Suggested Fix
- Use `allocate_vec_sized::<BsSkinBoneTrans>`, or size the Vec from the flat array after it is read, in `BsSkinBoneData::parse`.
- Use `allocate_vec_sized::<BlockRef>` for the rigid-body constraint-ref lists (`:181`, `:266`, `:375`) and the other loose fixed-size sites listed above.

Source: docs/audits/AUDIT_NIF_2026-09-21.md (NIF-D6-2026-09-21-01)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (every `allocate_vec` call site for a fixed-size element type across the crate)
- [ ] **TESTS**: A regression test pins this specific fix (forged-count rejection for `BsSkinBoneData` and the rigid-body constraint lists)
