# #3513: FO3-2026-08-27-D6-01: the ROADMAP FO3 compat-matrix cell still cites the pre-#3041 single-archive corpus (10 989 vs the live 17 172)

**Labels**: low, nif-parser, documentation, doc-rot, game:fo3, legacy-compat
**Audit**: `docs/audits/AUDIT_FO3_2026-08-27.md`

---

Source: `docs/audits/AUDIT_FO3_2026-08-27.md` — finding `FO3-2026-08-27-D6-01` (LOW, Dimension 6 — real-data validation / doc-rot).

Sibling of **#3342** (OPEN), which reports the identical defect on the *FNV* row of the same table. Filed separately because a fix to #3342 edits a different line and would leave this one stale.

## Location
`ROADMAP.md:578`

## Description
The row reads

```
| Fallout 3         | BSA v104      | 100% (10 989)                                | Interior (Megaton, 929 REFRs). Exterior wired; fresh GPU bench pending (R6a). |
```

10 989 is the `Fallout - Meshes.bsa`-only count. #3041 widened `run_game` to walk `Game::mesh_archives()` — all six vanilla FO3 archives — and the gate has been measuring the full corpus since.

## Evidence
`cargo test -p byroredux-nif --release --test parse_real_nifs -- --ignored parse_rate_fallout_3`, executed during the audit run:

```
[Fallout 3/Fallout - Meshes.bsa]  10989 NIFs, 10989 clean, 0 truncated, 0 failed (100.00%)
[Fallout 3/Anchorage - Main.bsa]   1597 NIFs ... (100.00%)
[Fallout 3/BrokenSteel - Main.bsa]  855 NIFs ... (100.00%)
[Fallout 3/PointLookout - Main.bsa]1372 NIFs ... (100.00%)
[Fallout 3/ThePitt - Main.bsa]     1614 NIFs ... (100.00%)
[Fallout 3/Zeta - Main.bsa]         745 NIFs ... (100.00%)
[Fallout 3] parsed 17172/17172 NIFs: clean 100.00%
```

## Impact
The headline understates verified coverage by 36 %, and — the reason it matters — it makes a *regression* harder to spot: a future reader comparing a re-measured 17 172 against the documented 10 989 has no way to tell growth from drift. The DLC archives are also where FO3's rarest collision blocks live (`bhkSPCollisionObject`, `bhkBlendCollisionObject`, `bhkConvexTransformShape` — see `crates/nif/tests/common/mod.rs:110-140`), so the number that certifies them should be visible.

## Related
#3342 (FNV row, OPEN), #3041 (the widening), #3369 (the Skyrim variant of the same blind spot, OPEN).

## Suggested Fix
`100% (17 172 across all 6 vanilla archives; 10 989 in Fallout - Meshes.bsa)`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the other per-game rows of the same ROADMAP compat matrix (see #3342, #3369)
