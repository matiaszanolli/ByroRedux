# Issues 3327, 3640, 3677, 3701

## #3327 — FNV-2026-08-26-D5-02: 614 FNV shader-float controllers reach the animation importer under the abstract name NiSingleInterpController and are silently dropped
Domain: nif / animation (byroredux-nif)
Severity: medium
Location: crates/nif/src/blocks/mod.rs:786-792 (dispatch), crates/nif/src/anim/entry.rs:404-421 (consumption), crates/nif/src/anim/types.rs:89-112 (FloatTarget)
Fix: newtype-carry the type_name for BSMaterialEmittanceMultController/BSRefractionStrengthController/BSFrustumFOVController (mirror NiPreSplitDataController pattern), add FloatTarget::EmissiveMultiple/RefractionStrength, route through extract_float_channel_at.

## #3640 — FO4-2026-08-30-D4-02: APP_CULLED geometry with a live visibility controller is dropped at import
Domain: nif (byroredux-nif)
Severity: low
Location: crates/nif/src/import/walk/mod.rs — shape.av.flags & 0x01 early-returns (4 shape sites + 4 node sites)
Fix: import culled shapes with visible=false when a visibility channel targets them, instead of dropping; keep unconditional drop otherwise.

## #3677 — PERF-D1-2026-08-30-01: live animation path is the last unconverted per-frame SipHash keyspace
Domain: ecs/animation (byroredux-core + byroredux)
Severity: low, performance
Location: crates/core/src/animation/types.rs:238 (AnimationClip.channels), byroredux/src/components.rs:1287/1298 (NameIndex.map, SubtreeCache.map)
Fix: switch the three std HashMap declarations to rustc_hash::FxHashMap; extend context/mod.rs:2889-style source-scan guard to cover these three.

## #3701 — ECS-2026-08-30-D10-01 (LATENT): AnimationLayer blend-in contributes exactly zero weight for the whole fade
Domain: ecs/animation (byroredux-core)
Severity: medium, LATENT (AnimationStack never registered by production code — no live repro)
Location: crates/core/src/animation/stack.rs (with_blend_in ~80-99, effective_weight ~87-99, advance_stack blend-timer block ~212-219)
Fix: make blend-in a weight target rather than a multiplier on self.weight (which starts at 0); advance_stack writes layer.weight = target * progress each tick. Check blend-out mirror defect too.
