# #5276: UI-D7-2026-10-05-01: #4724's SKIP contract missed `m48-5-fnv-hud.sh`, which still fails on missing data and is absent from the contract checker

Labels: low,ui,bug,test-gap,game:fnv
Filed from: docs/audits/AUDIT_UI_2026-10-05.md

**Source**: `docs/audits/AUDIT_UI_2026-10-05.md` (UI-D7-2026-10-05-01) · audit HEAD `a2c24b16e`

- **Severity**: LOW
- **Dimension**: MenuXml & HUD Drivers
- **Profile**: MenuXml (FNV)
- **Location**:
  - `docs/smoke-tests/m48-5-fnv-hud.sh:46`
  - `scripts/check-playable-smoke-contracts.sh:98`
  - Env-rename residue: `docs/smoke-tests/m48-6-skyrim-hud.sh:25-26,34` and `docs/smoke-tests/m48-menu-load.sh:50-51,57`
- **Status**: Incomplete fix of #4724, which was closed on 2026-10-04. Its title includes "the HUD smokes report
  missing data as FAIL".
- **Description**:
  - `m48-5-fnv-hud.sh:46` still does `{ echo "FAIL: missing $DATA/$f"; exit 1; }`. The other four HUD smokes exit 77
    and print `smoke[<name>]: SKIP -- missing`.
  - The checker loop lists `m48-menu-load m48-4-oblivion-hud m48-5-fo3-hud m48-6-skyrim-hud m48-7-fo4-hud`, without
    FNV, so it cannot catch the omission.
  - The FNV smoke was added by 3536794c3 on 2026-09-25, after #4724 was filed, with the old pattern.
  - Residue from the same family (the 63c0aee3b env rename):
    - `m48-6-skyrim-hud.sh:25-26` documents "BYROREDUX_SKYRIMSE_DATA / BYROREDUX_SKYRIMSE_DATA".
    - `m48-6-skyrim-hud.sh:34` and `m48-menu-load.sh:57` nest
      `${BYROREDUX_SKYRIMSE_DATA:-${BYROREDUX_SKYRIMSE_DATA:-…}}`.
    - `m48-menu-load.sh:50-51` contrasts the variable with itself.
- **Impact**: a runner without data reports FAIL instead of SKIP for the FNV HUD gate, the exact outcome that kept
  the HUD smokes out of CI.
- **Related**: #4724 (closed), #5237 (open: the audit-runtime skill's gate matrix omits `m48-5-fnv-hud`)
- **Suggested Fix**: copy the 77 / `SKIP -- missing` block from `m48-5-fo3-hud.sh`, add `m48-5-fnv-hud` to the checker
  loop, and collapse the doubled variable expressions.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix
