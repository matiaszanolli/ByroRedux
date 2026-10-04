# #5232 — FO4-D5-01: a present-but-unopenable BA2 makes every FO4 real-data NIF gate skip green (#4660's fix missed this harness)

https://github.com/matiaszanolli/ByroRedux/issues/5232

Source: `docs/audits/AUDIT_FO4_2026-10-03.md` (HEAD `32f4450d9`)

**Source**: `docs/audits/AUDIT_FO4_2026-10-03.md` (FO4-2026-10-03-D5-01)

- **Severity**: LOW (test harness; no runtime impact)
- **Dimension**: Archives + real data
- **Location**:
  - `crates/nif/tests/common/mod.rs:428-442` (`open_all_mesh_archives`)
  - `crates/nif/tests/common/mod.rs:487-506` (`open_ba2_by_name`)
  - `crates/nif/tests/parse_real_nifs.rs:316-364` (`run_all_meshes_gate`)
- **Status**: NEW. This is a sibling site that #4660's fix (ec63d2636) did not reach. #4660 named the bgsm, facegen, bsa, hkx and menuxml harnesses; `crates/nif/tests/common` was not among them.
- **Description**:
  - Both helpers treat "file exists, but `open` returns `Err`" exactly like "file missing": they print `skipping: failed to open …` and return `None`.
  - `run_all_meshes_gate` then `continue`s and never asserts `walked > 0`. `run_game` returns early.
  - The strict `BYROREDUX_REQUIRE_GAME_DATA` lane (#3850) only hardens the missing-data-dir case.
- **Impact**: a BA2 reader regression that rejects vanilla FO4 mesh archives would turn `parse_rate_fo4_all_meshes`, `parse_rate_fallout_4` and the baseline harnesses into passing no-ops. That is the class #5008 just widened, by making a former warn a hard `InvalidData`. `crates/bsa/tests/ba2_real.rs` `expect`s `Fallout4 - Meshes.ba2` and `Textures1.ba2` to open. Nothing covers `MeshesExtra.ba2` or the six DLC `Main.ba2`.
- **Related**: #4660 (parent sweep), #5008, #4628 (missing-file variant on FO76), #3850, #2334.
- **Suggested Fix**: panic on an `open` error when `is_file()` is true; keep `None` only for absent files. Assert `walked > 0` in `run_all_meshes_gate` whenever the data dir resolved.

## Completeness Checks
- [ ] **SIBLING**: `open_all_mesh_archives` and `open_ba2_by_name` panic on `open` errors when the file exists; `run_all_meshes_gate` asserts `walked > 0` whenever the data dir resolved
