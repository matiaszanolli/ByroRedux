title:	OBL-D3-NEW-02: Oblivion-unique base records BSGN/CLOT/APPA/SGST/SLGM silently skipped
state:	OPEN
author:	matiaszanolli (Matias Zanolli)
labels:	bug, legacy-compat, medium
comments:	0
assignees:	
projects:	
milestone:	
number:	966
--
**Audit**: `docs/audits/AUDIT_OBLIVION_2026-05-11_DIM3.md`
**Severity**: MEDIUM
**Domain**: ESM / TES4 record dispatch

## Premise

The records dispatch in [crates/plugin/src/esm/records/mod.rs:675-1262](../../crates/plugin/src/esm/records/mod.rs#L675-L1262) has no arm for `BSGN`, `CLOT`, `APPA`, `SGST`, or `SLGM`. They fall to the catch-all:

```rust
_ => {
    reader.skip_group(&group);
}
```

at `mod.rs:1259-1261`.

## Gap

- **BSGN** (birthsigns — 13 vanilla, references SPEL list for "Atronach absorb spell", etc.)
- **CLOT** (clothing — distinct fourCC in TES4; folded into ARMO from FO3 onward)
- **APPA** (alchemical apparatus — 4 vanilla records)
- **SGST** (sigil stones — Oblivion gates content)
- **SLGM** (soul gems — referenced by `ENCH` charge cross-ref)

## Impact

No content-layer impact on rendering (none carry placement REFRs to interior or exterior cells). Impact is on gameplay subsystems and inventory display once those land:

- Player starting under "The Mage" birthsign gets no auto-cast bonus.
- Gear-tier display for clothing items reports `unknown record`.
- ENCH cross-refs to SLGM dangle (soul-gem charge model breaks).

## Suggested Fix

Add 5 `extract_records` dispatch arms following the `MinimalEsmRecord` long-tail pattern from #810. For SLGM, add a single `soul_capacity: u8` field decoded from DATA byte 0:

```rust
b\"BSGN\" => extract_records(&mut reader, end, b\"BSGN\", &mut |fid, subs| {
    index.birthsigns.insert(fid, parse_minimal_esm_record(fid, subs));
})?,
// CLOT / APPA / SGST same shape
b\"SLGM\" => extract_records(&mut reader, end, b\"SLGM\", &mut |fid, subs| {
    index.soul_gems.insert(fid, parse_slgm(fid, subs));
})?,
```

## Completeness Checks

- [ ] **SIBLING**: Verify SLGM `soul_capacity` semantics match across FO3 (mesmetron orb / behemoth body) — confirm before reusing the byte.
- [ ] **TESTS**: Regression test parses `Oblivion.esm` and asserts `index.birthsigns.len() >= 13`, `index.clothing.len() >= 100`, etc.
- [ ] **DOCS**: Update CLAUDE.md ESM dispatch count (currently \"18 categories\", will be 23+).
