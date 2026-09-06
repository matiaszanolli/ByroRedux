# #4041 — REN-2026-09-06-D6-01: yesterday's fix to the Dimension 6 caller bullet replaced two right facts with two wrong ones

**Labels**: low, nifal, renderer, tech-debt, bug

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D6-01), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: NIFAL Material
- **Location**: `.claude/commands/audit-renderer/SKILL.md` (Dimension 6, the
  "Single boundary" bullet), introduced by `2853464f`
- **Status**: NEW (the incorrect text is new; #3904, the issue whose fix
  introduced it, is closed)
- **Description**: `2853464f` ("Fix #3904: correct two stale NIFAL facts in
  the shared audit skill files") reworded the bullet to read:

  > `translate_material` has three production callers — `byroredux/src/scene/nif_loader.rs`
  > (loose NIF), `byroredux/src/cell_loader/spawn.rs` (REFR placement) and
  > `byroredux/src/cornell.rs` (the Cornell RT test harness).

  The count is right and the set is wrong in three ways. The reworded
  invariant sentence that follows it — "no `Material {…}` literal is
  constructed *outside* `translate_material` … an independently-built
  `Material` downstream is [a finding]" — then contradicts the file it just
  named.
- **Evidence**: `rg -n "translate_material" --type rust` gives four call
  sites, and `rg -n "^#\[cfg\(test\)\]" byroredux/src/cornell.rs` gives one
  hit, at line 1904:
  1. `byroredux/src/scene/nif_loader.rs` — production. ✔ named.
  2. `byroredux/src/cell_loader/spawn/mesh_instance.rs` — production. The
     bullet names `byroredux/src/cell_loader/spawn.rs`, which exists (so the
     path gate passes) but contains **no** `translate_material` call; the
     caller moved into the subdirectory. Yesterday's report said so
     explicitly and the fix wrote the pre-move path back.
  3. `byroredux/src/cell_loader/placement_lod.rs` — production (the exterior
     placement-LOD spawner, #2444). **Omitted entirely.** This is the caller
     `docs/engine/nifal.md` §3 singles out as the one exempt from the two
     Phase-2 resolvers, so it is the caller an auditor most needs to know
     about.
  4. `byroredux/src/cornell.rs` — the call is at line 2073, inside the
     `#[cfg(test)] mod tests` that opens at 1904. **Not a production
     caller**, and the bullet says so in its own parenthetical ("the Cornell
     RT *test* harness") while listing it as production.

  The self-contradiction: `cornell.rs`'s **production** half constructs seven
  `Material` literals directly — `matte`, `pbr`, `pbr_bsdf`, `pbr_bsdf_lobes`,
  `glass`, `emissive`, `fire_refraction` — called from ~15 sites across the
  harness. By the bullet's absolute phrasing those are the finding; in fact
  they are legitimate (an RT reference scene has no `Imported*` tier) and each
  carries a documented rationale (#2477, #2514). `crates/save/src/driver.rs`'s
  `restore_world` is a further documented non-literal producer (#2687).
- **Impact**: An auditor applying this bullet literally reaches one of two
  false conclusions: `placement_lod.rs` is invisible to them, or `cornell.rs`'s
  seven constructors are reported as a boundary violation. Both are exactly
  the failure the bullet's *own* closing clause was rewritten to prevent. The
  bullet has now been wrong in three successive states (two callers → three
  wrong callers), which is what a hand-maintained list does; the structural
  fix yesterday's report asked for — point at the guard test instead — was not
  applied.
- **Related**: #3904 (closed; this is its incomplete half), #2444,
  #3733 (the directory-scan rewrite of the sibling guard — the pattern to
  copy), #1114 (path/symbol convention).
- **Suggested Fix**: Replace the caller enumeration with the invariant plus
  its guard: no `Material` literal outside `translate_material` /
  `translate_texture_only_material` on a *content* path, enforced by
  `every_exterior_spawner_inserts_a_boundary_material`
  (`byroredux/src/material_translate.rs`), with the Cornell harness named as
  the one documented exemption. If a caller list is kept at all, derive it the
  way `documented_texture_role_list_matches_the_struct` derives the role
  count — that test already scans `.claude/commands/` files and could scan one
  more.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: per-game logic stays at the NIFAL parser->`Material` boundary - never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
