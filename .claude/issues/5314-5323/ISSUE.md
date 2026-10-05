# Batch 5314–5323 (fetched 2026-10-05, all OPEN)

Source: docs/audits/AUDIT_PARSERS_2026-10-05.md, AUDIT_TECH_DEBT_2026-10-05.md, AUDIT_PAPYRUS_2026-10-05.md (audit HEAD a2c24b16e).

| # | Title (short) | Sev | Domain | Crate / target |
|---|---|---|---|---|
| 5314 | MenuXml include budget counts splices not bytes → 7.2 MiB/KiB linear blow-up, OOM at --hud | HIGH | menuxml | byroredux-menuxml |
| 5315 | draw.rs:1304-1305 orphaned truncated doc fragment above draw_frame_size_budget_tests | LOW | renderer doc | byroredux-renderer |
| 5316 | hkx MAX_TRANSFORM_SAMPLES=16M is 128× vanilla max (124,821); 279 KB clip → 597 MiB decode | MEDIUM | hkx | byroredux-hkx |
| 5317 | feature-matrix.md gameplay table pre-slice-closure; no P3–P5 rows; slice doc self-contradicts on P1 | LOW | docs | docs-only |
| 5318 | game-loop.md schedule table misses loading_model_turntable_system; footstep row pre-#5146 | LOW | docs | docs-only |
| 5319 | cdb_material_index `.ok()?` swallows archive errors, memoises None silently; log strings lost `\` continuation (22 spaces) | LOW | binary/asset_provider | byroredux (bin — 1.96 toolchain) |
| 5320 | sfmaterial MaterialIndex::build: no-DBFileIndex → Ok empty; rows/instance mismatch unchecked; no trailing-bytes check in stream_db_file_index; contextless dup-index error | LOW | sfmaterial | byroredux-sfmaterial |
| 5321 | .claude/commands/audit-tech-debt/SKILL.md:147-151 stale: gpu_material_size_claims widened by #5203 to all five pinned Gpu* structs | LOW | skill doc | docs-only |
| 5322 | papyrus expect_eol never errors; parse_expr ignores trailing input; no `Is` token → `If f is Actor` silently becomes `If f` + VarDecl named Actor | MEDIUM | papyrus | byroredux-papyrus |
| 5323 | sfmaterial skip_user_class_body walks field_layout (declaration order) after #3398 moved read path to read_order | LOW | sfmaterial | byroredux-sfmaterial |

Grouping:
- sfmaterial pair: #5320 + #5323 (same crate, related code paths — but separate commits)
- doc-only trio: #5317, #5318, #5321 (no cargo tests)
- code singles: #5314 (menuxml), #5315 (renderer, doc-only delete), #5316 (hkx), #5319 (bin crate — needs `rustup which --toolchain 1.96.0 cargo`), #5322 (papyrus)
