# 2012: LC0716-01: PACK schedule (PSDT) parsed with a single fixed byte layout; diverges on Skyrim+/FO4/FO76/Starfield

https://github.com/matiaszanolli/ByroRedux/issues/2012

Labels: medium, legacy-compat, bug

**Severity**: MEDIUM · **Dimension**: Dimension 6 — per-game translation-survey gaps
**Location**: `crates/plugin/src/esm/records/misc/ai.rs:538-550` (`parse_pack`'s `b"PSDT"` arm); `crates/plugin/src/esm/records/mod.rs:603-611` (the `b"PACK"` dispatch arm)
**Status**: NEW
**Audit**: docs/audits/AUDIT_LEGACY-COMPAT_2026-07-16.md (LC0716-01)

## Description
`parse_pack` decodes every game's `PACK.PSDT` sub-record with one fixed offset table (`duration: i32`@4..8), documented as "FO3/FNV PSDT." No `GameKind` reaches `parse_pack` at all, even though the same dispatch function already gates `SCOL`/`PKIN`/`MOVS`/`MSWP` on `is_scol_era`/`is_fo4_plus` for exactly this class of divergence.

Cross-checked against wrye-bash's `brec.MelPackSchedule`/`MelPackScheduleOld`: old (pre-Skyrim) `PSDT` is 8 bytes, `duration`@4 — matches Redux exactly. New (Skyrim+) `PSDT` is 12 bytes with a new `minute` field inserted, so `duration` moves to offset 8. Redux's fixed offset-4 read on Skyrim+ therefore reads `minute` + 3 padding bytes as `duration_hours`, never touching the real duration bytes.

Sibling `PKDT` checked and confirmed NOT affected — `package_ai_type`/`procedure_type` sits at the same offset 4 across eras (wrye-bash `MelPackPkdt`).

## Evidence
```rust
// ai.rs:538-541
b"PSDT" if sub.data.len() >= 8 => {
    // FO3/FNV PSDT: month i8, dayOfWeek i8, date u8, time i8
    // (hour; -1/0xFF = any), duration i32 (hours).
    let time = sub.data[3] as i8;
    let duration = i32::from_le_bytes([
        sub.data[4], sub.data[5], sub.data[6], sub.data[7],
    ]);
```
```rust
// mod.rs:603-611 — no game gate, unlike sibling arms above it
b"PACK" => {
    let pack_remap = reader.get_form_id_remap();
    extract_records(&mut reader, end, b"PACK", &mut |fid, subs| {
        index.packages.insert(fid, parse_pack(fid, subs, &pack_remap));
    })?;
}
```

## Impact
Currently dormant — `index.packages`' only production consumer (`npc_spawn::spawn_npc_entity`) runs only for Oblivion + Fallout3NV. The Skyrim+/FO4/FO76/Starfield spawn path never calls `active_package_is_*`. Forward-looking risk: the moment package selection is wired for those games, `PackSchedule::duration_hours`/`active_at` silently returns garbage — no parse error, just wrong schedule windows.

## Related
`docs/engine/npc-spawn-ai-packages.md` §3-4; the `is_scol_era`/`is_fo4_plus` gating precedent at `crates/plugin/src/esm/records/mod.rs:193-294`; #446 (CLOSED, original PACK bootstrap — this is a follow-on format-fidelity gap, not a regression).

## Suggested Fix
Thread `GameKind` into `parse_pack` and branch the `PSDT` decode on a "post-Skyrim package format" predicate to select the 12-byte layout (`duration`@8), surfacing the new `minute` field if a future consumer wants it.

## Completeness Checks
- [ ] SIBLING: `PKDT` in the same parser checked and confirmed NOT affected; no other `PACK` sub-records checked yet
- [ ] TESTS: A regression test pins this specific fix (Skyrim+ `PSDT` fixture asserting `duration_hours` reads from offset 8)
