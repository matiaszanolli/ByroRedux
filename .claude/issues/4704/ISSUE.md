# GAME-D5-2026-09-21-03: Quest-alias package overlays are never evaluated for actors whose own resolved package stack is empty

**Issue**: #4704
**Filed**: 2026-09-22 (audit-publish, AUDIT_GAMEPLAY_2026-09-21.md)

**Severity**: MEDIUM
**Dimension**: 5 — AI Package Selection
**Location**: `byroredux/src/npc_spawn/ai_package.rs` (`apply_ai_package_behavior` early return, no `AmbientPackageRuntime` insert; `ambient_ai_package_system` iterates runtime holders only)

## Description
`apply_ai_package_behavior` returns before inserting `AmbientPackageRuntime` when the TPLT-resolved package list is empty. `ambient_ai_package_system` — the only reader of `QuestAliasInjectedOverlays` — only visits entities that already carry that runtime.

## Evidence
`aliaspk` probe on Skyrim.esm: 2,131 ALPC aliases, 172 forced-reference on NPC_, 44 target package-less actors, 19 resolve to a supported behavior (e.g. `dunUstengravQST`, `MG03CallerAlias`).

## Impact
Quest-assigned sandboxing/patrolling never starts for those actors; they stand idle.

## Related
Skill known-open: tree packages resolve only Sandbox/Patrol leaves (separate scope limitation).

## Suggested Fix
Always insert `AmbientPackageRuntime` at spawn (even with an empty candidate list), or insert it lazily when an alias overlay lands.
