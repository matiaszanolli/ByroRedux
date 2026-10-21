# #6272: TD2-2026-10-21-02: a BC4 block decoder was written twice in the same commit, and generic codecs live in consumer modules

**Labels**: low,import-pipeline,terrain-exterior,tech-debt,bug

**Source**: docs/audits/AUDIT_TECH_DEBT_2026-10-21.md · **Severity**: LOW · **Dimension**: 2 · **Location**: `crates/terrain-height/src/bc4.rs:112-131` (`decode_block`) vs `crates/menuxml/src/tex.rs:279-307` (`bc4_block`); `crates/terrain-height/src/lib.rs:197-221` (`decode_top_mip` re-parses the DX10 header by hand: `148`, `84..88`, `rd(128) != 80`) vs `menuxml::tex::Rgba8::decode_dds` → `decode_dx10`; …

- **Severity**: LOW
- **Dimension**: 2
- **Location**: `crates/terrain-height/src/bc4.rs:112-131` (`decode_block`) vs `crates/menuxml/src/tex.rs:279-307` (`bc4_block`); `crates/terrain-height/src/lib.rs:197-221` (`decode_top_mip` re-parses the DX10 header by hand: `148`, `84..88`, `rd(128) != 80`) vs `menuxml::tex::Rgba8::decode_dds` → `decode_dx10`; `crates/plugin/src/esm/records/lmsw.rs:32-33` importing `parse_bgs_reflection_with_remap` / `ReflectionObject` from `weather_settings{,_instance}.rs`
- **Status**: NEW
- **Age**: `5017a64c2` (2026-10-20, #6195) added both BC4 decoders and widened menuxml's decoder to BC4/BC5/DX10 "for engine textures"; `17bd40f4f` (2026-10-21, #5757) made LMSW/MTPT the second consumer of the WTHS reflection decoder
- **Effort**: small
- **Description**: The two `decode_block`/`bc4_block` bodies are identical: same selector unpack, same 8- and 6-value palettes. `decode_top_mip` re-implements the DX10 header walk that `Rgba8::decode_dds` already does, and its only callers (`commands/terrain.rs:202`, tests, the example) run in the bin, which already depends on menuxml. More broadly, the workspace's CPU DDS codec now lives in the MenuXml UI crate. It serves `tex.dump`, terrain heights and HUD art, while `byroredux/examples/msn_basis_probe.rs` keeps a fourth BC1 decoder. The Bethesda reflection decoder (`parse_bgs_reflection*`) lives in the WTHS weather-settings module while it decodes LMSW and MTPT material records.
- **Evidence**: `grep -rn 'fn decode_block\|fn bc4_block\|fn decode_dds' crates byroredux` lists 2 BC4 decoders and 3 DDS front doors (renderer `vulkan/dds.rs` parses headers for upload only).
- **Impact**: A BC4 correctness fix (six-value-mode edge cases) has to be found twice. Nothing in the module names tells a reader where the general texture codec lives.
- **Related**: PERF-D3-01 (height-map full DDS decode on cache hit), CONC-2026-10-21-01/02 (height job panics / temp name).
- **Suggested Fix**: Move the CPU DDS codec (`menuxml::tex` decode half) to a neutral home, such as a `byroredux-bsa`-adjacent `texdecode` module or `core`. Have `terrain-height` take decoded `Rgba8`/BC4 bytes from it, or keep only the encoder. Rename `weather_settings.rs`'s reflection half to `records/reflection.rs` and re-export it.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix

_Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-21.md` (HEAD 4a3e89d56). Fix with `/fix-issue <number>`._
