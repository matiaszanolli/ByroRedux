# ESM-D3-02: REGN `RDMD` (Oblivion / FO3 music-type enum) is remapped by the #3401 post-pass (inverse shape 4)

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/6787
Labels: low, esm-plugin, bug, game:oblivion, game:fo3
Source: docs/audits/AUDIT_ESM_2026-11-08.md

_Filed from `docs/audits/AUDIT_ESM_2026-11-08.md` (audit HEAD 3500d2737)._

**Related**: Adjacent to open #3816 (Oblivion `RDMD` music-type enum decode).

- **Severity**: LOW
- **Dimension**: 3
- **Record/Sub-record**: REGN / RDMD (Oblivion; two FO3 records)
- **Location**: `crates/plugin/src/esm/records/misc/world.rs` - merge arm `b"RDMD" | b"RDMO" | b"RDSB"` (~line 1203);
  post-pass `*music = music.map(|f| remap_fid(f, remap));` (~line 1088); doc `world.rs:722-735`
- **Status**: NEW. Adjacent to open #3816 ("decode ... Oblivion RDMD music-type enum"). #3811 was closed doc-only, and the
  remap misroute is not mentioned in either.
- **Description**: The three sub-records share one `music: Option<u32>`. The field doc states that Oblivion `RDMD` is "NOT a
  FormID at all ... a `uint32` enum (0 Default, 1 Public, 2 Dungeon)" (xEdit `wbDefinitionsTES4.pas:4668`
  `wbInteger(RDMD, 'Music Type', itU32, wbMusicEnum)`; FNV `wbDefinitionsFNV.pas:7861` likewise). The post-pass
  remaps `music` unconditionally.
- **Evidence**: Same `compose` arithmetic as ESM-D3-01. A masterless Oblivion/FO3 plugin at slot N>0 turns `RDMD = 1`
  into `0x0N000001`. Vanilla Oblivion.esm (slot 0) is identity. This only bites mods.
- **Impact**: Latent, and also unconsumed correctly: #3816 is the open decode work, and `dispatch_region_ambient_music`
  already misses on this field. It will bite when #3816 decodes `RDMD`. A related schema point for #3816: FNV xEdit
  defines `RDMO` (MUSC, `:7862`) and `RDSB` (MSET, `:7865`) as separate fields. The shared `music` slot lets the later one win
  if a record carries both. The #3915 census says FNV data only has `RDSB`, so this is unmeasured and not claimed as a bug.
- **Suggested fix**: Keep the raw word for `RDMD` (a separate `music_type_enum: Option<u32>` is the natural shape for #3816)
  and remap only `RDMO`/`RDSB`. Pin with a test under a non-identity remap.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related decoders / other games' arms
- [ ] **LOCK_ORDER**: n/a unless a RwLock scope changes
- [ ] **TESTS**: A regression test pins this specific fix (real-data pin where an install exists)
