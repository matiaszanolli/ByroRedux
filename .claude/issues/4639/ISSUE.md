# ESM-2026-09-21-D3-01: Starfield's TES4 master flags use the Skyrim/FO4 bit layout — real small-master bit (0x100) ignored, 0x200 (Update) treated as ESL

**Issue**: #4639
**Filed**: 2026-09-22 (audit-publish, AUDIT_ESM_2026-09-21.md)

**Severity**: MEDIUM
**Dimension**: FormID Remap, Load Order & ESL Space
**Game Affected**: Starfield
**Location**: `crates/plugin/src/esm/reader.rs:1043` (`light_master = header.flags & 0x0200 != 0`, every game); doc `crates/plugin/src/esm/reader.rs:588-602`; consumer `byroredux/src/cell_loader/load_order.rs:604`, `:644-675`

## Description
`read_file_header` derives `light_master` from bit `0x200` on every game. On Starfield the small-master bit is `0x100`; `0x200` is *Update* and `0x400` is *Medium*. Oblivion, FO3/FNV, Skyrim LE and FO76 don't support light masters at all.

## Evidence
- `wbInterface.pas:20575-20592` (`IsLight`/`IsUpdate`), `:5358-5361` (`wbIsLightSupported`), `wbDefinitionsSF1.pas:9045-9053`.
- Installed plugins: 16 official masters carry `0x100` (Constellation.esm 0x181, OldMars.esm 0x181, SFBGS004/007/008, sfbgs00a_*, SFBGS00c/019/021/023/047, ...); ~20 mods carry `0x101`; SFBGS003/006, kgcdoom, neonvertigo carry `0x400`. None carries `0x200` today.

## Impact
Every Starfield small master takes a regular slot instead of light space; `allocate_global_slot` hard-errors once `0xFD` regular slots are exceeded — a Creations-heavy load order the game accepts is refused by Redux. Redux's global FormIDs diverge from the game's FE-space ids. A `0x200`-flagged plugin (or any plugin on a non-ESL title) gets wrongly packed into `0xFE` light space with 12-bit-truncated object ids, colliding forms in `EsmIndex`.

## Suggested Fix
Decode per game using the already-available variant/GameKind. Starfield: `0x100`=small, `0x400`=medium (new `GlobalSlot::Medium` = `0xFD`+8-bit index+16-bit id), ignore `0x200`. SSE/FO4: keep `0x200`. Every other game: no light masters.

## Related
#1554, #3546, #4384 (all closed — #4384 closed with "fold ESH 0xFD into GlobalSlot when needed"; the Medium half is this finding's scope)

## Source
docs/audits/AUDIT_ESM_2026-09-21.md (ESM-2026-09-21-D3-01)
