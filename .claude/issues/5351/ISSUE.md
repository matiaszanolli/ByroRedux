# #5351: OBL-2026-10-05-D4-01: `OblivionHdrLighting` docs say no renderer reads HNAM (the sunlight dimmer has been consumed since `df59c6362`), and say the 56-byte HNAM is "Oblivion / FO3 / FNV" (FO3/FNV author none)

Labels: low,documentation,doc-rot,esm-plugin,terrain-exterior,game:oblivion,legacy-compat
Filed from: docs/audits/AUDIT_OBLIVION_2026-10-05.md

**Source**: `docs/audits/AUDIT_OBLIVION_2026-10-05.md` (OBL-2026-10-05-D4-01) · audit HEAD `a2c24b16e`

- **Severity**: LOW (doc-rot).
- **Dimension**: Exterior & Lighting Data. The parse-side doc is in `/audit-esm`'s path, but the data fact is Oblivion-only.
- **Location**:
  - `crates/plugin/src/esm/records/weather.rs:113-122`: "**Parse-but-don't-consume gate (TD5-010):** no renderer system reads `OblivionHdrLighting` fields yet".
  - `crates/plugin/src/esm/records/weather.rs:160`: "Wire size of the full 14-field HNAM payload (Oblivion / FO3 / FNV)". This contradicts the same file's `:111` "FNV and Fallout 3 do not ship HNAM at all".
- **Status**: NEW.
  - No match in today's reports.
  - `gh` search "OblivionHdrLighting" finds only the closed #537, #1045, #1057 and #1062.
- **Evidence**:
  - `df59c6362` (2026-09-18, "consume the Oblivion HNAM sunlight dimmer on exterior sun"). `env_translate.rs:1628-1630` translates `hdr.sunlight_dimmer` onto `WeatherDataRes`, and `systems/weather.rs:852-854` / `:929-931` multiply the sampled sun by it.
  - Raw census this run (`/tmp/audit/oblivion/wthr_hnam.py`): WTHR with HNAM is Oblivion 37 / 37, FalloutNV 0 / 63, Fallout3 0 / 27.
- **Impact**:
  - A reader trusting the gate doc would conclude that HNAM is inert and might move or retype it without checking the live consumer.
  - The `WIRE_SIZE` doc invites an FO3/FNV HNAM arm that no data supports.
  - The 13 other fields (eye-adapt, bloom, grass/tree dimmer) are still unconsumed, so the gate statement is only partly true.
- **Related**: #537 (closed), `df59c6362`, `sunlight_dimmer_translates_from_the_hnam_block`.
- **Suggested Fix**:
  - Restate the gate as: `sunlight_dimmer` consumed via `translate_weather` (EXAL); the other 13 fields are still parse-only.
  - Change `:160` to "Oblivion only (FO3/FNV author no HNAM: 0 / 27, 0 / 63)".

## Completeness Checks
- [ ] **SIBLING**: Same drift checked in sibling docs / comments / skill files
- [ ] **TESTS**: Where a number is restated, a test or measured source pins it
