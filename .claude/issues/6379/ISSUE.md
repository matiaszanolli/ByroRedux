# TD3-2026-10-25-08: doc prose names functions and types renamed or deleted (current-tense)

Labels: low, doc-rot, tech-debt, documentation
Issue: https://github.com/matiaszanolli/ByroRedux/issues/6379
(Immutable snapshot of the issue as filed; GitHub is authoritative for state.)

- **Severity**: LOW
- **Dimension**: 3
- **Effort**: trivial
- **Sites** (all verified with `git log -S` and a HEAD grep):
  - `docs/engine/archives.md:266` and `:454` name `linear_size_for()`. It is now the private `pitch_or_linear_size_for` (`crates/bsa/src/ba2.rs:1187`; renamed by `ec63d2636`, 2026-09-28).
  - `docs/engine/save-load-roundtrip.md:95` names `SaveLoadNotifications`. It was replaced by `PlayerNotifications` (`byroredux/src/notifications`; last gone from `.rs` at `ae25ade38`, 2026-09-16).
  - `docs/engine/exal-groundcover.md:673-683` says GRAS "translates to species at the EXAL boundary (`species_from_gras` / `authored_species` in `groundcover_translate.rs`)... 138 of the 168 vanilla records... become real species". `f8a900b3c` (2026-09-13, "stop translating GRAS records into blade species") deleted both functions; `groundcover_translate.rs` now has `resolve_authored_cover` / `authored_cover_record`. Re-describe the current path.
  - Tracked elsewhere, still live: `CellRootRefIndex` / `persistent_ref_index` in `exterior-readiness-plan.md` (#6171) and the `active_*_location/_target` selectors in `npc-spawn-ai-packages.md` (#6169).
- **Fix**: Rename in prose. For exal-groundcover, rewrite the paragraph to the current decode-only behaviour.

---
Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-25.md` (HEAD c7463c04d), finding `TD3-2026-10-25-08`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / crates
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)
