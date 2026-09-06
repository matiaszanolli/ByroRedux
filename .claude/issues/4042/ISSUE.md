# #4042 — REN-2026-09-06-D6-02: `cd8691be`'s new tree-wide `classify_pbr` gate reads `.rs` only, and `triangle.frag` — the render-side file the rule is about — still frames the deleted symbol as live

**Labels**: low, nifal, renderer, shaders, documentation, doc-rot

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D6-02), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: NIFAL Material
- **Location**: `byroredux/src/workspace_hygiene_tests.rs`
  (`no_source_file_frames_the_deleted_classify_pbr_as_live`,
  `collect_live_classify_pbr_claims`) against
  `crates/renderer/shaders/triangle.frag`
- **Status**: NEW
- **Description**: `cd8691be` closed the fourth recurrence of "a doc names the
  deleted render-time `Material::classify_pbr` as live" (#1321 → #1522 →
  #1624 → #3869) and, correctly, replaced the single-file edit with a
  workspace-wide gate. The gate filters to `.rs` files:
  `if path.extension().and_then(|e| e.to_str()) != Some("rs") { continue; }`.
  One live claim survives, in the one file where the claim is most damaging.
- **Evidence**: `rg -n --glob '!target' -w 'classify_pbr' --glob '!*.rs'
  --glob '!*.md' .` returns exactly one hit,
  `crates/renderer/shaders/triangle.frag`:

  > `*   Legacy NIF (Oblivion / FO3 / FNV) — `classify_pbr` keyword`
  > `    fallback fills the same fields from texture-path tokens.`

  Present tense, no historic marker — the gate's `HISTORIC_MARKERS` list
  would reject this line verbatim if it could see it. The comment is wrong on
  two counts: the render-time `Material::classify_pbr` was deleted, and the
  live producer for legacy NIF content is `classify_legacy_pbr`
  (`crates/nif/src/import/mesh/`) at import time, not the core backstop
  `classify_pbr_keyword`. The sentence sits four lines below the block that
  declares "the shader is FORMAT-AGNOSTIC … Per-format branches in the shader
  were a smell we explicitly factored OUT", so the paragraph asserting the
  no-render-time-fallback rule is the paragraph breaking it.

  Two secondary reach gaps, noted for completeness rather than as separate
  findings: the gate also skips `.md`, and the recurrence history includes
  documentation (`ROADMAP.md` is discussed in `cd8691be`'s own message); and
  the directory-skip comment says `target/` and `.claude/issues/` are excluded
  while the code excludes `target` and `.git` — inert today because
  `.claude/issues/` holds no `.rs` files, but the comment does not describe
  the code.
- **Impact**: The gate's name is `no_source_file_frames_the_deleted_classify_pbr_as_live`,
  and GLSL is source in this workspace — a reader who sees the test green
  concludes the sweep is complete when the render-side instance is precisely
  the one still standing. This is the fifth instance of a class the project has
  now spent four fixes on; the fix that was supposed to end it does not reach
  the shader.
- **Related**: #3869 (closed, this is its reach gap), #1321, #1522, #1624,
  #3868 (a sibling open issue about *other* stale present-tense comments in
  `triangle.frag`), #2984 (`affected_shaders_include_constants_header` — the
  precedent for a Rust test that scans shader sources).
- **Suggested Fix**: Extend `collect_live_classify_pbr_claims`'s extension
  filter to `rs | vert | frag | comp | glsl | md`, then fix the one line it
  finds (name `classify_legacy_pbr` as the legacy producer and say the
  per-draw classifier was removed). The scan already walks the whole workspace
  tree, so this is a one-line predicate change plus the exclusions the
  directory comment already claims.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: per-game logic stays at the NIFAL parser->`Material` boundary - never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **SPIRV**: If GLSL changed, recompile the `.spv` (`cd crates/renderer/shaders && glslangValidator -V -I. <shader> -o <shader>.spv`) and commit it
- [ ] **TESTS**: A regression test pins this specific fix
