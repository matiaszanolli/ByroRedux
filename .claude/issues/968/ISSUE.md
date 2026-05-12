# OBL-D3-NEW-04: parse_clas reads FNV's 35-byte DATA; Oblivion CLAS DATA is 60 bytes with different semantics

**Labels**: bug, medium, legacy-compat

**Audit**: `docs/audits/AUDIT_OBLIVION_2026-05-11_DIM3.md`
**Severity**: MEDIUM
**Domain**: ESM / TES4 actor records

## Premise

`parse_clas` gates on `sub.data.len() >= 35` and reads 4 × u32 tag skills at offset 0..16, then 7 × u8 attribute weights at offset 28.

[crates/plugin/src/esm/records/actor.rs:587-601](../../crates/plugin/src/esm/records/actor.rs#L587-L601)

Doc comment explicitly labels this as the FNV layout.

## Gap

Oblivion CLAS DATA is 60 bytes with a different layout and semantics:

- 2 × u32 *primary attribute pair* (not 7 weights — Oblivion picks 2 primary attributes per class)
- u32 specialization (combat / magic / stealth)
- 14 × u32 major skills (skill-AVIF indices — different semantic from FNV \"tag skills\")
- u32 flags
- u32 services
- i8 trainer skill
- u8 trainer level
- 2 bytes padding

CLAS body in TES4 also carries `ICON` (class portrait, GUI-only).

## Impact

Every Oblivion CLAS is parsed but `attribute_weights` reads 7 bytes from offset 28 (which is mid-skill-list bytes for Oblivion), producing meaningless values. The first 4 `tag_skills` happen to alias the first 4 \"major skills\" by coincidence of the byte layout, but the semantic is wrong.

Doesn't block rendering. Will block any \"is the player using their primary attribute pair\" gameplay check.

## Suggested Fix

Thread `game: GameKind` into `parse_clas`. Branch on Oblivion to read the 60-byte layout. Extend `ClassRecord` with:

```rust
pub primary_attributes: Option<(u32, u32)>,  // Oblivion only
pub specialization: Option<u32>,             // 0=combat, 1=magic, 2=stealth
pub major_skills: Vec<u32>,                  // Oblivion: 14 skill-AVIF indices
```

FNV / Skyrim path stays untouched on the existing 35-byte gate.

## Completeness Checks

- [ ] **SIBLING**: Verify FNV CLAS tests still pass (existing `parse_rate_fnv_esm` should pin tag_skills count).
- [ ] **TESTS**: Regression test parses `Oblivion.esm` and asserts vanilla \"Knight\" class has `primary_attributes = Some((Strength, Personality))`, `specialization = Some(0 /* combat */)`, `major_skills.len() == 7`.
- [ ] **DOCS**: Note the Oblivion vs FNV CLAS layout split in the `ClassRecord` doc comment.
