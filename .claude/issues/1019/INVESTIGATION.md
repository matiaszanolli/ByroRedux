# #1019 — Investigation: per-worldspace sun-arc latitude tilt

## Audit premise vs reality

The audit body itself flags this as **deferred to M40**:

> Defer until M40 worldspace-metadata pass.

`compute_sun_arc` in [`byroredux/src/systems/weather.rs:79`](byroredux/src/systems/weather.rs#L79)
is already aware of the gap — line 85 carries the marker comment:

```
// Per-worldspace latitude tilt deferred to #1019.
```

So the issue is a forward-reference placeholder for M40, not a stand-alone
fix request.

## Why this can't ship today

Implementing the tilt requires two things that don't exist yet:

1. **Per-worldspace latitude data.** Skyrim ≈ 60°N analog,
   Cyrodiil ≈ 45°N, Capital Wasteland ≈ 38°N, Mojave ≈ 35°N — these are
   community estimates with no in-record source. Per the `feedback_no_guessing`
   memory, we don't ship guessed values; we wait for either nif.xml-style
   ground truth or a stakeholder-authored table.

2. **A WRLD-record slot to carry it.** The audit body floats "per-worldspace
   latitude metadata (or a WRLD record field if one exists)" — neither
   exists today. WRLD parsing in
   [`crates/plugin/src/esm/cell/walkers/wrld.rs`](crates/plugin/src/esm/cell/walkers/wrld.rs)
   has no latitude subrecord because Bethesda's WRLD format has none. We'd
   need an out-of-band per-worldspace TOML / config the engine reads, which
   is M40's worldspace-metadata pass.

Pre-empting M40 with a hardcoded match-on-WorldspaceFormID table would
poison the M40 design — that work needs to decide the canonical store
(TOML? CLAUDE.md doc? in-tree const?) before we add the first consumer.

## Recommendation

**Close as `wontfix` / pin to M40.** The placeholder comment in
`compute_sun_arc` is the right marker. When M40 lands and adds the
metadata store, the consumer in `compute_sun_arc` is a 6-line change:
read `latitude_deg` off the worldspace resource, rotate the
(`x`, `y`, `z`) arc vector through `Rot(X, π/2 - latitude)` before
normalising.

No code touched in this pass.
