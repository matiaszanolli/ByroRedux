# CHAR-2026-10-26-L-03: The VATS tuning family, the AP restore-rate unit and the `radiation_gauge` row are in no `charal-*` capture

- **Severity**: LOW
- **Dimension**: Coverage & Doctrine
- **Game**: fo3, fnv
- **Location**: `docs/engine/charal-fnv-fo3-ruleset.md` (no VATS, `fVATS*` or `RestoreRate` section; the radiation section at `:388-397` says "mechanism-only"); `docs/engine/charal.md`; code `vats.rs`, `regen.rs::ActionPointsRegen`, `profile.rs:238-244`
- **Status**: NEW (partly adjacent: #6376 asks only for a feature-matrix row, #6403 covers only the placeholder test)
- **Source**: the numbers are sourced elsewhere and verified (GECK *VATS Settings*, *Actor Value Codes* 54, *GetRadiationLevel*, both masters). Missing are the capture document and **any source for the unit claim** that FNV's `fActionPointsRestoreRate` 0.06 is "per frame on a 60 Hz clock" (4 per second on FO3). The only wiki mention of the setting is a `SetGameSetting` example.
- **Description**: The captures are the declared authority for every constant, and the repo cannot adjudicate these rows without one. `ap_regen_per_second` turns 0.06 into 3.6 AP/s. If the engine's unit were different, FNV AP regen would be off by a large factor and no test would notice. Also uncaptured: the reconstructed to-hit chain (disclosed in `vats.rs`), the 255 "always" part sentinel, and the radiation-gauge denominator ("highest sickness stage").
- **Suggested Fix**: Add a "VATS tuning, AP regen, radiation gauge" section to `charal-fnv-fo3-ruleset.md`. Include the probe table with the FNV divergences from L-02 and the GECK citations. Mark the per-frame unit explicitly "unverified" unless a source is found. Cross-reference #6403, and add one pointer line in `charal.md` §8.

---
Source: `docs/audits/AUDIT_CHARACTER_2026-10-26.md` (HEAD bf304851f), finding `CHAR-2026-10-26-L-03`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / profiles / crates
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)

