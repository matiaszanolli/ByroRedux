# Incremental / Delta Audit — 2026-05-26 (`--commits 5`)

**Scope**: 5 most-recent commits, `HEAD~5..HEAD` (`416a8df0` → `f6c23df5`). Same-session as the underlying fixes — this is a follow-up sanity sweep, not a discovery audit.

## Change Summary

```
f6c23df5 docs(reference): refresh audit-skill flag names and ROADMAP ESM stats
d11704da docs(bloom): document upsample DC-gain absorption and dead-Option guard
19d93a5b docs(audit): renderer Dim 11 (TAA) + Dim 16 (tangent-space) reports
b3adbf11 fix(shader): Gram-Schmidt orthogonalization in anisotropic-GGX TBN
416a8df0 fix(esm): extract SCRI attached-script FormID from NPC_ and CREA
```

**21 files / +590 / -17 lines.** Of those:

- **2 functional code changes** (`b3adbf11` GLSL + `416a8df0` Rust)
- **3 comment / docstring changes** (`d11704da` × 2 sites + `b3adbf11`'s SCRI doc on the new field)
- **8 doc-only / reference updates** (`f6c23df5` audit-skill flag-rename × 7 files + ROADMAP.md × 1)
- **5 audit reports + 3 issue snapshots** (`19d93a5b` + part of `d11704da`/`b3adbf11` close-out)
- **1 SPIR-V re-emit** (`triangle.frag.spv`, 128968 → 129288 bytes, expected for the GLSL change)

## High-Risk Changes

| File | Commit | Domain | Lines |
|---|---|---|---|
| `crates/renderer/shaders/triangle.frag` | `b3adbf11` | Shaders | +10 |
| `crates/renderer/shaders/triangle.frag.spv` | `b3adbf11` | Shaders (binary) | recompile |
| `crates/plugin/src/esm/records/actor.rs` | `416a8df0` | ESM Parser | +62 |
| `crates/plugin/src/esm/records/index.rs` | `416a8df0` | ESM Parser | +60 / -4 |
| `crates/plugin/src/equip.rs` | `416a8df0` | ESM Parser | +1 (field-add cascade) |
| `crates/plugin/src/esm/records/tests.rs` | `416a8df0` | ESM Parser | +1 (field-add cascade) |

## Risk-Categorized Review

### Shader change — `b3adbf11` (Gram-Schmidt at anisotropic-GGX TBN)

- **Sites**: [triangle.frag:2500-2505](crates/renderer/shaders/triangle.frag#L2500-L2505) + [:2745-2750](crates/renderer/shaders/triangle.frag#L2745-L2750)
- **Added**: `T = normalize(T - dot(T, N) * N);` between the existing `T = normalize(fragTangent.xyz)` and `B = normalize(cross(N, T)) * fragTangent.w` lines.
- **Sibling check**: ran `grep "fragTangent.xyz\|normalize(cross(N, T)) \* fragTangent.w"` against `triangle.frag`. **Both** anisotropic sites covered (`:2500` + `:2745`); the visualization site at `:1255` reads `fragTangent.xyz` but does not reconstruct a TBN; `perturbNormal` Path-1 already had Gram-Schmidt pre-fix. **No missed siblings.**
- **Latency**: behavior is identical for all current content (`mat.anisotropic == 0` on every legacy NIF). The path only fires on BGSM v22+ / synthetic hair-card authoring, neither of which ships today.
- **Verification**: 289/289 renderer lib tests pass; `cargo check` clean; SPIR-V re-emit landed atomically with the GLSL change.

### ESM change — `416a8df0` (NPC/CREA SCRI extraction)

- **Struct change**: `NpcRecord` gains `pub script_form_id: u32` at [actor.rs:208](crates/plugin/src/esm/records/actor.rs#L208) (default `0`, the "no script" sentinel).
- **Parser arm**: `b"SCRI" if sub.data.len() >= 4` reads via `SubReader::new(&sub.data).u32_or_default()`.
- **Index lookup**: [`base_record_script`](crates/plugin/src/esm/records/index.rs#L516) extended to walk `npcs` + `creatures` maps (CREA shares `parse_npc` per `records/mod.rs:381`, so a single arm covers both).
- **Sibling check**: ran `grep "NpcRecord {"` against `crates/` and `byroredux/`. **3 construction sites total**: `actor.rs:473` (parser, has new field), `equip.rs:782` (test helper, patched), `tests.rs:729` (test helper, patched). All covered by the field-add cascade in `416a8df0`.
- **Backward compat**: `base_record_script`'s extension is purely additive — pre-fix the NPC/CREA path returned `None`, post-fix it returns `Some(form_id)` when SCRI present. Any caller expecting `None` for an NPC base form will now correctly see a script reference. No existing test depends on the old behaviour (verified: 416/416 plugin lib tests pass post-fix).
- **Verification**: 4 new regression tests cover NPC SCRI, CREA SCRI, short-SCRI guard, index walk for both bins.

### Comment-only changes (`d11704da`)

- [bloom_upsample.comp:13-25](crates/renderer/shaders/bloom_upsample.comp#L13-L25) — 12 lines documenting the intentional ≥1.0 DC gain absorbed by `BLOOM_INTENSITY = 0.15`. No SPIR-V delta (glslang strips comments — confirmed: `bloom_upsample.comp.spv` unchanged).
- [draw.rs:2840](crates/renderer/src/vulkan/context/draw.rs#L2840) — 10-line crumb cross-referencing #1276 + #1081 + `context/mod.rs:1958` flagging the dead `Option<BloomPipeline>` `None` branch.

No behavioural impact. No regression risk.

### Doc / reference updates (`f6c23df5`, `19d93a5b`)

- 7 audit-skill prompts: `MAT_FLAG_BGSM_*` → current names (`PBR_BSDF`, `TRANSLUCENCY[_*]`, `MODEL_SPACE_NORMALS`). Audit-renderer.md additionally corrected an entry-point claim about where the `#define`s live (they're in `triangle.frag:183-187`, not `shader_constants_data.rs`).
- ROADMAP.md: FO3/FNV ESM record counts refreshed against today's `#1272` NAVM gains (FO3 `31 101 → 44 657`; FNV `73 054 → 77 825`). "Unreverified" caveat dropped.
- Audit reports + issue snapshots (`AUDIT_RENDERER_2026-05-26_DIM{11,16}.md`, `.claude/issues/127{4,5,6}/ISSUE.md`).

No code change → no regression surface.

## Findings

**Zero NEW findings.** All material in the 5-commit delta was audited as it landed; today's audit reports document the per-commit verification (Dim 11 TAA delta-clean, Dim 16 tangent-space all 4 prior findings FIXED-VERIFIED + 1 NEW that became #1274 + fixed, Dim 19 bloom 2 LOW filed + fixed). The incremental sweep confirms no missed siblings:

- `NpcRecord` literal sites: 3 total, all patched.
- `triangle.frag` TBN reconstruction sites: 4 total (perturbNormal Path-1, Path-2 RT-side, anisotropic fallback-directional, anisotropic per-light) — the 2 anisotropic sites are the only ones missing Gram-Schmidt pre-fix; both patched.
- `base_record_script` extension is additive; no regression in pre-existing call sites.

## Missing Tests

None. The two functional commits added their own regression coverage:

- `416a8df0`: 4 new tests (`npc_extracts_scri_attached_script`, `crea_extracts_scri_attached_script`, `npc_short_scri_is_ignored`, `base_record_script_finds_npc_and_creature_scripts`).
- `b3adbf11`: no test added — note in the issue close comment is explicit: no real-content fixture authors anisotropy today, so a regression test would require manufacturing synthetic content. Acceptable per the latency framing.

## Tech-Debt Observation (out-of-scope for this delta but worth noting)

The audit-renderer.md correction surfaced that `MAT_FLAG_*` bits 5-9 (PBR_BSDF, TRANSLUCENCY, MODEL_SPACE_NORMALS, TRANSLUCENCY_THICK_OBJECT, TRANSLUCENCY_MIX_ALBEDO) live as bare `#define`s in [triangle.frag:183-187](crates/renderer/shaders/triangle.frag#L183-L187) instead of going through `shader_constants_data.rs` (the generated-header lockstep contract from #1190 / TD4-NEW-01 covers only bits 0-4). Pre-existed the 5 commits — not a regression. Worth a tech-debt audit pickup if the project wants the contract symmetric across all flag bits.

## Methodology

1. `git diff HEAD~5..HEAD --name-only` to enumerate the 21 changed files.
2. Risk-categorize per the table in the orchestrator spec.
3. Sibling-check each functional change against potentially-related sites (`grep "NpcRecord {"`, `grep "fragTangent.xyz"`).
4. Cross-check test coverage against commits.
5. Verify `cargo check` + the relevant test targets ran clean during the underlying fixes (logged at each commit time).

---

Suggest: nothing to publish — `0 findings`. This report serves as the "delta verified clean" anchor for the 5-commit range.
