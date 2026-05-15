## Description

`#1047` closed administratively (the **parser-side** gating-milestone comments are present), but the parse-but-don't-consume **family** is still live. The fields are parsed by `crates/plugin/src/esm/records/` and `crates/nif/src/import/`, surfaced into runtime data structures, then never read by any system.

Today's audit (`AUDIT_TECH_DEBT_2026-05-14.md`, Dim 5) verified 7 still-live cases and surfaced **4 net-new** ungated parse-but-don't-consume cases that didn't make it into `#1047`'s original scope.

Severity: MEDIUM. The risk is "looks supported, drops on the floor at runtime" — the inverse of the `unimplemented!()` trap (no panic, no warning, just silent data loss).

## Carryover from prior audit (still live; reconfirmed 2026-05-14)

### TD5-001 — SpeedTree `--tree` CLI returns placeholder billboard
- `crates/spt/src/import/mod.rs:116-180` (`import_spt_scene` → `placeholder_root_node` → `placeholder_billboard_mesh`)
- Reachable from `cargo run -- --bsa "Fallout - Meshes.bsa" --tree trees\\joshua01.spt` (documented in CLAUDE.md) AND `docs/smoke-tests/m-trees.sh`
- Gated by SpeedTree Phase 2 (no ROADMAP row). **Add Tier-9 row** "SpeedTree Phase 2 — real geometry tail".

### TD5-002 — `StencilState` parsed (7 sub-fields), pipeline hardcodes `stencil_test_enable(false)`
- Parser: `crates/nif/src/import/material/mod.rs:640` + struct at `:703-740`
- Consumer hardcodes: `pipeline.rs:311 / :461 / :601`, `water.rs:375`, `composite.rs:719`
- Gate: `#337` (parser side); consumer side ungated. **Add Tier-9 row** "Stencil pipeline variants — decals/portals/shadow volumes".

### TD5-003 — `BSSkyShaderProperty` / `BSWaterShaderProperty` flags captured, zero renderer consumers
- `crates/nif/src/import/material/mod.rs:662-680` (`is_sky_object`, `sky_object_type`, `water_shader_flags`)
- Skyrim sky polish (post-#993 partial) is the gating milestone — already tracked.

## Net-new (this audit run)

### TD5-010 — `OblivionHdrLighting` (14 f32 HNAM HDR fields) parsed, zero consumers, **no gating docstring**
- `crates/plugin/src/esm/records/weather.rs` defines `OblivionHdrLighting` and a `parse_hnam` consumer; re-exported from `records/mod.rs:66`. No code reads it.
- The struct docstring lacks any "blocks M-NN" / "consumed by …" gate marker.
- **Fix**: either add a gate marker (`// Consumer-side gated on Tier-5 HDR pipeline polish` or similar) OR delete the parser surface entirely until needed.

### TD5-011 — TREE.SNAM (leaf indices) / TREE.CNAM (canopy params) parsed, zero consumers, ungated
- `crates/plugin/src/esm/records/tree.rs:23,86,154-170`
- Doc-comment says "SpeedTree runtime walks this" — but the SpeedTree runtime isn't wired (TD5-001 placeholder). Two ungated parsers feeding each other.
- **Fix**: cross-link to TD5-001 ROADMAP Tier-9 row; add gate marker in source.

### TD5-013 — FO4 `NpcRecord.face_morphs` parsed, zero consumers, asymmetric
- Sibling field `runtime_facegen` IS consumed in `npc_spawn.rs:619` (M41.0 Phase 3b).
- `face_morphs` is the .tri-morph weights array — needed by M41.0.5 (GPU per-vertex morph runtime, deferred to Tier 5 per ROADMAP).
- **Fix**: add gate marker to the `face_morphs` field documenting the M41.0.5 dependency.

### TD5-016 — BPTD body-parts parsed (FO3/FNV/Skyrim dismemberment routing), zero consumers, ungated
- `crates/plugin/src/esm/records/mod.rs:1082, 1101` + `misc/effects.rs:77`
- Per-NPC dismemberment routing + biped slot model. M41 Phase 4 is the natural consumer.
- **Fix**: add gate marker to `BPTD` parser doc-comment.

## Proposed fix shape

For each MEDIUM, ONE of:

1. **Add gating-milestone comment** to the parser surface (preferred for content the engine will eventually need — TD5-002, TD5-010, TD5-011, TD5-013, TD5-016).
2. **Delete the parser surface** (only for content with no plausible consumer path — none of the above qualify).
3. **Wire a stub consumer** that at least logs the field once on first sight (gives audit visibility without committing to the full feature).

The bar is: **every parsed field has a comment that says either "blocks M-NN" or "consumed by X"**, so the next audit pass can sort gated vs forgotten in one grep.

## Completeness Checks

- [ ] **UNSAFE**: N/A — comment-only or small-deletion changes
- [ ] **SIBLING**: After fixing the 7 listed cases, sweep the rest of `crates/plugin/src/esm/records/` and `crates/nif/src/blocks/` for parser fields with no `// Consumed by`, `// Gated on`, or `// Blocks M` marker. Audit script: `grep -rE "pub [a-z_]+:" crates/plugin/src/esm/records/ | <filter by file with no callers>`.
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: One new test per gate marker that asserts the parser still surfaces the field (just compile-time assertions via `let _: Field = …`).

## Effort
small (~15 LOC of comment additions + 2 ROADMAP Tier-9 rows + sweeping the long-tail in a 30-min follow-up grep)

## Cross-refs

- Audit report: `docs/audits/AUDIT_TECH_DEBT_2026-05-14.md` (Dim 5, TD5-001..003 carryover + TD5-010/011/013/016 net-new)
- Closed hub: `#1047` (administratively closed; the family-still-live nature is the reason for this issue)
- Related ROADMAP rows: M55 (volumetrics — `volumetrics::VOLUMETRIC_OUTPUT_CONSUMED` is the proven gate marker pattern to mimic), M41.0.5 (GPU morph), M28 (physics), Tier-9 SpeedTree Phase 2
