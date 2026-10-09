# #5466: SF-2026-10-08-D3-02: Phase-1 and "no `.mat` resolver" text survives Phase 2 and #4277, including an INFO log printed every session, and `apply_loose_mat` was inserted under the `single_boundary_tests` doc comment

**Labels**: low,import-pipeline,documentation,doc-rot,game:starfield,legacy-compat,game:fo4,nifal
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5466

**Source**: `docs/audits/AUDIT_STARFIELD_2026-10-08.md` — `SF-2026-10-08-D3-02` (HEAD `00f580e09`)

**Publish note**: Validated at HEAD: `cdb.rs` INFO log at ~:280 still says "Phase 1 — full parse + per-field extraction is the deferred Phase 2 follow-up"; `merge.rs` still carries "no JSON `.mat` resolver" (~:439) and "not yet parsed (tracked in #4277)" (~:576); the #2412/#3857 doc block (:1664) still precedes `fn apply_loose_mat` (:1683) rather than `mod single_boundary_tests` (:1722).

- **Severity**: LOW
- **Dimension**: CDB Material Database (doc rot)
- **Location**:
  - `byroredux/src/asset_provider/material/cdb.rs:7-8` (module doc: "until the full parse lands");
  - `cdb.rs:277-281` (`probe_starfield_cdb` INFO log: "Phase 1 — full parse + per-field extraction is the deferred Phase 2 follow-up");
  - `cdb.rs:346-350` ("there is no resolver for a `.mat` path to miss");
  - `merge.rs:436-442` ("no JSON `.mat` resolver exists yet");
  - `merge.rs:574-577` ("The .mat format is not yet parsed (tracked in #4277)", which is closed);
  - `merge.rs:1664-1683` (the #2412/#3857 doc block for `mod single_boundary_tests` now precedes `fn apply_loose_mat`, so rustdoc attaches it there and the test module loses its header).
- **Status**: NEW. #5210 covers only the separate `merge_external_material` "forwards no authored field … Phase 2 should return `Merged`" comment and nifal.md. These sites were written stale by `224a19372` / `c2f28e06c`.
- **Description / Evidence**: As listed. The INFO line is runtime output. It is emitted once per discovered CDB on every session, and it tells an operator that per-field extraction does not exist while `MaterialIndex` is translating roles.
- **Impact**: Misleading logs and docs. No runtime effect.
- **Related**: #5210, NIFAL-D8-2026-10-08-01 (the loose `.mat` module doc's false "zero files" claim, a separate site), #3398, #4277.
- **Suggested Fix**: Reword the log to "index built lazily on first lookup". Drop the "no resolver" and "not yet parsed" claims. Move `apply_loose_mat` above the `#2412 / #3857` doc block so the block sits directly on `mod single_boundary_tests`.

## Also reported as `FO4-2026-10-08-D2-01` (AUDIT_FO4_2026-10-08.md)

Cross-report duplicate merged at publish time; the sibling report's text follows.

**Source**: `docs/audits/AUDIT_FO4_2026-10-08.md` — `FO4-2026-10-08-D2-01` (HEAD `00f580e09`)

- **Severity**: LOW (doc rot).
- **Dimension**: BGSM/BGEM merge, in `merge.rs`. The loose `.mat` content is owned by `/audit-nifal`.
- **Location**: `byroredux/src/asset_provider/material/merge.rs:1664-1686` (the doc block, now attached to
  `fn apply_loose_mat` at `:1683`) and `:1721-1722` (`#[cfg(test)] mod single_boundary_tests`, which lost its doc).
- **Status**: NEW. NIFAL-2026-10-08 lists other stale comments in the loose-`.mat` arm (`merge.rs:436-440` and
  `:573-575`) but not this one.
- **Description**: c2f28e06c (#4277) put the new private fn directly after the outer `///` block that explains why
  `single_boundary_tests` exists. That block says #2412 recommended no split, #3857 split anyway, and "visibility is
  exactly what a behavioural test cannot see". Rust attaches it to `apply_loose_mat`, so its rendered doc runs that
  rationale straight into "#4277 — merge the loose `.mat` file's bytes …". The test module that encodes the NIFAL
  single-boundary invariant now carries no rationale.
- **Evidence**: `merge.rs:1664` (`/// #2412 / #3857 — pin the invariant the split had to preserve.`) runs unbroken to
  `:1682` (`/// \`parse_loose_mat\`'s; this fn owns the boundary-side application.`) and then `:1683 fn apply_loose_mat(`.
  `mod single_boundary_tests` starts at `:1722`.
- **Impact**: This is a readability and maintainability issue only. The doc that defends the merge boundary is the
  first thing `/audit-nifal` reads, per the `asset_provider/material/mod.rs` module doc.
- **Related**: #2412, #3857, #4277, and the NIFAL-2026-10-08 loose-`.mat` findings.
- **Suggested Fix**: Move `apply_loose_mat` and its own 4-line doc above the #2412/#3857 block, so the block sits
  directly on `#[cfg(test)] mod single_boundary_tests` again.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (`docs/engine/nifal.md` Starfield section; #5210 sites)
