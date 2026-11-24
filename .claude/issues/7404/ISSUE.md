# #7404 SK-D4-01: `EsmReader::skip_group` with opaque collection on recurses without the GRUP depth cap; a crafted mod plugin aborts the process

**Labels**: high,esm-plugin,safety,bug,game:skyrim,legacy-compat

**Source**: `docs/audits/AUDIT_SKYRIM_2026-11-23.md` — finding `SK-D4-01` (HEAD `e01ca71f0`, re-validated against `origin/main` `4e68ab7b7` at publish).

- **Severity**: HIGH (precedent: #3237, #3503 and #5687 are the same defect class and carry `high`)
- **Dimension**: Multi-master load order, ESM reader (mechanism owner `/audit-esm`; found through this dimension's fold)
- **Location**: `crates/plugin/src/esm/reader.rs:1181-1204` (`skip_group`, `note_skipped_content`); enabled by `byroredux/src/cell_loader/load_order.rs:1057` (`ParseOptions { collect_opaque: idx > 0 }`)
- **Status**: NEW — same class as closed #3237 / #3503 (both HIGH), at a new site (regression of the #3237 / #3503 class at a new site, introduced by 55f4f8bee, LMC-REC-010). Searched open and closed issues for `note_skipped_content`, `collect_opaque`, "nested GRUP", "skip_group recursion": nothing covers it.
- **Description**: With collection on, `skip_group` calls `note_skipped_content(end)`, which reads the skipped group's content and calls itself for every nested GRUP with `group_end = (pos + group_content_len(&group)).min(end)`. There is no `depth` parameter and no use of `bounded_group_content_end` / `MAX_GRUP_NESTING_DEPTH` (64). `reader.rs` documents exactly this shape as forbidden ("A self-recursive walker must not call this ... thread a `depth` parameter"). Collection is on for every plugin after the first in the fold, and any top-level GRUP label with no dispatch arm reaches `skip_group` (`records/parse.rs`, the `_` arm; also the FO4+-only labels on Skyrim). A nest of minimal 24-byte GRUPs therefore recurses once per 24 bytes of file.
- **Evidence**: probe crate against `byroredux-plugin` (path dependency, scratchpad), crafted Skyrim-header plugin (HEDR 1.71, form version 44) with one top-level `ZZZZ` GRUP holding N nested empty GRUPs, parsed on a 2 MiB thread (the rayon worker stack the fold uses):

  | build | N | file size | `collect_opaque` | result |
  |---|---|---|---|---|
  | dev | 20,000 | 480 KB | false | ok |
  | dev | 20,000 | 480 KB | true | `thread has overflowed its stack ... aborting`, exit 134 |
  | release | 15,000 | 360 KB | true | ok |
  | release | 20,000 and 50,000 | 480 KB and 1.2 MB | true | stack overflow, exit 134 |

  The existing table-driven guard `every_recursive_grup_walker_honours_the_shared_depth_cap` (`crates/plugin/src/esm/records/grup_walker.rs`) lists the `records/` and `cell/` walkers and never calls `skip_group` with collection on, so nothing fails.
- **Impact**: any `.esp` / `.esm` that is not the first plugin of a profile (every mod; the official DLC too) can abort the engine at load, before any cell opens, with an uncatchable error rather than a skipped group. Vanilla Skyrim data is unaffected (the real plugins nest at most a few levels; the SE set parses and folds). Hostile or corrupted mod input only, but this is the load path the LMC work opens to arbitrary mods.
- **Related**: #3237, #3503, #5687, #4644 (clamp), LMC-REC-010 / 55f4f8bee.
- **Suggested Fix**: give `note_skipped_content` a `depth` and stop descending at `MAX_GRUP_NESTING_DEPTH` (or make it iterative with an explicit stack and a depth counter), clamping to the parent end as `bounded_group_content_end` does; add `skip_group` with `enable_opaque_collection` to the table in `every_recursive_grup_walker_honours_the_shared_depth_cap`.

## Completeness Checks
- [ ] **SIBLING**: every other self-recursive walker reachable with opaque collection on (`skip_group`, `note_skipped_content`, any GRUP skip the collector hooks) honours `MAX_GRUP_NESTING_DEPTH`
- [ ] **TESTS**: `skip_group` with opaque collection on joins the table in `every_recursive_grup_walker_honours_the_shared_depth_cap`
