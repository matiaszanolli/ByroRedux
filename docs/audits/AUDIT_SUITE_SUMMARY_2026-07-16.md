# Audit Suite Summary — comprehensive — 2026-07-16

| Audit | Findings | CRITICAL | HIGH | MEDIUM | LOW | Report |
|-------|----------|----------|------|--------|-----|--------|
| Renderer | 0 | 0 | 0 | 0 | 0 | AUDIT_RENDERER_2026-07-16.md |
| ECS | 1 | 0 | 0 | 0 | 1 | AUDIT_ECS_2026-07-16.md |
| Safety | 0 | 0 | 0 | 0 | 0 | AUDIT_SAFETY_2026-07-16.md |
| Concurrency | 0 | 0 | 0 | 0 | 0 | AUDIT_CONCURRENCY_2026-07-16.md |
| Performance | 15 | 0 | 0 | 4 | 11 | AUDIT_PERFORMANCE_2026-07-16.md |
| NIF | 10 | 0 | 3 | 4 | 3 | AUDIT_NIF_2026-07-16.md |
| NIFAL | 2 | 0 | 1 | 1 | 0 | AUDIT_NIFAL_2026-07-16.md |
| Audio | 0 | 0 | 0 | 0 | 0 | AUDIT_AUDIO_2026-07-16.md |
| SpeedTree | 0 | 0 | 0 | 0 | 0 | AUDIT_SPEEDTREE_2026-07-16.md |
| Scripting | 7 | 0 | 1 | 3 | 3 | AUDIT_SCRIPTING_2026-07-16.md |
| Save | 9 | 0 | 2 | 4 | 3 | AUDIT_SAVE_2026-07-16.md |
| Legacy-Compat | 1 | 0 | 0 | 1 | 0 | AUDIT_LEGACY-COMPAT_2026-07-16.md |
| Tech-Debt | 40 | 0 | 2 | 6 | 32 | AUDIT_TECH-DEBT_2026-07-16.md |
| FNV | 9 | 0 | 2 | 3 | 4 | AUDIT_FNV_2026-07-16.md |
| FO3 | 3 | 0 | 0 | 1 | 2 | AUDIT_FO3_2026-07-16.md |
| Skyrim | 6 | 0 | 1 | 2 | 3 | AUDIT_SKYRIM_2026-07-16.md |
| Oblivion | 3 | 0 | 0 | 0 | 3 | AUDIT_OBLIVION_2026-07-16.md |
| FO4 | 1 | 0 | 0 | 0 | 1 | AUDIT_FO4_2026-07-16.md |
| Starfield | 14 | 0 | 1 | 4 | 9 | AUDIT_STARFIELD_2026-07-16.md |
| Regression | — | — | — | — | — | AUDIT_REGRESSION_2026-07-16.md (33 + 5/5 PASS, 10 PARTIAL, 0 FAIL, 0 UNVERIFIABLE — verification pass, not a findings audit) |
| Runtime | 1 | 0 | 1 | 0 | 0 | AUDIT_RUNTIME_2026-07-16.md |

**Total: 122 findings (0 critical, 14 high, 33 medium, 75 low)**

No CRITICAL findings anywhere in the sweep.

## Highest-value findings (HIGH severity or otherwise notable)

- **Runtime (HIGH)** — the player character never grounds at spawn in either TES-family cell (Oblivion, Skyrim) — infinite freefall — while all three Fallout-family cells ground normally. Matches a symptom closed issue #1832 flagged as unresolved but never separately filed.
- **Starfield (HIGH)** — `numeric_sibling_paths` doesn't recognize Starfield's zero-padded `Meshes01`/`Meshes02` archive series as a sibling group, silently dropping ~18% of sub-meshes on single-LOD-slot content (weapons, ship modules) under the project's own documented launch command.
- **Skyrim (HIGH)** — the Skyrim+ prebaked NPC spawn path has no body-mesh fallback (FaceGeom NIFs are head-only, RACE skin WNAM never parsed) — at least 2 of 6 control-bench named NPCs resolve to feet-only equipment coverage.
- **FNV (2× HIGH)** — the multi-plugin FormID-remap fix (closed #1996) covered `parse_npc`'s classic fields but missed three sibling parsers (OTFT/LVLI-LVLN/CONT) and `parse_npc`'s own FaceGen fields — DLC NPCs whose gear/hair/eyes are defined in the same DLC plugin can silently spawn naked/bald.
- **Save (2× HIGH)** — seven M42 AI-package runtime-state components (Wander/Travel/Follow/Escort/Guard/Patrol/Sandbox) are unregistered for save/load, so terminal completion markers are silently lost on save/load (opt-in flags only, but a real non-recoverable regression once set). Also, the `#1714` regression-guard's file-scan list wasn't updated when `ActorValues` was registered, leaving a latent hole in the "no silent `serde(default)`" safety net.
- **NIF (2× HIGH)** — two Havok/morph parsers (`BhkSimpleShapePhantom`/`BhkAabbPhantom`, `NiGeomMorpherController`) are missing the no-`block_sizes`-era version gate that sibling parsers in the same file already received.
- **NIFAL (HIGH)** — `classify_pbr_keyword`'s glass-classification arm matches bare `"ice"` via unbounded substring, misclassifying paths like `office`/`notice`/`device`/`justice` as glass — same root-cause class as a prior closed issue that patched only one symptom, missed on the shared classifier for six prior sweeps.
- **Scripting (HIGH)** — `SetObjectiveDisplayed/Completed/Failed` repeat a bool-arg coercion bug just fixed elsewhere in a sibling table; now live and reachable on the fully-wired QUST fragment path (both live-dispatched and save-persisted).
- **Tech-Debt (2× HIGH)** — `audit-scripting`'s own SKILL.md has a stale dedup baseline that actively misdirects future scripting audits (claims no prior audit exists when 6 do, preloads 7 "known open" issues that are all closed); `triangle.frag` hand-writes shader constants that bypass the generated-header lockstep guard.

Full detail, evidence, and suggested fixes for every finding are in the per-domain reports listed above.

## Process note

Two dimension-group sub-agents (renderer Dim 1-3 and Dim 4-7) and one performance sub-agent (Dim 6) stalled on their first run with no completion notification ever received. Both were identified, abandoned, and re-run successfully before the final merge — no data was lost, but it's worth knowing the fan-out pattern used by several of these audit skills (renderer, performance, NIF, NIFAL, scripting, save, tech-debt, per-game audits) isn't fully reliable at this scale and may need a retry/timeout in future automated runs.
