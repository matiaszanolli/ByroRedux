# #6977: TD4-2026-11-15-02: the tech-debt SKILL.md Phase-1 snapshot paragraph: figures that have drifted

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/6977
- **Labels**: low,documentation,doc-rot,tech-debt
- **Source**: docs/audits/AUDIT_TECH_DEBT_2026-11-15.md

From the tech-debt audit `docs/audits/AUDIT_TECH_DEBT_2026-11-15.md` (HEAD 995ed089b).

- **Severity**: LOW. **Dimension**: 4. **Effort**: trivial (one `/audit-sync` pass). **Labels**: `tech-debt`, `doc-rot`. **Existing**: none.
- **Method**: re-ran the Phase-1 greps and `prod_loc` at HEAD (the paragraph says "diff direction only, re-run, never quote", so these are listed for the next author, not as errors).
- **Drifted** (`.claude/commands/audit-tech-debt/SKILL.md:80-135`):
  - `allow(dead_code)` quoted 43, now 46: new at `crates/renderer/src/vulkan/context/mod.rs`, `crates/renderer/src/vulkan/shadow_filter.rs` and `byroredux/src/render/skin_offsets_hasher_tests.rs` (0 -> 1 each since 8ff82d514). The paragraph's list of the "five new" is therefore incomplete.
  - Audio split: `crates/audio/src/` "largest child ~610" is now `world.rs` 736 prod LOC (+21 %, 719 at 8ff82d514); hub `lib.rs` 202 (said ~200).
  - Largest children: `byroredux/src/inventory/` ~790 -> `catalog.rs` 852 (HEAD, +8 %); `byroredux/src/npc_spawn/` ~1460 -> `ai_package.rs` 1538 (+5 %); `crates/debug-ui/src/panels/game_menu.rs` 749 -> 757.
  - Watch list: `byroredux/src/app_step.rs` 1899 -> 1935 (now 65 from the band; the row text says "within 5 %"); `crates/plugin/src/esm/records/misc/world.rs` 2054 -> 2052; `byroredux/src/systems/character.rs` (1834 at 8ff82d514, 1842 now) and `crates/physics/src/world/mod.rs` (1807) are over 1800 production LOC and named nowhere in the paragraph.
  - ROADMAP.md quoted 797, now 798.
- **Verified unchanged**: markers 40 (18 `XXXX` + 18 `HACK` + 3 `FIXME` + 1 `TBD`), `unimplemented!/todo!()` 0, `#[ignore]` 436 (quoted 435), files >2000 production LOC 2 (`shader_constants_data.rs` 3502, `world.rs`), test files >2000 total LOC 95, `studio.rs` 883, `gpu_timers.rs` 1196, `context/init.rs` 1131, `mesh.rs` 1412, `fragment/vm` largest child ~1400 (1422), `nif/import/types` 575, `obscript_effects` ~550 (556), `mq101_conformance` ~530 (532), `panels/studio.rs` and the two other children.
- **Fix**: refresh on the next `/audit-sync`; add `character.rs` and `physics/src/world/mod.rs` to the watch list.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
