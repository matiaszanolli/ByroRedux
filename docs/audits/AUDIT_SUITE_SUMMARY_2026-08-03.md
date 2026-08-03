# Audit Suite Summary — comprehensive — 2026-08-03

21 audits run against HEAD `1ae86f62` (renderer/ECS/safety/concurrency/
performance, NIF/NIFAL, audio/speedtree/scripting/save, legacy-compat/
tech-debt, all 6 per-game compat audits, regression, runtime telemetry).

**0 CRITICAL findings.** Several HIGH findings are cross-cutting — they
surfaced under one game's audit but affect shared code paths used by
multiple or all games. See "Cross-Cutting HIGH Findings" below before
reading the per-audit table; those are the highest-value results from
this sweep.

| Audit | Findings | CRITICAL | HIGH | MEDIUM | LOW | Report |
|---|---:|---:|---:|---:|---:|---|
| Renderer | 6 | 0 | 0 | 3 | 3 | AUDIT_RENDERER_2026-08-03.md |
| ECS | 0 | 0 | 0 | 0 | 0 | AUDIT_ECS_2026-08-03.md |
| Safety | 4 | 0 | 0 | 0 | 4 | AUDIT_SAFETY_2026-08-03.md |
| Concurrency | 2 | 0 | 0 | 1 | 1 | AUDIT_CONCURRENCY_2026-08-03.md |
| Performance | 5 | 0 | 1 | 2 | 2 | AUDIT_PERFORMANCE_2026-08-03.md |
| NIF | 4 | 0 | 1 | 1 | 2 | AUDIT_NIF_2026-08-03.md |
| NIFAL | 20 | 0 | 0 | 7 | 13 | AUDIT_NIFAL_2026-08-03.md |
| Audio | 0 | 0 | 0 | 0 | 0 | AUDIT_AUDIO_2026-08-03.md |
| SpeedTree | 0 | 0 | 0 | 0 | 0 | AUDIT_SPEEDTREE_2026-08-03.md |
| Scripting | 5 | 0 | 1 | 2 | 2 | AUDIT_SCRIPTING_2026-08-03.md |
| Save | 5 | 0 | 1 | 3 | 1 | AUDIT_SAVE_2026-08-03.md |
| Legacy-Compat | 0 | 0 | 0 | 0 | 0 | AUDIT_LEGACY_COMPAT_2026-08-03.md |
| Tech-Debt | 7 | 0 | 0 | 2 | 5 | AUDIT_TECH-DEBT_2026-08-03.md |
| FNV | 6 | 0 | 2 | 2 | 2 | AUDIT_FNV_2026-08-03.md |
| FO3 | 14 | 0 | 2 | 5 | 7 | AUDIT_FO3_2026-08-03.md |
| Skyrim | 9 | 0 | 1 | 3 | 5 | AUDIT_SKYRIM_2026-08-03.md |
| Oblivion | 6 | 0 | 0* | 1 | 5 | AUDIT_OBLIVION_2026-08-03.md |
| FO4 | 2 | 0 | 0 | 0 | 2 | AUDIT_FO4_2026-08-03.md |
| Starfield | 16 | 0 | 1 | 7 | 8 | AUDIT_STARFIELD_2026-08-03.md |
| Regression | 2 | 0 | 0 | 1 | 1 | AUDIT_REGRESSION_2026-08-03.md |
| Runtime | 1** | 0 | 0 | 1 | 0 | AUDIT_RUNTIME_2026-08-03.md |

**Total: 114 findings (0 critical, 10 high, 41 medium, 63 low)**

\* Oblivion has 0 *new* HIGH findings this cycle, but contributed new
negative evidence on the pre-existing open HIGH issue #2193.
\** Runtime's table counts only its 1 new MEDIUM finding; it also
corroborated (didn't re-file) two already-open issues, #2215 and #2216
— the latter now shows a materially escalated drift and is worth a look.

---

## Cross-Cutting HIGH Findings (highest priority — read these first)

These surfaced during a single game's audit but touch code shared across
most or all games, so their blast radius is larger than the report they
live in suggests:

1. **FNV-D7-01 / PHYSAL ragdoll transform composed twice** (`AUDIT_FNV_2026-08-03.md`)
   — `activate_ragdoll` seeds every ragdoll body by composing the bone's
   world transform with the imported body transform, but the body
   transform is *already* in skeleton-root space (confirmed empirically
   across FNV, Oblivion, and Skyrim SE skeletons). Every ragdoll the
   engine builds today is seeded in the wrong place. The `#1616`
   round-trip test can't catch it — it round-trips the same wrong offset.

2. **FO3-D5-01 / non-T `bhkRigidBody` collider displacement** (`AUDIT_FO3_2026-08-03.md`)
   — a prior audit dismissed the `bhkRigidBody` vs `bhkRigidBodyT`
   distinction as harmless; this cycle measured real FO3 data and found
   2,701 non-T bodies (9.5% of the mesh corpus, 1,007 of them `FIXED`
   walkable architecture) carry a non-identity translation the importer
   wrongly applies, median displacement 160 engine units. Shared code
   path (`rigid_body.rs`) — affects Oblivion/FNV/Skyrim too.

3. **SF-D8-01 / fabricated PBR scalars on Effect/Sky/Water shader arms** (`AUDIT_STARFIELD_2026-08-03.md`)
   — `BSEffectShaderProperty`/`BSSkyShaderProperty`/`BSWaterShaderProperty`
   walker arms set `has_material_data = true` without ever authoring
   `specular_color`, so the PBR classifier fabricates
   metalness/roughness off an unauthored struct default. Extends the
   #1873 chrome-flyer bug (previously fixed for only one arm) to three
   more, cross-game (Skyrim/FO4/FO76/Starfield).

4. **SK-D1-01 / all Skyrim SE NPCs render headless** (`AUDIT_SKYRIM_2026-08-03.md`)
   — every `BSDynamicTriShape` (FaceGen head/eye/brow/mouth geometry)
   imports to zero meshes on Skyrim SE, confirmed against real vanilla
   NPC head records. Skyrim-specific (FO4/Starfield FaceGen uses a
   different code path, confirmed not to share this bug).

5. **FO3-D1-01 / ungated `env_map_scale` forwarding** (`AUDIT_FO3_2026-08-03.md`)
   — FO3/FNV's PBR classifier reads `env_map_scale` with no flag gate,
   so most `BSShaderPPLightingProperty` meshes with a co-bound
   `NiMaterialProperty` get invented metalness/roughness — same failure
   class as #1873, reached through a different, still-open input path.

## Other notable findings

- **PERF-D7-01 (HIGH)** — exterior persistent-cell load bypasses the new
  resumable/budgeted streaming architecture (`AUDIT_PERFORMANCE_2026-08-03.md`).
- **NIF-D2-01 (HIGH)** — `NiMaterialProperty`'s Bethesda-compact
  ambient/diffuse default is `0.5` where nif.xml specifies `1.0`,
  systemically darkening FO3/FNV content by ~50% (`AUDIT_NIF_2026-08-03.md`).
- **Scripting HIGH** — `SetMotionType` mis-maps Havok motion-type values,
  reintroducing the #1652 bug pattern in a new module (`AUDIT_SCRIPTING_2026-08-03.md`).
- **Save HIGH** — the new M47.3 scripting subsystem's lever/switch state
  (`TwoStateActivator`) was never wired into `build_save_registry` and
  silently reverts on save/load (`AUDIT_SAVE_2026-08-03.md`).
- **FNV-D8-01 (HIGH)** — `--grid 0,0` worldspace auto-pick is
  non-deterministic and can silently load the wrong worldspace, including
  one with no usable collision (`AUDIT_FNV_2026-08-03.md`).
- **Today's own commit introduced a regression** — `7bb517b2` (#2258 split)
  reintroduced 9 undocumented `unsafe` blocks, a third occurrence of the
  #1904→#2131 bug class; confirmed live via `cargo clippy -D warnings`
  (`AUDIT_REGRESSION_2026-08-03.md`, MEDIUM).

## Process note

Several per-audit orchestrator agents in this run internally fan out into
many per-dimension sub-agents, but (in this environment) a subagent that
spawns its own background children has no way to retrieve their results —
only the top-level session receives completion notifications. Multiple
audits (FNV, FO3, Skyrim, FO4, Oblivion, Starfield) initially returned
"no findings" for a dimension whose deeper sub-agent was, in fact, still
producing real results in the background. Each case was caught by
checking the dimension's scratch file under `/tmp/audit/<game>/dim_N.md`
against the written report and, where a real finding had been dropped,
patching the report directly before finalizing this summary. The five
cross-cutting HIGH findings above were all recovered this way.

## Suggested next steps

For each report with findings, file them as GitHub issues:

```
/audit-publish docs/audits/AUDIT_RENDERER_2026-08-03.md
/audit-publish docs/audits/AUDIT_CONCURRENCY_2026-08-03.md
/audit-publish docs/audits/AUDIT_PERFORMANCE_2026-08-03.md
/audit-publish docs/audits/AUDIT_NIF_2026-08-03.md
/audit-publish docs/audits/AUDIT_NIFAL_2026-08-03.md
/audit-publish docs/audits/AUDIT_SCRIPTING_2026-08-03.md
/audit-publish docs/audits/AUDIT_SAVE_2026-08-03.md
/audit-publish docs/audits/AUDIT_TECH-DEBT_2026-08-03.md
/audit-publish docs/audits/AUDIT_FNV_2026-08-03.md
/audit-publish docs/audits/AUDIT_FO3_2026-08-03.md
/audit-publish docs/audits/AUDIT_SKYRIM_2026-08-03.md
/audit-publish docs/audits/AUDIT_OBLIVION_2026-08-03.md
/audit-publish docs/audits/AUDIT_FO4_2026-08-03.md
/audit-publish docs/audits/AUDIT_STARFIELD_2026-08-03.md
/audit-publish docs/audits/AUDIT_REGRESSION_2026-08-03.md
/audit-publish docs/audits/AUDIT_RUNTIME_2026-08-03.md
```

Given the cross-cutting nature of findings 1, 2, 3, and 5 above, consider
triaging PHYSAL ragdoll (#1) and the PBR-fabrication family (#3, #5,
and the underlying #1873 pattern) as single issues with a shared root
cause rather than one issue per game.
