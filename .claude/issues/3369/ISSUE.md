# SKY-2026-08-27-D6-03: the Skyrim SE parse-rate gate omits 5 vanilla-shipped BSAs (715 NIFs), the same blind spot #3041 closed for FNV

Labels: low,nif-parser,test-gap,bug,game:skyrim,legacy-compat

- **Severity**: LOW
- **Confidence**: CONFIRMED (read the code + ran the sweep on the omitted archives)
- **Location**: `crates/nif/tests/common/mod.rs:184`
- **Description**: `Game::mesh_archives()` returns
  `Game::SkyrimSE => &["Skyrim - Meshes0.bsa", "Skyrim - Meshes1.bsa"]`. A stock Steam
  Skyrim SE (Anniversary) `Data/` also ships `_ResourcePack.bsa` and four Creation Club
  archives that carry NIFs, none of which the gate opens. This is structurally the same
  hole #3041 closed for FNV (*"the gate that certifies FNV NIF parse rate 100 % clean
  was measuring a fraction of the content it claimed"*) — the fix widened the FNV list
  but Skyrim's was left at two entries.
- **Evidence**: sweeping the omitted archives with the same parse path the gate uses:
  ```
  _ResourcePack.bsa              total=149 clean=149 truncated=0 recovered=0 failed=0   (BSTreeNode=16)
  ccBGSSSE001-Fish.bsa           total=231 clean=231 truncated=0 recovered=0 failed=0
  ccBGSSSE025-AdvDSGS.bsa        total=266 clean=266 truncated=0 recovered=0 failed=0
  ccBGSSSE037-Curios.bsa         total= 65 clean= 65 truncated=0 recovered=0 failed=0
  ccQDRSSE001-SurvivalMode.bsa   total=  4 clean=  4 truncated=0 recovered=0 failed=0
  ```
  715 NIFs, all clean today — so there is no live defect hiding behind the gap, only an
  unguarded surface (including 16 `BSTreeNode` SpeedTree roots that exist nowhere in the
  gated set at that density).
- **Impact**: No current user-visible impact. A parser regression that touched only
  Creation Club / Anniversary content would not turn the Skyrim gate red, and the
  ROADMAP compat-matrix "Skyrim SE 100 % clean" figure describes 32,709 of 33,424
  shipped NIFs.
- **Suggested Fix**: Extend the `Game::SkyrimSE` arm of `mesh_archives()` to
  `["Skyrim - Meshes0.bsa", "Skyrim - Meshes1.bsa", "_ResourcePack.bsa",
  "ccBGSSSE001-Fish.bsa", "ccBGSSSE025-AdvDSGS.bsa", "ccBGSSSE037-Curios.bsa",
  "ccQDRSSE001-SurvivalMode.bsa"]`. `open_all_mesh_archives` already skips absent
  archives, so non-AE installs are unaffected.
- **Related**: #3041 (the FNV instance of this same gap).

---

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
---

*Filed from `docs/audits/AUDIT_SKYRIM_2026-08-27.md` (`/audit-skyrim`, 7 dimensions),
verified against HEAD `558af58c` on a full vanilla Skyrim SE install.*
