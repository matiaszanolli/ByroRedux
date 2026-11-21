# #7121 — ESM-D8-04: FO76 `CELL.XCRI` uses `u64` counts and 8/16 B entries; the FO4-shaped arm rejects all 1,182 (warn and drop)

Labels: low,esm-plugin,bug,game:fo76
Filed from: docs/audits/AUDIT_ESM_2026-11-20.md (finding ESM-D8-04)

Source: `docs/audits/AUDIT_ESM_2026-11-20.md` (`/audit-esm`, HEAD 47ae285c6 / f3713bf5e) · ID **ESM-D8-04**

- **Severity**: LOW
- **Dimension**: Real-Data Validation
- **Record / Sub-record**: `CELL` `XCRI` on Fallout 76
- **Location**: `crates/plugin/src/esm/cell/helpers.rs:155-198` (the `XCRI` arm is not gated by game)
- **Status**: NEW
- **Description**: xEdit `wbDefinitionsFO76.pas:8069-8085` defines `XCRI` as `u64 meshes count`, `u64 references count`, then 8-byte `Mesh` entries (hash + 4 unknown) and 16-byte `Reference` entries (ref, 4 unknown, combined mesh, 4 unknown). The decoder implements the FO4 layout (`wbDefinitionsFO4.pas:6088`: `u32` counts, 4-byte mesh hashes, 8-byte ref pairs).
- **Evidence**: 1,182 FO76 `XCRI` size-mismatch warns, `hdr=1+0 expected=12 actual=40`, `hdr=2+0 … actual=64`, `hdr=3+0 … actual=88`: the "second count" is the high dword of the first `u64`, and 40 = 16 + 8×1 + 16×1, 64 = 16 + 8×2 + 16×2, 88 = 16 + 8×3 + 16×3. The arm warns and skips, so the data is dropped, not mis-read.
- **Impact**: FO76 precombined-mesh dedup (`precombined_mesh_hashes`, `absorbed_ref_bakes`) is inactive for 1,182 cells, and the log carries 1,182 warns per load. There is no FO76 render path that consumes it. FO4 and Starfield do not hit this warn (Starfield has no `XCRI`).
- **Suggested fix**: add a FO76 arm with the `u64` header and wider entries (or log at `debug` and document FO76 as unsupported); keep the FO4 arm for `Fallout4`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related decoders / other games' arms (per-game layout splits, other callers of the same helper)
- [ ] **TESTS**: A regression test pins this specific fix (synthetic fixture in the default lane; real-data pins stay in the ignored lane)
