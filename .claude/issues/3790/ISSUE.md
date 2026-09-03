# #3790 — FNV-2026-08-30-D8-02: --game fnv omits Update.bsa, so 21 base-game FalloutNV.esm MODL paths (incl. the NCR guard towers) resolve to no mesh

**Severity**: LOW · **Location**: `assets/debug_profiles.toml` (`[profiles.fnv]`)
**Source**: `docs/audits/AUDIT_FNV_2026-08-30.md` (FNV-2026-08-30-D8-02)

`Update.bsa` (FNV's own base-game patch archive) loads at higher priority than
`Fallout - Meshes.bsa` in retail, isn't a `<stem>N.bsa` sibling of anything, and wasn't listed
in `default_bsas`. 21 of 11,994 base `FalloutNV.esm` MODL paths (incl. the NCR guard towers)
resolve only in `Update.bsa` and were dropping to no mesh.

## Fix implemented

Added `Update.bsa` to `[profiles.fnv].default_bsas` — **corrected order** relative to the
issue's own literal suggested text. The issue's suggested fix wrote
`["Fallout - Meshes.bsa", "Update.bsa"]`, but the actual archive-resolution code
(`TextureProvider::extract_mesh`/`extract` in `byroredux/src/asset_provider/texture.rs`) walks
its archive list in push order and returns the **first** hit — so listing the patch archive
*second* would make it lose priority to the base archive it's meant to override, the opposite of
retail's own priority and the opposite of what the issue's own prose asked for ("ordered so the
patch archive wins"). Implemented as `default_bsas = ["Update.bsa", "Fallout - Meshes.bsa"]`
instead, verified against the actual resolution code before writing it.

**SIBLING** (issue's own checklist item): checked the mounted Oblivion and Fallout 3 Data
directories for an equivalent unlisted base-game patch archive — neither ships one (no
`update.bsa`/`patch.bsa`-named file in either). No equivalent fix needed for
`[profiles.fo3]`/`[profiles.oblivion]`.

**TESTS** (issue's own checklist item): added
`fnv_profile_lists_update_bsa_before_the_base_meshes_archive` (`crates/game-detect/src/lib.rs`),
reading the real shipped `assets/debug_profiles.toml` and asserting `Update.bsa` precedes
`Fallout - Meshes.bsa` — pinning the priority ORDER, not just presence. Verified live: reverting
the order back to `["Fallout - Meshes.bsa", "Update.bsa"]` makes the test fail with the correct
diagnostic; restored and re-confirmed passing.

Full workspace: `cargo test --no-fail-fast` 7036 passing, 0 failing.
