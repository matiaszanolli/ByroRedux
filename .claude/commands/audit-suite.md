---
description: "Run a preset suite of audits in parallel"
argument-hint: "--preset <name>"
---

# Audit Suite Orchestrator

## Presets

### `--preset quick`
Fast sanity check after changes (< 10 min):
1. `/audit-incremental --commits 5`

### `--preset pre-release`
Run before tagging a release:
1. `/audit-safety`
2. `/audit-renderer`
3. `/audit-ecs`
4. `/audit-regression --limit 20`

### `--preset nif-deep`
After NIF parser changes (N23 work):
1. `/audit-nif --game fnv`
2. `/audit-safety`
3. `/audit-incremental --commits 10`

### `--preset renderer-deep`
After significant renderer changes:
1. `/audit-renderer`              # all 15 dimensions
2. `/audit-performance --focus 1,2,3,7,8`
3. `/audit-concurrency --focus 2,3,5`
4. `/audit-safety`

### `--preset rt-deep`
After ray tracing / denoiser / G-buffer changes:
1. `/audit-renderer --focus 8,9,10`
2. `/audit-performance --focus 1,2`
3. `/audit-concurrency --focus 2,3,5`

### `--preset taa-deep`
After TAA / denoiser / motion-vector changes (M37.5):
1. `/audit-renderer --focus 10,11`
2. `/audit-concurrency --focus 2,5`
3. `/audit-safety` (focus on §3 memory, §7 new compute, §6 RT)

### `--preset skin-deep`
After GPU skinning / BLAS refit changes (M29.5 / M29.3):
1. `/audit-renderer --focus 8,12`
2. `/audit-performance --focus 1,7`
3. `/audit-concurrency --focus 2,3,5`
4. `/audit-safety`

### `--preset material-deep`
After R1 material table changes (GpuMaterial layout, dedup, SSBO):
1. `/audit-renderer --focus 6,14`
2. `/audit-performance --focus 8`
3. `/audit-safety` (focus on §8 R1 invariants)

### `--preset sky-weather-deep`
After M33 / M33.1 / M34 sky / weather / exterior lighting changes:
1. `/audit-renderer --focus 9,15`
2. `/audit-incremental --commits 10`

### `--preset streaming-deep`
After M40 world streaming / M41 NPC spawn changes:
1. `/audit-performance --focus 9`
2. `/audit-concurrency --focus 6`
3. `/audit-safety`

### `--preset audio-deep`
After M44 audio (kira backend) changes — emitter/listener pose sync, spatial sub-track lifecycle, reverb send, streaming music:
1. `/audit-audio`
2. `/audit-concurrency --focus 4,6`
3. `/audit-safety`

### `--preset normals-deep`
After M-NORMALS / tangent-space changes (Sessions 26–29):
1. `/audit-renderer --focus 6,16`
2. `/audit-nif --focus 1,4`
3. `/audit-safety`

### `--preset glass-deep`
After IOR refraction / glass-passthrough / Frisvad changes (Sessions 27–29):
1. `/audit-renderer --focus 9,10`
2. `/audit-safety`

### `--preset comprehensive`
Full audit coverage (longest — run monthly or before major milestones):
1. `/audit-renderer`
2. `/audit-ecs`
3. `/audit-safety`
4. `/audit-nif`
5. `/audit-performance`
6. `/audit-concurrency`
7. `/audit-audio`
8. `/audit-legacy-compat`
9. `/audit-regression`

### `--preset nif-all-games`
Test NIF parser against all available game data:
1. `/audit-nif --game fnv`
2. `/audit-nif --game skyrim`
3. `/audit-nif --game oblivion`
4. `/audit-nif --game fo4`

## Execution

1. Parse the `--preset` argument from `$ARGUMENTS`
2. `mkdir -p /tmp/audit`
3. Launch each audit as a **background agent** (max 3 concurrent)
4. Each writes to `docs/audits/AUDIT_<TYPE>_<TODAY>.md`
5. When all complete, produce a summary:

```markdown
# Audit Suite Summary — <preset> — <date>

| Audit | Findings | CRITICAL | HIGH | MEDIUM | LOW | Report |
|-------|----------|----------|------|--------|-----|--------|
| Safety | 3 | 0 | 1 | 2 | 0 | AUDIT_SAFETY_... |
| ...   | ... | ... | ... | ... | ... | ... |

Total: X findings (C critical, H high, M medium, L low)
```

6. If any CRITICAL findings, warn prominently
7. Suggest: `/audit-publish docs/audits/AUDIT_<TYPE>_<TODAY>.md` for each report with findings
