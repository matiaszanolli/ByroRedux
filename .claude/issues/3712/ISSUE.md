# #3712: NIF-2026-08-30-D3-01: Oblivion's eight DLC archives (1,580 NIFs, 16.4%) are guarded by no parse gate — on the one game where a dispatch regression truncates instead of recovering

**Labels**: bug, nif-parser, medium, nif, game:oblivion, test-gap
**Filed**: 2026-08-30 (audit-publish)

---

**Report**: `docs/audits/AUDIT_NIF_2026-08-30.md` · **Severity**: MEDIUM · **Dimension**: Block Dispatch Coverage
**Game affected**: Oblivion (`bsver <= 11`, NIF v20.0.0.4/5 and the v10.x NetImmerse family — the `no_block_sizes` band)

## Location
- `crates/nif/tests/common/mod.rs` — `mesh_archives` (Oblivion arm), `optional_mesh_archives` (returns `&[]` for Oblivion)
- `crates/nif/tests/parse_real_nifs.rs` — `parse_rate_oblivion`
- `crates/nif/tests/block_coverage_baselines.rs` — `oblivion_block_count_parity` (opens the primary archive only)

## Description
The #3041 → #3466 → #3369 sequence widened every game's parse gate from "the primary mesh archive" to "every mesh-bearing archive", and added an `optional_mesh_archives` tier so account-varying content could be gated by rate even when it cannot be baselined by count. **Oblivion was left out of both.** 1,580 NIFs across eight vanilla DLC archives are covered by no test in the repository — not `parse_rate_oblivion`, not `per_block_baselines`, not `oblivion_block_count_parity`.

Verified current: `Game::optional_mesh_archives` has a `Game::SkyrimSE` arm listing 5 archives and `_ => &[]` for everything else.

## Evidence
Full-corpus sweep of every `.bsa` in Oblivion's `Data/`:

```
Oblivion - Meshes.bsa            8032 nifs   8032 clean   0 trunc   0 fail  <- gated
DLCShiveringIsles - Meshes.bsa   1438        1438         0         0       <- ungated
Knights.bsa                        75          75         0         0       <- ungated
DLCBattlehornCastle.bsa            24          24         0         0       <- ungated
DLCFrostcrag.bsa                   17          17         0         0       <- ungated
DLCOrrery.bsa                       9           9         0         0       <- ungated
DLCVileLair.bsa                     8           8         0         0       <- ungated
DLCThievesDen.bsa                   5           5         0         0       <- ungated
DLCHorseArmor.bsa                   4           4         0         0       <- ungated
```

1,580 of 9,612 Oblivion NIFs (16.4%) ungated. `Game::mesh_archives`' doc-comment explains the omission — requiring GOTY-only DLC would make the all-or-nothing rule skip Oblivion entirely on a base-game install — but that is exactly the problem `optional_mesh_archives` was introduced to solve for Skyrim SE's Creation Club archives under #3369, and it was not applied here. (`Skyrim - Animations.bsa`, 44 NIFs, is likewise in neither list.)

## Impact
Oblivion is the **only** supported game with no `block_sizes` table, so it is the only game where an undispatched or under-reading block truncates the remainder of the scene instead of being absorbed into an `NiUnknown` placeholder. `oblivion_block_count_parity` exists specifically to catch that cascade and is blind to 16.4% of the content. Shivering Isles alone is 1,438 files of distinctly authored Daedric/Mania architecture and creatures — later-authored content of exactly the kind #3041 widened the FNV gate to cover. All 1,580 parse clean today, so this is an unguarded-corpus gap, not a live defect.

## Related
#3041, #3466, #3369 (the same blind spot closed for every other game), #1332 (`oblivion_block_count_parity`). Sibling: the D1-01 sizeless drift-detector finding filed alongside this one.

## Suggested Fix
Populate `Game::optional_mesh_archives` for Oblivion with the eight DLC archives (and `Skyrim - Animations.bsa` for Skyrim SE) — the present-only tier is safe for `parse_rate_oblivion` because that gate asserts a *rate*. Then widen `oblivion_block_count_parity` to `open_all_mesh_archives` + `open_optional_mesh_archives` and regenerate its truncating-file baseline, which is count-based and needs its own regen pass.

## Completeness Checks
- [ ] **SIBLING**: `Skyrim - Animations.bsa` (44 NIFs) is in neither list either — check both games in the same pass
- [ ] **TESTS**: A regression test pins this specific fix — the widened gate must actually open the DLC archives when present
