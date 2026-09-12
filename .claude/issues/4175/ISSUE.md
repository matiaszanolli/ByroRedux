# D3-NEW-01: CommonNamedFields::from_subs (identity-remap) is still callable and used by ~40 production parsers

URL: https://github.com/matiaszanolli/ByroRedux/issues/4175
Labels: bug, low, tech-debt, esm-plugin

---

**Severity**: LOW
**Dimension**: FormID Remap, Load Order & ESL Space
**Record / Sub-record**: `SCRI`/`VMAD` (via the shared `CommonNamedFields` decoder)
**Location**: `crates/plugin/src/esm/records/common.rs:293-295` (the `from_subs` passthrough), with ~40 production call sites across `soun.rs`, `gras.rs`, `outfit.rs`, `misc/character.rs`, `misc/equipment.rs`, `misc/scene.rs`, `misc/effects.rs`, `misc/world.rs`, `misc/quest.rs`, `misc/pack.rs`, `misc/dialogue.rs`, `misc/water.rs`, `misc/magic.rs`, `actor/mod.rs`
**Status**: NEW (dedup search against the live open-issue list found no matching GitHub issue)

**Description**: `CommonNamedFields::from_subs(subs)` is a thin wrapper over `from_subs_with_remap(subs, &None)` — it silently discards whatever real `FormIdRemap` the calling parser has in scope. Today this is harmless only because none of the ~40 identity-path call sites' record structs keep `script_form_id`/`has_script`/`script_instance` past the `CommonNamedFields` struct — they only copy `editor_id`/`full_name`/`model_path`/`icon_path` out. That safety property is not enforced anywhere; it depends on every future edit to one of these ~40 record structs remembering not to add a FormID-bearing field sourced from the identity path. This is structurally the same trap #2189 and #4067 already fell into twice for exactly this method pair.

**Evidence**: `common.rs:293-295` (`from_subs` → `from_subs_with_remap(subs, &None)`); e.g. `actor/mod.rs:1441-1450` (`parse_race` takes and uses `remap` for its own fields but still calls the identity `from_subs`).

**Impact**: No live impact today (exhaustively verified: every production read of `common.script_form_id`/`has_script`/`script_instance` goes through a `from_subs_with_remap` call site). Becomes a live defect the moment a future change adds one of those fields to a record struct that currently only calls `from_subs`.

**Related**: #4067 (D7-01, same method pair, already fixed twice on the other side); #2189.

**Suggested Fix**: Delete `CommonNamedFields::from_subs` (identity) and require every caller to pass its `remap` explicitly (threading `&None` only at genuinely-remap-free call sites), removing the trap rather than fencing it. Alternatively add a guard test alongside the existing remap-coverage guards that fails when a FormID-bearing field is populated from an identity-path `common`.

## Completeness Checks
- [ ] **SIBLING**: All ~40 identity-path call sites re-audited if `from_subs` is kept rather than deleted
- [ ] **TESTS**: A guard test alongside the existing remap-coverage guards catches a future FormID-bearing field sourced from the identity path

