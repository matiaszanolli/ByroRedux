# TD3-2026-10-19-07: 18 `file.rs:LINE` references point past the end of their (now split) target

Labels: low,documentation,doc-rot,tech-debt
Source: docs/audits/AUDIT_TECH_DEBT_2026-10-19.md

- Severity: LOW · Effort: small · Status: NEW (the split wave; `components.rs` 89 lines, `cell_loader.rs` 643, `save_io.rs` 107, `condition.rs` 224, `records/mod.rs` 169, `cell/mod.rs` 198, `material/walker.rs` 194)
- Evidence (out-of-range detector, `git ls-files` x regex, unique basenames only; in-range-but-wrong is invisible, so this is a floor):
  - docs: `dependency-update-followups-plan.md:405` (`components.rs:1241`), `exterior-grid-streaming.md:199` (`components.rs:968`; also `:174` `components.rs:60-73`), `exal-groundcover.md:102` (`components.rs#L914`), `m47-3-quest-alias-design.md:55` (`condition.rs:326`; fn now `condition/eval.rs:171`), `npc-spawn-ai-packages.md:129` (`records/mod.rs:587`), `per-game-translation-survey.md:142,177` (`walker.rs:172-257`, `:331-345`), `save-load-roundtrip.md:35,47` (`save_io.rs:783`, `:223`), `exterior-readiness-plan.md:898` (`cell/mod.rs:1271-1316`), `sdk-v0.1-next-action-plan.md:230` (`compatibility.rs:4252`).
  - source comments: `parsed_nif_cache.rs:70` (`cell_loader.rs:1322`), `variant_pack_gating_tests.rs:224` (`render.rs:603`), `nif/import/material/legacy_properties.rs:308-309` (`render.rs:573`, `cell_loader.rs:2463`), `texture_slot_3_4_5_tests.rs:1653`, `plugin/src/esm/cell/wrld.rs:14` (`cell_loader.rs:778`), `renderer/src/texture_registry_tests.rs:60` (`cell_loader.rs:945`), `plugin/tests/parse_real_esm.rs:1619`. Also `render/mod.rs:588,618` and `scene/procedural_fallback_tests.rs:6` cite `systems.rs:1446/1437-1442/1136` (`systems.rs` is 88 lines; `weather_system` is `systems/weather.rs:789`).
- Proposed fix: replace each with a symbol name (`cell_loader::...::fn`) - line numbers into split files will rot again. A hygiene test over `\.rs:\d+` against `wc -l` (my 40-line script) would hold the class.

## Completeness Checks
- [ ] **SIBLING**: Same drift checked in the neighbouring docs / skills
- [ ] `.claude/commands/_audit-validate.sh` still passes after the edit

_Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-19.md` (HEAD 12677d718). Fix with `/fix-issue <number>`._
