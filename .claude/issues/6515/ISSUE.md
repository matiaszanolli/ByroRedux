# SKY-D3-01: Skyrim's Use Attack Data (0x0800) and Use Keywords (0x1000) template bits are absent from the enumerated set, and Use Keywords has no consumer: KeywordTable reads the shell's own KWDA

Labels: low, esm-plugin, bug, game:skyrim, legacy-compat
Source: docs/audits/AUDIT_SKYRIM_2026-10-27.md

**Source**: `docs/audits/AUDIT_SKYRIM_2026-10-27.md` finding `SKY-D3-01` (Dimension 3, NPC equip / template chain #4086) · Severity LOW · Status NEW (related to open #4137 and open #6434) · Game: Skyrim.

**Location**: `crates/plugin/src/esm/records/actor/npc.rs:382-406` (`template_flags` doc); `crates/plugin/src/equip.rs` (`TEMPLATE_FLAG_*` set, today's eae8fd740 rewrite); `crates/scripting/src/keywords.rs:40-49` (`KeywordTable::from_index`).

## Description
Skyrim's ACBS template-flags u16 at offset 18 carries 13 bits. UESP *Skyrim Mod:Mod File Format/NPC* lists `0x800 Use Attack Data` and `0x1000 Use keywords`, and so does the CK wiki *Template Data* page. The `template_flags` doc and today's equip.rs rewrite list 0x0001–0x0400 and say "Only `0x0200` Script and `0x0400` Def Pack List are parsed and stored with no consumer yet". Neither 0x0800 nor 0x1000 has a constant or a mention. `KeywordTable::from_index` chains `npc.keywords` (the placed shell's KWDA) with the race's keywords, and never walks TPLT under 0x1000.

## Evidence
Current reader:
```rust
let keywords: HashSet<u32> = npc.keywords.iter()
    .chain(race.into_iter().flat_map(|race| race.keywords.iter()))
    .copied().collect();
```
A byte-walk census over `Skyrim.esm` (5,118 `NPC_`), from the audit:
- 768 set Use Keywords, and 716 of those author no KWDA.
- Templated shells that lose keywords their terminal authors: 2 through an `NPC_` chain (`E3demoGiant01`/`E3demoGiant02` -> `EncGiant01`), and 10 through an LVLN template whose leveled members author KWDA (`LvlHorse`, `LvlHorseSaddled`, `LvlHorseSaddledPlayer`, `LvlFrostbiteSpiderNoGiantAmbush`, ...).
- 3,596 set Use Attack Data. ATKD is not parsed at all, so that bit is a doc gap only.

## Impact
`HasKeyword`-family conditions on those templated actors evaluate against an empty or wrong set. The blast radius is small in vanilla. The doc understates the gap on the day it was written.

## Related
#4137, #6434 (the race facet of the same reader), #5498 (FO4 TPTA already reserves the Keywords slot; FO4 shares the gap).

## Suggested Fix
Add `TEMPLATE_FLAG_USE_KEYWORDS = 0x1000` and a `ResolvedNpc` keywords terminal, and have `KeywordTable` read it. Fold #6434's race fix in at the same time. Enumerate 0x0800 in the doc as "not parsed".

## Completeness Checks
- [ ] **SIBLING**: FO4 (`TPTA` Keywords slot) and Starfield template resolution checked for the same gap
- [ ] **LOCK_ORDER**: If `KeywordTable` gains a template walk, no new multi-storage guard nesting is introduced
- [ ] **TESTS**: A pin covers a templated shell with Use Keywords set, resolving the terminal's KWDA (e.g. `LvlHorse`)

