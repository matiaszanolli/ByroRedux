# #6283: CHAR-2026-10-21-L-01: The #5928 fix wrote a false sentence into the FO4 barter capture: Fallout4.esm does author the price caps, and they are 0.8 / 1.2

**Labels**: low,character,documentation,doc-rot,game:fo4

**Source**: docs/audits/AUDIT_CHARACTER_2026-10-21.md · **Severity**: LOW · **Dimension**: Coverage & Doctrine · **Location**: `docs/engine/charal-fo4-ruleset.md:241-244` (added by `2c16ef06d`)

**Audit status**: NEW (introduced by the #5928 fix)

- **Game**: fo4
- **Source**: Fallout4.esm `GMST`s read this run: `fBarterSellMax` 0.8, `fBarterBuyMax` 1.2, `fBarterMax` 3.5, `fBarterMin` 2.0. Fallout4.exe defaults: `fBarterSellMax` 0.95, `fBarterBuyMax` 1.05. The executable has no `fBarterBuyMin` string.
- **Description**: The capture now says the executable's "`fBarterSellMax=0.95` and `fBarterBuyMin=1.05`; Fallout4.esm does not author them. The sale may not exceed 95% of item value, and the purchase may not fall below 105%". That is false on three counts:
  - FO4's floor setting is `fBarterBuyMax`, not `fBarterBuyMin`.
  - The master authors both caps.
  - The effective limits are 80 % and 120 %.

  The paragraph also contradicts the capture's own formula block three lines above ("floored at 1.2 / ceiled at 0.8"). The code is right: `BarterModel { buy_floor_setting: "fBarterBuyMax", .. }` reads the authored values, and `barter.rs`'s module doc states them correctly. The Skyrim half of the same commit is correct.
- **Impact**: The capture is the authority for every CHARAL constant. A future reader "fixing" the code to match it would raise FO4 sell prices from 80 % to 95 % of value.
- **Suggested Fix**: Replace the paragraph: Fallout4.esm authors `fBarterBuyMax` 1.2 / `fBarterSellMax` 0.8 (the executable defaults 1.05 / 0.95 apply only when a load order removes them).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the Skyrim half of the same barter capture paragraph)
