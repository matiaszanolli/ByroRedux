# SPT-2026-09-11-D3-01: import.rs still asserts the two corpus-premise errors #3740/#3751 fixed everywhere else

URL: https://github.com/matiaszanolli/ByroRedux/issues/4118
Labels: documentation, low, game:oblivion, terrain-exterior, speedtree, doc-rot

- **Severity**: LOW
- **Dimension**: TREE→Billboard Wiring (audit-speedtree)
- **Location**: `byroredux/src/cell_loader/references/import.rs:509-529`
- **Status**: NEW (residual of closed #3740, #3751 — the fix commit `57fdcc57` swept `crates/plugin/src/esm/records/tree.rs` and `crates/spt/src/import/mod.rs` but not this file)

**Description**

`57fdcc57` ("Fix doc rot: SpeedTree billboard wiring, CNAM/BNAM corpus claims") corrected two false corpus premises — "CNAM is 5 floats on Oblivion, 8 on FO3/FNV" and "BNAM is FO3/FNV-only, absent on Oblivion" — in `crates/plugin/src/esm/records/tree.rs` and `crates/spt/src/import/mod.rs`. `byroredux/src/cell_loader/references/import.rs`, which builds the same `SptImportParams` from the same `TreeRecord` fields one call site further out, states both false premises verbatim and was not touched by that commit:
- Line 509-510: *"CNAM's positional semantics remain unpinned across the 5-float Oblivion and 8-float Fallout layouts."* — CNAM is 8 floats on all three games (142/142 Oblivion, 9/9 FO3, 3/3 FNV measured, `#3751`); there is no split.
- Line 517-521: *"Oblivion ships MODB on 100% of TREE records and OBND on none, so the placeholder size fallback needs MODB to size Cyrodiil trees correctly"* — the OBND/MODB percentages are correct, but the conclusion is exactly the disproved `#3740`/D4-01 premise: Oblivion also ships BNAM on 100% of `.spt`-bearing TREE records, BNAM outranks MODB in `compute_billboard_size`'s precedence, so BNAM — not MODB — actually sizes every vanilla Oblivion tree.
- Line 524-529: *"#1002 — BNAM (FO3/FNV billboard width × height) as a fallback BELOW OBND"* — restates BNAM as FO3/FNV-only, the same false premise a second time in the same comment block.

**Evidence**

```
crates/spt/src/import/mod.rs:78:  ...8 × f32 on all three games — Oblivion, FO3, FNV, no split)   [FIXED]
crates/plugin/src/esm/records/tree.rs:194: ...8 × f32 on all three games... no split.               [FIXED]
byroredux/src/cell_loader/references/import.rs:509-510: ...5-float Oblivion and 8-float Fallout...  [STALE]
```

**Impact**

No functional consequence — the actual code at this call site (lines 522, 530) correctly reads `t.bound_radius` and `t.billboard_size` and passes both through `SptImportParams` unconditionally, so the precedence bug D4-01 already fixed in behaviour is not reintroduced. The cost is purely that a future editor reading this specific call site (the one that actually assembles `SptImportParams` from a live `TreeRecord`, arguably the most load-bearing comment block in the wiring for understanding *why* the fields are threaded the way they are) will reason from both disproved premises, exactly the failure mode `#3740` and `#3751` were filed to close.

**Related**

#3740, #3751 (both closed, both partially — not regressed in behaviour, only in documentation completeness at this one additional site); D4-01 and D3-03 in `docs/audits/AUDIT_SPEEDTREE_2026-08-30.md`.

**Suggested Fix**

Replace lines 509-510 with the measured 8-floats-on-all-three-games fact (mirroring the wording now in `import/mod.rs`), and replace 517-529 with a corrected statement that BNAM, not MODB, is the Oblivion-reachable tier (mirroring D4-01's fix in `import/mod.rs`'s `bound_radius` field doc). No behaviour change — this is a comment-only fix.

## Completeness Checks
- [ ] **SIBLING**: Grep the whole repo once more after the fix to confirm no fourth occurrence of either premise survives

Source: `docs/audits/AUDIT_SPEEDTREE_2026-09-11.md`
