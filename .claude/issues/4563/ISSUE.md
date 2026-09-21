# NIFAL-D7-2026-09-21-02: Canonical clip fields are overridden after the convert_hkx_clip boundary at two HKX install sites

**Labels**: low, nifal, animation, tech-debt, bug

**Severity**: LOW · **Dimension**: Animation / controllers (HKX boundary hygiene) · **Tier Violated**: single-boundary (softened — policy decided at call sites, not a second construction site) · **Game Affected**: Skyrim
**Location**: `byroredux/src/asset_provider/animation.rs:206-212` (walk installer sets `clip.accum_root_name` post-boundary, duplicating the boundary's own COM lookup at `:482-484`) and `:304` (combat installer sets `clip.cycle_type = Clamp` after the boundary chose `Loop`)
**Source**: `docs/audits/AUDIT_NIFAL_2026-09-21.md` (CONFIRMED at `4dca737e2`)

### Description
`convert_hkx_clip` owns cycle-type and accum-root policy. The walk installer re-implements the COM bone lookup outside the boundary to bind `NPC COM [COM ]`, and the combat installer overwrites `cycle_type` after the boundary's decision. Both are once-at-install mutations (not per-frame re-resolution) and not field-by-field construction — the caller-count detector correctly does not fire — but the "one decision point per canonical field" doctrine now has three places deciding `cycle_type` (boundary cart rule, boundary Loop default, combat Clamp override).

### Evidence
`let mut clip = convert_hkx_clip(…); … clip.accum_root_name = Some(pool.intern(&com.name));` (walk, :195-212); `clip.cycle_type = CycleType::Clamp;` (combat, :304).

### Impact
A future field addition or policy change to `AnimationClip` must be checked against the boundary AND every call-site override; the COM-lookup duplication can drift (e.g. a different accum-root name per rig).

### Related
#2305 / NIFAL-D7-NEW-01 (the declared second boundary)

### Suggested Fix
Parameterize `convert_hkx_clip` (e.g. an explicit `one_shot: bool` and `accum_root: Option<&str>`) so the overrides happen inside the one boundary; the walk installer then stops duplicating the COM lookup.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other readers, other gates)
- [ ] **TESTS**: A regression test pins this specific fix
