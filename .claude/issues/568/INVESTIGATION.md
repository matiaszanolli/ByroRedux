# #568 Investigation

## Premise confirmed — hidden recovery rate

When a block parser returns `Err` mid-file, `parse_nif` has three
recovery paths (`lib.rs:302` block-size seek, `:349` runtime size cache,
`:372` oblivion_skip_sizes hint) plus one at dispatch level
(`blocks/mod.rs:696` unknown-type fallback). All four substitute an
`NiUnknown` placeholder and continue. None of them bumped a counter on
`NifScene`; `nif_stats::record_success` ran as if the scene were clean.

Measured impact once the counter landed:

| Game | Clean | Recoverable |
|---|---|---|
| Starfield | **0.80%** | 100.00% |
| Skyrim SE | **45.50%** | 100.00% |
| FNV | 82.94% | 100.00% |
| FO4 | 95.22% | 100.00% |
| FO76 | 95.57% | 100.00% |

Starfield reported "100.00% clean" pre-fix. The actual clean rate is
0.80%; **99.20% of Starfield NIFs had at least one block replaced by
`NiUnknown`** during parse. That's the `bhkRigidBody` fall-through from
#546 (SK-D5-01) plus dozens of other under-consuming parsers hidden
behind the old gate.

## Fix

Option B from the issue body — a separate `recovered_blocks: usize`
counter on `NifScene`. Distinct from `dropped_block_count` (blocks lost
past a hard abort point) because the scene still contains a
placeholder block at the original index, so downstream block refs still
resolve. The distinction matters for diagnosis.

### Increments

- `lib.rs:302` — block-size seek recovery
- `lib.rs:349` — runtime size cache recovery
- `lib.rs:372` — oblivion_skip_sizes hint recovery
- `lib.rs:300` (new check, post-parse) — dispatch-level unknown-type
  fallback. `parse_block` returns `Ok(NiUnknown)` here; the outer loop
  now inspects `block.block_type_name() == "NiUnknown" && type_name !=
  "NiUnknown"` before pushing.

The one `Box::new(NiUnknown)` site that does NOT bump the counter is
`lib.rs:210` — the `skip_animation` option flag's intentional skip for
KF-only parsing. Correctly not a recovery.

### Consumers

- `nif_stats::process_bytes`: routes `recovered_blocks > 0` through
  `record_truncated` (Option B from the issue).
- `tests/common/mod.rs::parse_all_nifs_in_archive` (both variants):
  ditto, routes into `ParseStatus::Truncated`.
- `parse_real_nifs.rs`: gate switches from `MIN_SUCCESS_RATE` (clean-
  rate) to `MIN_RECOVERABLE_RATE` (clean + recovered + truncated), still
  pinned at 1.0. No hard parse failure is allowed; the `clean` rate
  rides as a secondary metric. Previously the gate was unmeetable at
  100% clean because the recoveries dropped it as low as 0.80%.

## Why the gate moves to recoverable

The issue's body notes *"After this lands, #546 can be gated on a
parse-rate regression test rather than manual inspection."* The design
intent is: clean rate becomes a **diagnostic** metric you can watch for
regressions against a baseline; recoverable rate stays the hard gate
(no silent data loss from hard parse failures).

Pinning the gate at 100% clean would force every downstream parser bug
to be fixed before landing #568 — the issue's source audit
(`AUDIT_SKYRIM_2026-04-22.md`) already flags more than a dozen such
bugs. Driving clean upward is ongoing follow-up work tracked on
individual parser issues.

## Tests

- `recovered_blocks_flagged_for_unknown_dispatch_fallback`: builds a
  synthetic NIF with an unknown block type + block_size, asserts
  `recovered_blocks == 1` + `truncated == false` + the placeholder is
  an `NiUnknown` at index 0.
- Existing `nif_scene_struct_carries_truncated_field` extended to
  assert `recovered_blocks` on the new field surface.
- 1029 workspace tests pass. 7 integration parse-rate tests pass at the
  new recoverable-rate gate (all 100% recoverable).

## Files changed

- `crates/nif/src/scene.rs` — new `recovered_blocks: usize` field,
  `Default` updated, 6 internal test-fixture literals updated.
- `crates/nif/src/lib.rs` — bump counter at 4 recovery sites, thread
  into the returned `NifScene`, one struct-literal test fixture
  updated, new regression test.
- `crates/nif/examples/nif_stats.rs` — route `recovered_blocks > 0`
  through `record_truncated`.
- `crates/nif/tests/common/mod.rs` — same routing in both
  `ParseStats` call sites.
- `crates/nif/tests/parse_real_nifs.rs` — gate switches from
  `MIN_SUCCESS_RATE` to `MIN_RECOVERABLE_RATE`; docstring rewritten to
  explain the metric split.
