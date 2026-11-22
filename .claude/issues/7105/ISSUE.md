# #7105 — ESM-D2-09: `EFIT` gated at 12 bytes on every game; a short Oblivion `EFIT` defaults to a Self-range, no-AV effect

Labels: low,esm-plugin,bug,game:oblivion
Filed from: docs/audits/AUDIT_ESM_2026-11-20.md (finding ESM-D2-09)

Source: `docs/audits/AUDIT_ESM_2026-11-20.md` (`/audit-esm`, HEAD 47ae285c6 / f3713bf5e) · ID **ESM-D2-09**

- **Severity**: LOW (latent; vanilla is 24 B on 100% of 5,553 Oblivion `EFIT`)
- **Dimension**: Sub-Record Byte Accounting
- **Record / Sub-record**: `SPEL` / `ENCH` / `ALCH` / `INGR` / `SGST` `EFIT` (Oblivion)
- **Location**: `crates/plugin/src/esm/records/misc/magic.rs:849` (`b"EFIT" if sub.data.len() >= 12 && self.pending_efid != 0`), `:859-864` (`actor_value: ...get(20..24).map(...).filter(>= 0)`, `range: ...get(16..20).map_or(0, ...)`)
- **Status**: NEW (the `range` / `actor_value` reads came with `9cb2703b3` #7045 and `9feaee8c5`)
- **Description**: xEdit TES4 `EFIT` (`wbDefinitionsTES4.pas:1498`) is a fixed 24 bytes: code @0, magnitude @4, area @8, duration @12, `Type` (Self 0 / Touch 1 / Target 2) @16, actor value s32 @20. The shared arm accepts any body of 12+ bytes (the Skyrim length) and `decode_efit` only needs 16 for Oblivion; a 16-23 byte Oblivion body is accepted and the missing `range` is defaulted to `0` (Self), the missing actor value to `None`. A truncated Touch/Target spell then decodes as a Self-cast. This is the "`*_or_default` mid-fixed-layout" case; the ALCH path (#7045) turns the same value into the item's `delivery`.
- **Evidence**: no vanilla record is short (walker above); `oblivion_effect_chain` rejects an `EFID` with no `EFIT` but not a short `EFIT`.
- **Impact**: none on shipped data; a malformed mod effect gets a plausible wrong delivery.
- **Suggested fix**: per-game `EFIT_LEN` (Oblivion 24, FO3/FNV 20, Skyrim/FO4 12) and decline a shorter body (leave the latch set so the chain is refused whole), as the neighbouring `decode_spit` (`< 16`) already does for `SPIT`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related decoders / other games' arms (per-game layout splits, other callers of the same helper)
- [ ] **TESTS**: A regression test pins this specific fix (synthetic fixture in the default lane; real-data pins stay in the ignored lane)
