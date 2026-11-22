# #7197 CHAR-2026-11-21-M-05: Oblivion Fatigue/Magicka regen constants ignore the GMSTs Oblivion.esm authors, now that the lane is live

**Labels**: medium,character,bug,game:oblivion

From `docs/audits/AUDIT_CHARACTER_2026-11-21.md` (`/audit-character`, HEAD `a06d5f9db`).

- **Reported as**: CHAR-D3-02
- **Severity**: MEDIUM
- **Dimension**: Progression & Pools
- **Game**: oblivion
- **Location**: `crates/core/src/character/regen.rs:73-91` (`FATIGUE_REGEN_PER_SEC` at :73, `MAGICKA_REGEN_WILLPOWER_COEFF`, `MAGICKA_REGEN_BASE`, `magicka_regen_per_sec`), `regen.rs:246` and `:285` (tick body); installed by `CharacterRulesProfile::build_tes_pool_regen` (`profile.rs:900`, `cell_loader/references/mod.rs:379`)
- **Status**: NEW. The baseline recorded these rows as "PASS (lane `None` everywhere)", which stopped being true when `278886579` (2026-11-15) made `build_tes_pool_regen` resolve the lanes; no open or closed issue covers it (searched "fFatigueReturnBase", "regen GMST retune").
- **Source**: `charal-oblivion-ruleset.md:580` (`FatigueRegen = Endurance x fFatigueReturnMult(0.0) + fFatigueReturnBase(10.0)`), `:732-736` (`MagickaRegen = (Willpower x 0.02 + 0.75) x MaxMagicka/100`); **Oblivion.esm byte scan**: `fFatigueReturnBase` 10.0, `fFatigueReturnMult` 0.0, `fMagickaReturnBase` 0.75, `fMagickaReturnMult` 0.02 are all authored records.
- **Description**: The four numbers are authored GMSTs, but they are module constants: `Fatigue` is `FATIGUE_REGEN_PER_SEC * elapsed` and the Endurance term (`fFatigueReturnMult`) has no code at all ("correctly not modelled" was true only while the lane was dead). A load order that retunes any of the four (regen overhauls are a staple Oblivion mod type, and the Endurance hook exists precisely for that) is ignored. The doc and `regen.rs:26` also misspell the Endurance setting `fFatigureReturnMult`; the master's name is `fFatigueReturnMult` (zero hits for the typo), so an implementer following the doc would read nothing.
- **Evidence**: `avs.restore(fatigue, FATIGUE_REGEN_PER_SEC * elapsed)`; `(willpower * MAGICKA_REGEN_WILLPOWER_COEFF + MAGICKA_REGEN_BASE) * (max_magicka / 100.0)`; `PoolRegenConfig` carries AVIF ids and (AP lane only) a normalized rate, no Oblivion rates, and `build_tes_pool_regen(resolve)` takes no GMST closure (the AP lane, by contrast, gets its rate through `VatsModel::with_gmst`).
- **Impact**: Mod-compat gap on a live system for Fatigue and Magicka of every actor with the pool (player and NPCs). Vanilla behaviour is correct.
- **Related**: CHAR-D3-01, #6756, `charal.md:263`.
- **Suggested Fix**: Give the Oblivion lane a small `TesPoolRegen { fatigue_base, fatigue_end_mult, magicka_base, magicka_will_mult }` row on `PoolRegenConfig`, built by `build_tes_pool_regen(resolve, gmst)` with today's numbers as fallbacks, add the Endurance term, correct the setting spelling in `regen.rs:26` and `charal.md:263`, and pin with a retuned-GMST test.

### LOW

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other CHARAL rule rows / overlays / profiles)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
