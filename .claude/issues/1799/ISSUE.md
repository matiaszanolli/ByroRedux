# PERF-D5-NEW-01: Legacy 16-slot WRS reservoir arrays stay live on the default ReSTIR path — dead per-thread storage in the frame's hottest shader

**Issue**: #1799
**Labels**: medium,renderer,pipeline,performance,bug
**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-07-02.md` (PERF-D5-NEW-01)

**Severity**: MEDIUM
**Source**: `AUDIT_PERFORMANCE_2026-07-02.md` (PERF-D5-NEW-01)

## Location
`crates/renderer/shaders/triangle.frag:1967-1983` (declaration + unconditional init), `:2237-2250` (legacy streaming writes), `:2558-2673` (legacy pass-2 reads)

## Description
#1369 retired a larger reservoir array (dropping the third `resRadiance` array, 320 B → 128 B) because per-thread reservoir storage was "the dominant per-thread footprint suppressing WRS occupancy," landing at `resLight[16]` + `resWSel[16]`. Session 49 then made ReSTIR-DI (a single scalar reservoir) the default shadow path, but kept the 16-slot legacy WRS arm compiled in for a runtime A/B toggle (`DBG_DISABLE_RESTIR`, a dynamically-uniform branch, not a compile-time constant). The arrays are declared and zero-initialized before `useRestir` is even computed (`:1980-1983` init vs `:1998` compute), so the compiler must budget their registers/local memory on every invocation — including the ~100% of production frames that take the ReSTIR path and never read them.

## Evidence
`NUM_RESERVOIRS = 16` at `:1967`; unconditional init loop `:1980-1983`; `useRestir` at `:1998` from a runtime-uniform flag; only the `!useRestir` path touches the arrays afterward.

## Impact
Up to ~32 extra live registers (or spilled local bytes) per fragment thread in a shader that already carries the full RT uber-path — silently re-eroding a portion of the #1369 occupancy win on a path that gets zero benefit. Blast radius: every lit fragment, every frame, every game. (Footprint smaller than a naive read: #1369 already halved the array set.) Confidence: MEDIUM — storage-lifetime analysis is code-verified; the magnitude of the occupancy hit needs Nsight/RenderDoc SASS confirmation. ALU/register-only, no pipeline-state/barrier change.

## Related
#1369, `DBG_DISABLE_RESTIR` toggle.

## Suggested Fix
Promote the legacy-WRS arm to a compile-time toggle through the existing generated-constants channel (the mechanism #1758 used for skin workgroup size); A/B then costs a shader recompile instead of taxing every production frame.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader passes / other reservoir arrays)
- [ ] **TESTS**: A regression test pins this specific fix

