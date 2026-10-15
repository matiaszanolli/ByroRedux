# TD2-2026-10-15-05: Four WATR `DATA`/`DNAM` decoders restate the same wind/wave/specular/fog/colour head in three read styles; every correction (#3105, #3107, #3144, #3146, #4931) was a per-decoder fix

Labels: medium,esm-plugin,tech-debt,bug

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-10-15.md` (finding `TD2-2026-10-15-05`).

- **Severity**: MEDIUM
- **Dimension**: 2 - Logic Duplication
- **Location**: `crates/plugin/src/esm/records/misc/water.rs`: `decode_data_short` `:500-568`, `decode_data_oblivion` `:570-680`, `decode_data_fo3nv` `:681-740`, `decode_dnam_pre_fo4` `:864-933` (the same head again at `decode_dnam_fo4` `:1118`)
- **Status**: NEW
- **Age**: head repeated since the WATR decoders landed; #3146 `845c1aa30` (2026-09-09) deleted one unreachable copy of it; #4931 `55dd8fa7e` (2026-09-28) diverged the authored flag per arm
- **Effort**: medium
- **Description**: wind speed at 0, wind direction (degrees -> radians, #3144) at 4, wave amplitude/frequency at 8/12, `sun_specular_power.clamp(1.0, 2048.0)`, reflectivity/fresnel `clamp(0,1)`, `fog_near.max(0)`, `fog_far.max(near+1)` and three colours appear in each, once through `SubReader::f32_finite`, once through `read_f32_at` + `for (dst, offset) in [...]`, once through `u8_to_linear(data[40])`. Only the fog/colour offsets (+4 on Oblivion, +0 elsewhere) and the authored-flag policy differ, and that is exactly what a layout table would state.
- **Evidence**: the doc comments are a list of one-copy-fixed bugs: `decode_data_short`'s comment says its tail reads "used to sit here too, structurally unreachable ... assigning offsets that contradicted the long decoder for the same fields (#3146)"; `decode_data_fo3nv`'s #3107 note says the majority 196/184-byte `DNAM` form "ran `decode_dnam_pre_fo4` alone ... left at canonical defaults on 79% of FO3 and 90% of FNV water types"; #3105 swapped the Oblivion rain/displacement starting sizes (5x wrong, "both live to the GPU").
- **Impact**: WATAL (the all-games water goal) consumes these values straight into `GpuWaterParams`; a missed copy is a wrong-looking water type on one game, found only by a per-record bytes census.
- **Related**: #3105, #3107, #3144, #3146, #4931, #5596 (the `read_f32_at` helper).
- **Suggested Fix**: `struct HeadLayout { fog_near: usize, colours: usize, wind_direction_authored: fn(f32)->bool }` and one `fn decode_legacy_head(p: &mut WaterParams, data: &[u8], layout: &HeadLayout)` in `water.rs`; the four decoders call it and keep only their tails (`apply_fo3nv_tail`, Oblivion simulator block, Skyrim tail). Add a test that decodes the same synthetic bytes through each layout and pins the shared fields.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files/docs
- [ ] **TESTS**: A regression test or guard pins this specific fix (or the existing guard is named)
