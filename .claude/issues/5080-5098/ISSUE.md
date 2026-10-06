# Fix-issue batch 5080–5098 (fetched 2026-10-06)

| # | Sev | Crate(s) | Summary |
|---|-----|----------|---------|
| 5080 | LOW doc | docs | FO76 98.18%→100% (102,968, audit 2026-09-29) in ROADMAP + game-compatibility.md; add Skyrim LE row (22,466 @100%) + fix FO4 row in nif-parser.md table |
| 5081 | LOW doc | nif doc | 2 docs still say SF BSGeometry streams "decoded Y-up" — tangent.rs `synthesize_tangents_yup` doc + per-game-translation-survey.md |
| 5082 | LOW doc | nif doc | effect.rs stub-gate comment "FO76 (152..171)" wrong — gate is bsver >= FO76 (155); >= STARFIELD (172) takes !name.is_empty() |
| 5083 | MED process | gh | ~30 fixed issues left OPEN by mis-titled commits (e.g. 2f8538334 grab-bag) — verify-at-HEAD + close sweep |
| 5092 | LOW td | byroredux | split streaming.rs (2069 prod_loc): telemetry.rs + pre_parse.rs, state stays |
| 5093 | LOW td | plugin | split esm/records/actor/mod.rs (2120): npc.rs/race.rs/class.rs/faction.rs + re-exports |
| 5094 | LOW td | renderer | split volumetrics.rs (2391, regression of #2256): fog_clusters.rs + combustion.rs; repoint include_str! scans in caustic.rs, svgf.rs, context/draw.rs |
| 5095 | MED | byroredux | Skyrim 3rd-person player headless: prebaked FaceGen miss skips head entirely; docs claim "race-default head" — fix behavior or docs + smoke assertion |
| 5096 | LOW td | bin+renderer | per-frame fns regrew (about_to_wait 969, record_geometry_pass 763, render_one_frame 656, merge_bgsm_arm 669); only draw_frame has budget |
| 5098 | LOW doc | docs | Skyrim SE 33,424/7 → 33,468/8 archives in ROADMAP matrix + stats + game-compatibility.md; add LE row |

Order: 5080/5098/5081/5082 (docs; verify current state first — may be half-stale) → 5095 (code) → 5092/5093/5094 (splits) → 5096 (budgets) → 5083 (issue sweep).
