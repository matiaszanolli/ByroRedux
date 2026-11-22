# #7104 — ESM-D2-08: New Oblivion decode arms use raw indexing, bare length gates and slice `try_into().unwrap()`

Labels: low,esm-plugin,tech-debt,bug,game:oblivion
Filed from: docs/audits/AUDIT_ESM_2026-11-20.md (finding ESM-D2-08)

Source: `docs/audits/AUDIT_ESM_2026-11-20.md` (`/audit-esm`, HEAD 47ae285c6 / f3713bf5e) · ID **ESM-D2-08**

- **Severity**: LOW
- **Dimension**: Sub-Record Byte Accounting
- **Record / Sub-record**: NPC_ `DATA` (33), CREA `DATA` (20), `AIDT`, ACBS, ALCH `ENIT`, `EFIT` / `SCIT`, WTHR `SNAM`, DOBJ `DATA`, DOOR `FNAM`
- **Location**:
  - `crates/plugin/src/esm/records/actor/npc.rs:727-743` (`sub.data.len() == 33` / `== 20` bare, `&sub.data[..21]`, `[21]`, `[22]`, `[25..33]`, `data[0..4]`, `[6]`, `[7]`, `[10]`, `[11]`, `[12..20]` — beside the named `CREATURE_DATA_LEN` and a `SubReader` for the FO3/FNV CREA arm directly below), `:823-828` (`sub.data[..4].try_into().expect("length checked")`), `:903` (`>= 16` bare; the FO4/Starfield ACBS arms use `ACBS_FO4_LEN` / `ACBS_STARFIELD_LEN`)
  - `crates/plugin/src/esm/records/actor/mod.rs:81-88` (Oblivion `decode_ai_data`: `data.len() >= 2` then `data[0]` / `data[1]` for a 12-byte struct)
  - `crates/plugin/src/esm/records/items.rs:1303-1306` (`s.data.len() >= 5 ... s.data[4] & 2`) and `:1327-1330` (`< 5`); the ENIT is 8 bytes
  - `crates/plugin/src/esm/records/misc/magic.rs:859, 864, 877` (`bytes.try_into().unwrap()` x3 inside `MagicEffectAccumulator::feed`, joining the pre-existing one at `:824`)
  - `crates/plugin/src/esm/records/weather.rs:883` (`read_weather_sound`: `bytes.try_into().unwrap()`)
  - `crates/plugin/src/esm/records/misc/music.rs:236-246` (`parse_default_music`: private `get(at..at+4)?.try_into().ok()?` cursor plus a cryptic `data.get((DOBJ_FIRST_MUSIC_SLOT + 5) * 4 - 1)?` length probe)
  - `crates/plugin/src/esm/cell/support.rs:87` (`door.fnam = sub.data[0]`)
- **Status**: NEW. The checklist is explicit ("a new ... `unwrap` on a slice-to-array in a decoder is the regression"; "a new bare numeric gate ... is the regression"). All are infallible given the preceding length checks, so this is hygiene, not a panic path. `try_into().unwrap()` also still exists in older decoders (e.g. `load_screen.rs:115-127, 249-254`, `weather.rs:539, 736`, `misc/magic.rs:84, 1158`, `items.rs:1424, 1460`), so the rule is not currently enforced by any guard.
- **Description**: the window's Oblivion actor-value / spell / ingestible work was written by-index rather than through `SubReader` + named constants. Every width below is correct against xEdit and the shipped master (see Checked and clean), but the gates are looser than the struct sizes (`AIDT >= 2` for a 12-byte struct, `ENIT >= 5` for 8), so a plugin that truncates the struct decodes a prefix instead of declining.
- **Evidence**: widths verified, not defective: Oblivion.esm NPC_ `DATA` 33 B x2,482, CREA `DATA` 20 B x914, `ACBS` 16 B x3,396, `AIDT` 12 B x3,396, ALCH `ENIT` 8 B x253, `EFIT` 24 B on all (ALCH 330, INGR 656, SPEL 1,856, ENCH 2,411, SGST 300).
- **Impact**: none on vanilla; the cursor/gate convention drifts again exactly as `ESM-D2-01` described one window earlier.
- **Suggested fix**: `OBLIVION_NPC_DATA_LEN = 33`, `OBLIVION_CREA_DATA_LEN = 20`, `OBLIVION_ACBS_LEN = 16`, `OBLIVION_AIDT_LEN = 12`, `OBLIVION_ENIT_LEN = 8`; read through `SubReader` (`fixed::<21>()`, `u16()`, `skip_to_or_eof`) and `SubReader::fixed::<4>()` for the AIDT bytes; in the accumulator read `EFIT` with one `SubReader` (name @0, magnitude @4, area @8, duration @12, range @16, actor value @20) instead of three `get(..).map(unwrap)` copies. Consider extending the #6367 text guard to flag `try_into().unwrap()` in `esm/records` (it would need an allowlist for the pre-existing sites).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related decoders / other games' arms (per-game layout splits, other callers of the same helper)
- [ ] **TESTS**: A regression test pins this specific fix (synthetic fixture in the default lane; real-data pins stay in the ignored lane)
