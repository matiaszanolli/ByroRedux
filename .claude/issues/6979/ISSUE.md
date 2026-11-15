# #6979: TD6-2026-11-15-01: The `EsmIndex` unconsumed-record inventories are stale in both directions

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/6979
- **Labels**: low,esm-plugin,documentation,doc-rot,tech-debt
- **Source**: docs/audits/AUDIT_TECH_DEBT_2026-11-15.md

From the tech-debt audit `docs/audits/AUDIT_TECH_DEBT_2026-11-15.md` (HEAD 995ed089b).

- **Severity**: LOW
- **Dimension**: 6 - Stub & Placeholder Implementations (production-unreachable) / doc rot
- **Location**: `docs/engine/esm-records.md:441-455`; `crates/plugin/src/esm/records/index.rs:640`; `ROADMAP.md:673-691`
- **Age**: the media_sets claim went stale at `45b3b0a64` (2026-11-14, the music controller); the inventories were last declared "complete" at `9398d788a` (2026-11-08, #6805 / #6806)
- **Effort**: small
- **Labels**: tech-debt, doc-rot
- **Status**: NEW (the earlier rows, #6387 / TD6-2026-10-25-01, are CLOSED; #6805 / #6806 are CLOSED as "inventory is complete")
- **Description**:
  1. **Wrongly listed as unconsumed.** `esm-records.md:441-446` lists `media_sets` (MSET) among nine maps that "nothing outside `records/` reads" and `index.rs:640` says "no runtime reads it yet; that is the open #3816". `byroredux/src/music/translate.rs:169,443,447,453` reads `index.media_sets` in production (non-test) code, and `media_locations` (ALOC) is read at `music/mod.rs:236` and `translate.rs:163,198,441`. The Fallout 3 / New Vegas / Oblivion music path consumes them; only Skyrim `MUST` and Starfield remain open.
  2. **Genuinely unconsumed but on neither list.** A scan of every `pub` `EsmIndex` map for readers outside `records/index.rs`, `records/dispatch_*` and tests finds these with none: `debris` (DEBR), `casinos` (CSNO), `music_tracks` (typed `MustRecord`, doc "what a Skyrim `MUSC.TNAM` lists"), `soul_gems` (typed `SlgmRecord`), and the Oblivion-unique stubs `clothing`, `apparatuses`, `sigil_stones` (only `sigil_stones` is on the ROADMAP list; the first three also dual-fill `cells.statics`, so their maps are unread). `ROADMAP.md:673` counts "Fifteen" maps and lists exactly fifteen, so the count is wrong by at least `debris` and `casinos`. Other fields that look unread in a grep (`movement_types`, `body_parts`, `navi_info`, `object_mod_loose_items`, `magic_effects_by_code`) are consumed through `EsmIndex` accessors or `systems/vats.rs:516` and are fine.
  3. The ROADMAP list is honest about the typed-but-unwired set otherwise (`enchantments`, `reputations`, `combat_styles`, `impact_data_sets`, `impacts`, `weather_settings`, `volumetric_lighting`, `god_rays` still have zero readers: re-verified).
- **Evidence**: `grep -rnE '\.(enchantments|reputations|combat_styles|impact_data_sets|impacts|media_sets|weather_settings|volumetric_lighting|god_rays)\b' crates byroredux` (the doc's own re-check recipe) minus tests and `records/` returns the `music/translate.rs` hits for `media_sets` and nothing for the other eight.
- **Impact**: the doc's own recipe now disagrees with its list, and the long-tail allocation cost the ROADMAP row exists to track (a `HashMap` plus `String`s per stub record on every FNV/FO3/Oblivion load) is under-counted by the unlisted maps.
- **Suggested Fix**: in `esm-records.md` move `media_sets` out of the unconsumed list (eight maps) and add `music_tracks` and `soul_gems` to it; fix `index.rs:640`'s comment to say what the music controller reads and that only Skyrim `MUST` remains; in `ROADMAP.md:673-691` add `debris` and `casinos` to the "candidates to drop" list and correct the count. A source-scan guard that derives the list the way the recipe does would keep it honest, but is optional.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
