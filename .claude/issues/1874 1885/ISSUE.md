# #1885 (NIF-D6-001, LOW) — FIXED · #1874 (HIGH, renderer) — DEFERRED (RenderDoc-gated)

## #1885 — parse_legacy raw Vec::with_capacity on a file-driven count
`NiBlendInterpolator::parse_legacy` reserved its blend-item array with a raw
`Vec::with_capacity(array_size as usize)` on a count read straight from the
stream, instead of the crate's `allocate_vec` byte-budget guard (#408/#764/#768).
`array_size` is u8/u16, so the blast radius was small (~1 MB worst case, then
EOF → block-size recovery) — a pattern-consistency divergence, not a live OOM
like the #388 text-key case.

**Fix** (`crates/nif/src/blocks/interpolator.rs`): route both interpolator
blend-array reservations through `stream.allocate_vec::<InterpBlendItem>(array_size as u32)?`:
- `parse_legacy` line 952 (the named site) — `Vec::with_capacity` → `allocate_vec`.
- `parse_modern` line 910 (SIBLING, same anti-pattern) — `items.reserve(...)` →
  reassign the guarded Vec (`items` is empty there — manager-controlled blends
  carry no array).

Grep confirmed these are the only two raw reservations in `interpolator.rs`;
every other count-driven site already uses `allocate_vec` / `read_array_of`.

**Test**: `parse_legacy_blend_interpolator_rejects_oversized_array_size` —
array_size = u16::MAX with no payload → parse now errors with the allocate_vec
budget message ("only … bytes remain") instead of reserving 65535 items and
EOF-ing in the read loop. The message is unique to the guard, so the test fails
against the old raw-`with_capacity` path (genuine regression pin).

Scoped `cargo test -p byroredux-nif` green (870 lib, +1); full workspace green,
no new warnings.

## #1874 — ghosted diagonal double-image in TES interiors — DEFERRED
This is a **tracking issue**, not a fixable defect in this pipeline. The issue
body itself states "Suggested Fix: None proposed" and files it per the
`AUDIT_RENDERER_2026-07-04.md` "needs RenderDoc" item. The adversarial
investigation narrowed the mechanism (a spatially-uniform bad shared motion
vector, amplified/frozen by TAA's intentional #1479 parked-camera clamp bypass)
but did NOT find the root cause — every motion-vector authoring site reads
correct on static analysis.

Not attempted because:
- The failure mode is invisible to `cargo test` — it needs a live RenderDoc
  capture on the affected frame (motion-vector G-buffer, CameraUBO prevViewProj
  vs. actual previous viewProj, SVGF histAge, TAA history), which can't be
  produced in this environment.
- Project policy is no-speculative-Vulkan-fix. The only in-reach knob (removing
  TAA's parked-camera clamp-skip) is exactly #1479's deliberate convergence
  fix; touching it blind would trade one invisible-to-tests regression for
  another.

Left OPEN. The next step is the user's RenderDoc capture per the four checks in
the issue body.
