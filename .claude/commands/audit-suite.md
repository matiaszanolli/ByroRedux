---
description: "Run a preset suite of audits in parallel"
argument-hint: "--preset <name>"
---

# Audit Suite Orchestrator

## Presets

### `--preset pre-release`
Run before tagging a release:
1. `/audit-safety`
2. `/audit-renderer`
3. `/audit-ecs`

### `--preset comprehensive`
Full audit coverage:
1. `/audit-renderer`
2. `/audit-ecs`
3. `/audit-safety`
4. `/audit-legacy-compat`

### `--preset renderer-deep`
After significant renderer changes:
1. `/audit-renderer`
2. `/audit-safety` (focused on unsafe + Vulkan)

### `--preset pre-nif`
Before starting NIF loader work:
1. `/audit-legacy-compat`
2. `/audit-ecs` (verify component infrastructure is ready)

## Execution

1. Parse the `--preset` argument from `$ARGUMENTS`
2. Launch each audit as a **background agent** (they write independent reports)
3. Max 3 concurrent agents
4. Each writes to `docs/audits/AUDIT_<TYPE>_<TODAY>.md`
5. When all complete, summarize: which audits ran, finding counts by severity
