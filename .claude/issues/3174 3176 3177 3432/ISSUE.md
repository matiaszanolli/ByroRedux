# Issue #3174
title:	NIF-D1-2026-08-20-01: the NiPSys*Ctlr family has the same until=10.1.0.103 / since=10.1.0.104 split #2562/#2563 just fixed on nine siblings — 22 dispatch names, compound desync
state:	OPEN
author:	matiaszanolli (Matias Zanolli)
labels:	bug, medium, nif, nif-parser
comments:	0
assignees:	
projects:	
milestone:	
issue-type:	
parent:	
sub-issues:	
sub-issues-completed:	
blocked-by:	
blocking:	
number:	3174
--
Filed from `docs/audits/AUDIT_NIF_2026-08-20.md` (Dimension 1 — Stream Position Integrity).

**Severity**: MEDIUM — escalates to HIGH on any sizeless file that actually carries one, since `read_string` then reads a garbage length prefix.
**Game Affected**: any NIF below file-version **10.1.0.104** — on shipping content, the Oblivion-era NetImmerse band (`bsver` <= 11, file versions v3.3–v10.1.0.103). FO3+ (`v20.2.0.7`) and later are above the boundary and unaffected.

**Location**:
- `crates/nif/src/blocks/particle.rs:938-947` (`parse_modifier_ctlr`)
- `crates/nif/src/blocks/particle.rs:950-963` (`parse_emitter_ctlr`)
- `crates/nif/src/blocks/particle.rs:965-978` (`parse_multi_target_emitter_ctlr`)
- dispatched from `crates/nif/src/blocks/mod.rs:1135-1169`
- existing gate helper that should be reused: `NifVersion::has_keyframe_controller_data` (`crates/nif/src/version.rs:262`)

## This is an INCOMPLETE FIX, not a new class

#2562 / #2563 (landed as `e32e2b1f`) fixed the missing `until="10.1.0.103"` `Data` ref on **nine** `NiSingleInterpController` subclasses. nif.xml declares the same field on **four more**, all in the `NiPSys*Ctlr` family, and none of them were touched — the fix only reached `blocks/controller/mod.rs` and `blocks/controller/shader.rs`, and `version.rs:247-262`'s enumerating doc comment lists exactly the nine that were fixed.

| type | `Data` template | reaches |
|---|---|---|
| `NiPSysEmitterCtlr` | `NiPSysEmitterCtlrData` | `parse_emitter_ctlr` |
| `NiPSysModifierActiveCtlr` | `NiVisData` | `parse_modifier_ctlr` |
| `NiPSysModifierFloatCtlr` (abstract base of 19 dispatch names) | `NiFloatData` | `parse_modifier_ctlr` |
| `NiFloatsExtraDataController` | `NiFloatData` | *no dispatch arm* |

## Description — a compound desync, worse than a plain missing field

These three functions do **not** delegate to `NiSingleInterpController::parse` (which gates its interpolator ref correctly at `controller/mod.rs:257`). They open-code the inheritance chain and read the interpolator ref *unconditionally*:

```rust
// particle.rs:938-943 — parse_modifier_ctlr (verified at HEAD)
let _base = parse_interp_controller_base(stream)?;
let _interpolator_ref = stream.read_block_ref()?; // NiSingleInterpController
let _modifier_name = stream.read_string()?;       // NiPSysModifierCtlr
//  ^ no version gate                             ^ and no trailing Data ref
```

Below v10.1.0.104 that produces a three-stage desync:
1. 4 bytes of a non-existent interpolator ref are consumed;
2. `modifier_name` is then read from a 4-byte-shifted offset — and since the string table only exists at `STRING_TABLE_THRESHOLD` and above, `read_string` here reads a `u32` length prefix, so a shifted read yields an arbitrary length;
3. the real trailing `Data` ref is never read.

## Evidence

nif.xml field declarations (extracted mechanically from `/mnt/data/src/reference/nifxml/nif.xml`):
```
NiPSysEmitterCtlr:           <field name="Data" type="Ref" template="NiPSysEmitterCtlrData" until="10.1.0.103" />
                             <field name="Visibility Interpolator" type="Ref" template="NiInterpolator" since="10.1.0.104" />
NiPSysModifierActiveCtlr:    <field name="Data" type="Ref" template="NiVisData"   until="10.1.0.103" />
NiPSysModifierFloatCtlr:     <field name="Data" type="Ref" template="NiFloatData" until="10.1.0.103" />
NiFloatsExtraDataController: <field name="Data" type="Ref" template="NiFloatData" until="10.1.0.103" />
```

**The inconsistency is visible inside one function.** `parse_emitter_ctlr` already gates the *visibility* interpolator on `>= V10_1_0_104` (`particle.rs:958`, the #1544 fix) and its own comment even names the missing field — *"the pre-10.1.0.104 `Data` ref is the mutually-exclusive legacy slot"* — while the *primary* interpolator two lines above it (`:952`) is read with no gate at all, and the legacy slot is never read.

Neither `niobject` carries a `since=` attribute in nif.xml, so the schema itself considers the pre-split form reachable; that is why the `Data` ref is declared at all.

**Dispatch blast radius: 22 type names** — `NiPSysEmitterCtlr`, `BSPSysMultiTargetEmitterCtlr`, `NiPSysModifierActiveCtlr`, and the 19 `NiPSysModifierFloatCtlr` descendants listed at `blocks/mod.rs:1139-1168` (`NiPSysEmitterSpeedCtlr`, `NiPSysGravityStrengthCtlr`, `NiPSysAirFieldSpreadCtlr`, `NiPSysRotDampeningCtlr`, …). Notably `blocks/mod.rs:1156-1164`'s own comment asserts the trailing ref "is gated `until="10.1.0.103"` so FO76 (v20.2.0.7) skips it via the same NiTimeController base" — but **no code path implements that gate**; the field is simply never read at any version.

## Impact

Latent on vanilla content (0 truncations on Oblivion today proves no vanilla sub-10.1.0.104 file carries a `NiPSys*Ctlr`), exactly as #2563 characterised its own eight latent types. Reachable on mod / legacy NetImmerse particle content. On a sizeless file there is no `block_size` anchor, so the desync cascades through every subsequent block — the same failure mode that cost `meshes\marker_map.nif` 8 of its 13 blocks before #2562.

## Suggested Fix

In `particle.rs`, gate the interpolator ref on `stream.version() >= NifVersion::V10_1_0_104` (or better: delegate to `NiSingleInterpController::parse` so the gate cannot drift), and add the complementary `has_keyframe_controller_data()`-gated `Data` ref after `modifier_name` in all three functions — the field sits at the same offset for `NiPSysModifierActiveCtlr` (via `NiPSysModifierBoolCtlr`, no own fields) and `NiPSysModifierFloatCtlr`, so one read covers both. Extend `version.rs:247-262`'s enumerating doc comment with the four new types. Pin with synthetic v10.1.0.103 fixtures following the convention `crates/nif/src/blocks/dispatch_tests/controllers.rs` already established for the nine sibling types.

## Related

- Direct continuation of #2562 / #2563 (`e32e2b1f`) — same defect class, four types they did not reach.
- `NiFloatExtraDataController` (**singular**) *was* fixed by them; `NiFloatsExtraDataController` (**plural**) has no dispatch arm and so is out of scope until one is added — the two names are one character apart and will be easy to conflate.

## Completeness Checks
- [ ] **SIBLING**: all three `particle.rs` controller parsers fixed, not just the one reproduced; `version.rs`'s enumerating doc comment extended to 13 types
- [ ] **GATE-REUSE**: the fix uses `NifVersion::has_keyframe_controller_data` / `V10_1_0_104` rather than a fourth open-coded version literal
- [ ] **TESTS**: synthetic v10.1.0.103 + v10.1.0.104 fixtures pin both halves (interpolator gate AND trailing `Data` ref) per `dispatch_tests/controllers.rs`
- [ ] **DISPATCH-PARITY**: the stale comment at `blocks/mod.rs:1156-1164` (claims a gate that does not exist) is corrected in the same pass


---

# Issue #3176
title:	NIF-D4-2026-08-20-03: the #2632/#2828 degenerate-tangent guard emits a zero tangent for exactly the case its own comment names as the motivation
state:	OPEN
author:	matiaszanolli (Matias Zanolli)
labels:	bug, low, nif, nif-parser
comments:	1
assignees:	
projects:	
milestone:	
issue-type:	
parent:	
sub-issues:	
sub-issues-completed:	
blocked-by:	
blocking:	
number:	3176
--
Filed from `docs/audits/AUDIT_NIF_2026-08-20.md` (Dimension 4 — Geometry Extraction & Import Handoff).

**Severity**: LOW — the shader has a documented, legitimate fallback for a zero tangent, so the observable effect is "synthesis silently declines", not corruption and not NaN.
**Game Affected**: all — both producers. Z-up (`synthesize_tangents`) serves Oblivion / FO3 / FNV `NiTriShape` and Skyrim SE+ `BSTriShape` without `VF_TANGENTS`; Y-up (`synthesize_tangents_yup`) serves Starfield `BSGeometry` and SSE-reconstructed geometry.

**Location**: `crates/nif/src/import/mesh/tangent.rs:275-292` (Z-up, added by `8075133c` / #2828) and `:481-502` (Y-up, added by #2632).

## Description

Both degenerate branches take a cyclic permutation of the normal as a seed tangent, Gram-Schmidt it against `N`, and normalize:

```rust
let t_y_raw = [n_yup[1], n_yup[2], n_yup[0]];               // :489
let dot_nt  = n_yup[0]*t_y_raw[0] + n_yup[1]*t_y_raw[1] + n_yup[2]*t_y_raw[2];
let mut t_y = [ t_y_raw[0] - n_yup[0]*dot_nt, … ];
normalize_inplace(&mut t_y);
let b_y = cross(n_yup, t_y);
```

The #2632 comment directly above states the reason for the projection: *"a raw cyclic permutation of N's components is NOT generally orthogonal to N (e.g. **any N with all-equal components permutes to itself**)"*.

But for precisely that input the projection removes the entire vector: `t_y_raw == N` => `dot_nt == 1` => `t_y == [0,0,0]`, and `normalize_inplace` maps a below-`1e-12` vector to `[0.0, 0.0, 0.0]` (`tangent.rs:550-560`) rather than picking a different seed. `b_y = cross(N, 0) = 0` follows, and `bitangent_sign(N, 0, 0)` returns `+1.0` via `clamp_sign(0.0)` (`crates/nif/src/types.rs:154-162`). The vertex ships `[0.0, 0.0, 0.0, 1.0]`.

The Z-up sibling has the identical algebra: the coordinate swap is orthogonal, so `dot(n_yup, t_y_raw) == dot(n_zup, t_z)` and the trigger condition is unchanged.

## Evidence

`N = ±(1,1,1)/sqrt(3)` is the trigger, and it is representable in every source encoding the branch sees. `BSTriShape` normals are three independent `byte_to_normal` reads (`crates/nif/src/blocks/tri_shape/bs_tri_shape.rs:1194-1197`), so any vertex whose three normal bytes are equal — the ordinary encoding for a diagonal/corner-chamfer normal — lands exactly on it.

The branch is additionally gated on `vec3_is_zero(&tangent_zup) || vec3_is_zero(&bitangent_zup)` i.e. degenerate UVs, so the two conditions must coincide; that makes it rare, not unreachable.

Pre-#2828 / pre-#2632 the branch returned the raw permutation, which is non-orthogonal but never zero — so the guard traded a slightly-wrong basis for **no basis at all** in this one case.

## Impact

Bounded. `triangle.frag`'s documented contract (`crates/renderer/shaders/triangle.frag:446-451`) is that a zero `fragTangent.xyz` falls back to the screen-space derivative TBN (Path 2), so the affected vertices get a valid — just not authored-quality — basis. No NaN, no corruption. The cost is that `synthesize_tangents`' whole purpose is defeated on those vertices, silently.

## Suggested Fix

When `t_y` collapses below the `normalize_inplace` threshold, fall back to a second, non-parallel seed — the standard choice is the world axis least aligned with `N` (pick the smallest of `|N.x|` / `|N.y|` / `|N.z|`, cross with `N`), which is orthogonal by construction and cannot degenerate. Add the `N = (1,1,1)/sqrt(3)` case to `crates/nif/src/import/mesh/tangent_convention_tests.rs`, whose existing #2632 guard tests (`:229`, `:290`) use normals that do not trigger it.

## Related

- #2828 (CLOSED, Z-up half), #2632 (CLOSED, Y-up half). This is **not** a regression of either — the code is exactly as they landed it — it is an incompleteness in the fix they both implement.
- Distinct from OPEN #2815 (`perturbNormal` Path 1 NaN when tangent is *parallel* to normal) — that is a renderer-side guard for a non-zero degenerate tangent; this is a producer-side zero.
- Sibling of the Z-up normalize asymmetry filed alongside this one (same function pair).

## Completeness Checks
- [ ] **SIBLING**: the fallback seed is applied to **both** producers (`:275-292` Z-up and `:481-502` Y-up) — they have identical algebra
- [ ] **TESTS**: `tangent_convention_tests.rs` gains an `N = (1,1,1)/sqrt(3)` + degenerate-UV case asserting a non-zero, unit, N-orthogonal tangent
- [ ] **NO-REGRESSION**: existing #2632 guard tests at `:229` / `:290` still pass unchanged


---

# Issue #3177
title:	NIF-D4-2026-08-20-04: synthesize_tangents (Z-up) never normalizes N while its Y-up sibling does — and it is the producer receiving quantized BSTriShape normbyte normals
state:	OPEN
author:	matiaszanolli (Matias Zanolli)
labels:	bug, low, nif, nif-parser
comments:	1
assignees:	
projects:	
milestone:	
issue-type:	
parent:	
sub-issues:	
sub-issues-completed:	
blocked-by:	
blocking:	
number:	3177
--
Filed from `docs/audits/AUDIT_NIF_2026-08-20.md` (Dimension 4 — Geometry Extraction & Import Handoff).

**Severity**: LOW
**Game Affected**: Skyrim SE / FO4 / FO76 `BSTriShape` meshes that set `VF_NORMALS | VF_UVS` but not `VF_TANGENTS`. Oblivion / FO3 / FNV `NiTriShape` normals are authored `f32` and are unit in practice, so the legacy path is unaffected in practice.

**Location**: `crates/nif/src/import/mesh/tangent.rs:261-262` (Z-up, no normalize) vs `:462-463` (Y-up, explicit normalize). Caller supplying the quantized input: `crates/nif/src/import/mesh/bs_tri_shape.rs:199-204`.

## Description

#2632 added an explicit per-vertex normalize to the Y-up producer with a comment stating the reason plainly:

```rust
// tangent.rs:454-463
// #2632 / SF2D2-D2-04 — `normals_yup` is unit-length only to
// quantization for a UDEC3-decoded source …; the Gram-Schmidt
// projection below (and the degenerate branch's permutation +
// cross product) is only correct for `|n| == 1`.
let mut n_yup = normals_yup[i];
normalize_inplace(&mut n_yup);
```

The Z-up producer does no such thing — it converts the raw normal and uses it directly:

```rust
// tangent.rs:261-262
let n_zup = normals_zup[i];
let n_yup = byroredux_core::math::coord::zup_to_yup_pos([n_zup.x, n_zup.y, n_zup.z]);
```

and then runs the same `T - N*(N.T)` projection at `:305-311` and `:317-323`. The stated precondition (`|n| == 1`) is not established on this path.

## Evidence

The Z-up producer's third caller is `crates/nif/src/import/mesh/bs_tri_shape.rs:199`, which passes `shape.normals` — decoded as three independent `byte_to_normal` reads with **no vector renormalization** (`crates/nif/src/blocks/tri_shape/bs_tri_shape.rs:1194-1205`). That is the same class of quantized, non-unit input #2632 fixed for UDEC3, arriving at the sibling that was left unpatched. #2632's own scope note ("Starfield BSGeometry … SSE-reconstructed BSTriShape") explains why: the Z-up caller was not in view.

## Impact

Small and systematic rather than catastrophic. With `|N|^2 = 1 + eps`, the Gram-Schmidt step over-subtracts by `eps*(N.T)*N`, so the emitted tangent is not exactly orthogonal to the shading normal and the derived bitangent sign can flip on near-perpendicular cases. Normbyte quantization keeps `eps` under about ±1.5%, so the angular error is sub-degree — visible, if at all, as slight normal-map shading drift on Skyrim+/FO4 meshes shipping no authored tangents. No corruption.

## Suggested Fix

Mirror `:462-463` — bind `n_yup` mutably and `normalize_inplace` it once per vertex before the branch, and carry the same #2632 comment so the two producers document one shared precondition. Cheap (one `sqrt` per vertex on a cold path) and a no-op for already-unit input.

## Related

- #2632 (CLOSED) fixed exactly this on the Y-up half.
- Sibling of the degenerate-tangent zero-seed finding filed alongside this one (same function pair, same divergence between the two producers). Land them together.

## Completeness Checks
- [ ] **SIBLING**: both producers end up with one identical, commented precondition (`|n| == 1` established locally, not assumed of callers)
- [ ] **TESTS**: a `BSTriShape`-shaped fixture with deliberately non-unit normbyte normals asserts the emitted tangent is orthogonal to the normalized normal within tolerance
- [ ] **PERF**: the added `sqrt` stays on the import path only (not a per-frame cost)


---

# Issue #3432
title:	SAFE-2026-08-27b-01: NiControllerSequence `duration` and `weight` are unsanitised past #3258 — both latch a NaN into the pose
state:	OPEN
author:	matiaszanolli (Matias Zanolli)
labels:	animation, bug, medium, nif, nif-parser, safety
comments:	0
assignees:	
projects:	
milestone:	
issue-type:	
parent:	
sub-issues:	
sub-issues-completed:	
blocked-by:	
blocking:	
number:	3432
--
From `docs/audits/AUDIT_SAFETY_2026-08-27b.md` (Dimension 8 — NPC/animation spawn safety + Dimension 9 — NaN/Inf on the GPU).

- **Severity**: MEDIUM
- **Location**: producer `crates/nif/src/anim/sequence.rs:20` (`duration`) and `:23` (`weight`); boundary `byroredux/src/anim_convert.rs:506` + `:520`; consumers `crates/core/src/animation/player.rs:61-84` + `:134-142`, `crates/core/src/animation/stack.rs:165-181`, `:332-334`, `:378-380`
- **Status**: NEW. Sibling of #3258 (CLOSED, fixed in `bbfd742f`); nothing in the issue list or `docs/audits/` covers `duration`/`weight`.

## Description

#3258 established the rule: `NiControllerSequence` scalars are raw file data, and a non-finite one that reaches the animation clock latches the entity's pose to NaN for the rest of its life. It fixed exactly one scalar, `frequency`, at the translate boundary (`sanitized_clip_frequency`), plus a defense-in-depth `finite_time_delta` on the `dt * speed * frequency` product.

The **two adjacent fields of the same struct, read by the same parser function, and passed through the same three lines of `convert_nif_clip`, were not touched** — and each has its own latch:

1. **`duration`** — `CycleType::Reverse` routes through `fold_reverse_time` (`player.rs:61-84`), whose only guard is `if duration <= 0.0`. `NaN <= 0.0` is **false**, so a NaN duration falls through: `period = 2.0 * NaN = NaN`, `m = (phase + delta).rem_euclid(NaN) = NaN`, and the `m > duration` branch is `NaN > NaN` = false, so it returns `(NaN, false)`. `local_time` is NaN from that tick onward and never recovers. `advance_stack` (`stack.rs:172-181`) carries the byte-identical arm.
2. **`weight`** — `sample_blended_transform`'s per-layer skip is `let ew = layer.effective_weight() * clip.weight; if ew < 0.001 { continue; }` (`stack.rs:332-334`, repeated at `:378-380`). `NaN < 0.001` is **false**, so a NaN-weighted layer is *not* skipped; `total_weight` becomes NaN, the `total_weight < 0.001` early return at `:363` is likewise false, and the blended position / rotation / scale come out NaN.

`find_key_pair` (`crates/core/src/animation/interpolation.rs`) does not rescue either: it handles ±inf correctly (endpoint clamps) but a NaN `time` fails **both** comparisons, falls into the binary search, and emits `t = (NaN - t_lo) / dt` = NaN. There is no `is_finite` check anywhere between there and the GPU.

The affected import path is the one that matters: `import_sequence` is what `import_kf` calls for **both** standalone `.kf` files and embedded `NiControllerManager` sequences (`crates/nif/src/anim/entry.rs`). The other path, `import_embedded_animations`, is already immune — it derives duration from key times behind a `> 0.0` guard.

## Evidence

Producer — no finiteness gate on either field:
```rust
// crates/nif/src/anim/sequence.rs:20-23
let duration = seq.stop_time - seq.start_time;
let cycle_type = CycleType::from_u32(seq.cycle_type);
let frequency = seq.frequency;
let weight = seq.weight;
```

Boundary — the gap is visible in three consecutive lines:
```rust
// byroredux/src/anim_convert.rs:504-520
AnimationClip {
    name: nif.name.clone(),
    duration: nif.duration,                          // ← unvalidated
    cycle_type,
    // #3258 — `NiControllerSequence.frequency` is raw file data …
    frequency: sanitized_clip_frequency(nif.frequency),
    weight: nif.weight,                              // ← unvalidated
```

Float semantics verified by execution rather than by reading:
```
f32::MIN - f32::MAX = -inf   finite=false
NaN <= 0.0                   = false     // fold_reverse_time's only guard
(0.35f32).rem_euclid(2.0*NaN)= NaN,  NaN > NaN = false
NaN < 0.001                  = false     // sample_blended_transform's skip
```

## Impact

A `.kf` or embedded sequence carrying a non-finite `stop_time`/`start_time` pair (or a literal NaN `weight`) poisons the affected entity's bone transforms permanently — `Transform` → `GlobalTransform` → the `GpuInstance` model matrix and the bone palette. NaN on the GPU is UB by this project's own severity rules. Corrupt or hostile archive content is the realistic source, which is exactly the reachability #3258 was accepted on. Rated MEDIUM to match #3258's own label rather than escalated.

## Related

#3258 (the fix that stopped one field short), #3194 (the same NaN-transparency class on the SpeedTree wind field), #3373 (the same "a later field was added past the sanitiser" shape in `Material`).

## Suggested Fix

Sanitise both at the same boundary `frequency` already uses. `duration`: reject non-finite (and negative) to `0.0`, which every cycle arm already treats as "no wrap / no fold". `weight`: reject non-finite to `1.0`, nif.xml's own default. Then make the gates NaN-safe rather than NaN-transparent — `if !(ew >= 0.001) { continue; }` and `if !(duration > 0.0) { return (0.0, false); }` — so a future producer cannot reopen it.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (the `advance_time` / `advance_stack` twin arms, the second `ew < 0.001` site at `stack.rs:378`)
- [ ] **CANONICAL-BOUNDARY**: per-game logic stays at the NIFAL parser→canonical boundary — the sanitiser belongs in `anim_convert.rs`, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix


---

