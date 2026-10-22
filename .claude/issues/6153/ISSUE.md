# #6153: TD2-2026-10-19-08: The four `lower_*_fragment` entry points still repeat the `Scope` build and the prune/never-suspend/barriers pipeline (prior-partial: TD2-2026-10-15-10 second half)
STATE: OPEN
LABELS: bug,low,tech-debt,scripting

- **Severity**: LOW
- **Dimension**: 2 - Logic Duplication
- **Location**: `crates/scripting/src/translate/effects/lower.rs:71-76,92-98,116-122,222-228` (Scope) and the `.map(prune_unread_locals).filter(|e| locals_never_suspend(e) && provider_barriers_fit(e))` tail at `:77-79,99-101,123-125,235-237`
- **Age**: M47.2, 2026-10-12..16; #5811 (c1983a81d) fixed the populate half only
- **Effort**: trivial
- **Description**: they differ in `library`, `vmad_is_quest`, and (info) the speaker-parameter seeding plus a latent-tail check; everything else is the same 4-field Scope literal and pipeline.
- **Proposed fix**: `fn lower_with(body, providers, scope: Scope) -> Option<Vec<Effect>>` in `lower.rs` that owns the pipeline, and `fn scope_from(quest_props, argless, per_instance, library) -> Scope`; each entry point then sets its two or three distinguishing fields.

Related: #5811 (closed; fixed the populate half only).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the related files named above
- [ ] **TESTS**: A regression test / source-scan guard pins this specific fix (or the finding is a pure move/delete covered by existing tests)

_Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-19.md` (HEAD 12677d718). Fix with `/fix-issue <number>`._

