# Investigation — #939 (NIF-D3-NEW-03)

## Current state

`crates/nif/src/lib.rs` already counts drift via `drifted_by_type: HashMap<String, u32>`
and emits an aggregate `warn!` summary at end of parse. The per-block `debug!`
line logs `expected N bytes, consumed M` (effectively declared/consumed). What's
missing:

1. **Drift value lost.** The counter is a per-type *count* of drifts — not a
   histogram of the actual drift magnitudes. A type that drifts +1 byte once
   and +12 bytes once is indistinguishable from a type that drifts +1 byte
   twice.
2. **No programmatic surface.** The detail only goes to logs. `nif_stats`
   walking 184k NIFs can't aggregate drift across files because nothing on
   `NifScene` carries it.
3. **No automated regression coverage.** No test pins that a clean parse
   produces zero drift entries.

## Fix shape

- Add `drift_histogram: BTreeMap<String, BTreeMap<i64, u32>>` to `NifScene`
  (type_name → drift_value → occurrence count). i64 since drift = declared - consumed
  can be either sign.
- Populate it from the existing drift-detection site (line 326). Skip Havok
  constraint stubs (#117) just like the existing counter does.
- Update the existing `debug!` line to spell out `drift=N-M` per the issue.
- Add `--drift-histogram` flag to `nif_stats`: aggregate per-type drift
  histograms across the file walk, print a sorted summary.
- Tests: clean-parse → empty `drift_histogram`; synthetic NIF with inflated
  `block_size` → expected single-entry histogram.

## Files touched

1. `crates/nif/src/scene.rs` — add `drift_histogram` field + Default + test fixtures
2. `crates/nif/src/lib.rs` — populate the field; update debug log format; tests
3. `crates/nif/examples/nif_stats.rs` — `--drift-histogram` flag, aggregation, print
4. `crates/nif/src/scene.rs` validate_refs test fixtures (struct literal init sites)

## Scope check

4 files, all within the `byroredux-nif` crate. Well under the 5-file gate.
