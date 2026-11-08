# ESM-D4-02: FO4 / FO76 `MUSC` and `MUST` are gated off on a premise xEdit and the data contradict

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/6790
Labels: low, esm-plugin, bug, game:fo4, game:fo76
Source: docs/audits/AUDIT_ESM_2026-11-08.md

_Filed from `docs/audits/AUDIT_ESM_2026-11-08.md` (audit HEAD 3500d2737)._

**Related**: Adjacent to open #3816 (scopes Skyrim + FNV only). Also covers the duplicate ESM-D8-01 from the same audit.

- **Severity**: LOW (coverage; no consumer yet for non-Skyrim music)
- **Dimension**: 4 · **Record**: `MUSC`, `MUST`
- **Location**: `dispatch_misc_stub.rs:102-105` (comment: "FO4 / Starfield author `MUST` with another layout, which is not decoded"; arm `if game == GameKind::Skyrim`), `misc/music.rs` `parse_musc_remapped` (`_ => {}`); the same claim is in the audit-esm skill text ("`MUST` is Skyrim-gated because FO4/Starfield lay it out differently")
- **Status**: NEW (related to open #3816, which scopes only Oblivion `RDMD` and FNV `MSET`)
- **Description**: xEdit `FO4.pas:12602` and `FO76.pas:13685` define `MUST` identically to Skyrim's (`CNAM` kind enum with the same three hashes, `FLTV`, `DNAM`, `ANAM`, `BNAM`, `LNAM`, `FNAM` cue points, `CITC` / `CTDA`, `SNAM`). `FO4.pas:12499` `MUSC` is identical to Skyrim's too (`FNAM` flags, `PNAM`, `WNAM`, `TNAM`). Only Starfield genuinely differs.
- **Evidence**:
  - `Fallout4.esm` `MUST` carries exactly Skyrim's sub-record set (`ANAM` 506, `BNAM`, `CIS2`, `CITC`, `CNAM`, `CTDA`, `DNAM`, `FLTV`, `FNAM`, `LNAM`, `SNAM`, `EDID`) on 552 records; `SeventySix.esm` is the same on 1,825.
  - FO4 `MUSC` is `EDID` + `FNAM` + `PNAM` + `TNAM` + `WNAM` on 111 of 111 records. FO76 `MUSC` adds `UNAM` / `VNAM` (231 of 246).
  - Starfield `MUST` really is different (`MSTF` 169, `MTSH` 168), so that arm is correct.
- **Impact**: 552 FO4 and 1,825 FO76 music tracks are walked and discarded, and 111 / 246 `MUSC` are EDID-only. Any future FO4 music (cell `XCMO` / location music) has to re-open this.
- **Suggested fix**: widen the `parse_musc_remapped` Skyrim arm to FO4 / FO76 and the `MUST` gate to `Skyrim | Fallout4 | Fallout76`, with real-data counts (FO4 111 / 552) as the pin. Correct the comment and the skill text either way.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related decoders / other games' arms
- [ ] **LOCK_ORDER**: n/a unless a RwLock scope changes
- [ ] **TESTS**: A regression test pins this specific fix (real-data pin where an install exists)
