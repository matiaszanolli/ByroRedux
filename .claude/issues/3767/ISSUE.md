# #3767 — CHAR-2026-08-30-D4-01: AfflictionTable::band_for requires bands to be sorted ascending, and nothing states, enforces or tests that

**Repo**: matiaszanolli/ByroRedux · **Filed**: 2026-08-30 · **HEAD**: `64f64480`
**Labels**: low, character, bug

---

**Audit**: `/audit-character` — `docs/audits/AUDIT_CHARACTER_2026-08-30.md` (Dimension 4 — Pools, Afflictions, Resistances & Reputation), HEAD `64f64480`
**Finding ID**: `CHAR-2026-08-30-D4-01`

- **Severity**: LOW
- **Status**: NEW

## Location

`crates/core/src/character/affliction.rs:76-98` — `AfflictionTable::band_for` and the `bands` field

## Description

`band_for` returns `self.bands.iter().rposition(|b| pool_value >= b.min_pool)` and its docstring calls that "the highest `min_pool` reached".

`rposition` returns the **last index** satisfying the predicate, which equals the highest threshold only when `bands` is sorted ascending by `min_pool`. `AfflictionTable { pool_avif, bands }` has a fully `pub` `bands: Vec<AfflictionBand>` with no constructor, no sort, no `debug_assert`, and no documented ordering contract.

## Evidence

`affliction.rs:97`:
```rust
self.bands.iter().rposition(|b| pool_value >= b.min_pool)
```

With `bands = [{min 600, …}, {min 200, …}]` and `pool = 700`, both predicates hold and `rposition` yields index `1` — the *200* band — so a heavily-irradiated actor gets the mild penalty.

The only test, `band_for_picks_the_highest_threshold_reached`, builds `stand_in_radiation_table()` already sorted, so it cannot detect this. Re-verified at HEAD: `affliction.rs:80` `pub bands: Vec<AfflictionBand>`, `:96-97` unchanged.

## Impact

Latent, not live: no `AfflictionTable` is constructed in production today (thresholds are PENDING for every game per `charal.md` §4.6, and `affliction_tick_system` has no scheduler registration).

The blast radius is the moment real per-game tables *are* authored — a threshold list transcribed in the natural "worst first" reading order from a wiki table silently inverts every band, and `reevaluate_affliction`'s diff logic stays perfectly consistent while doing it, so there is no crash and no assertion to catch it. This is the same class as the transposed-grid trap Dimension 4 checks for on `ReputationStanding`, which *is* pinned by an asymmetric test.

## Related

- `charal.md` §4.6 (thresholds PENDING for every game)
- #3766 / `CHAR-2026-08-30-D2-01` (sibling latent-contract doc gap in `derived.rs`)

## Suggested Fix

State the ascending-`min_pool` contract on the `bands` field, add a `debug_assert!(bands.is_sorted_by_key(|b| b.min_pool))` in a constructor (or sort on construction), and extend `band_for_picks_the_highest_threshold_reached` with a deliberately unsorted table asserting the intended answer.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (any other `rposition`/`position` threshold lookup over a `pub Vec` in `crates/core/src/character/`)
- [ ] **TESTS**: A regression test pins this specific fix — an unsorted `bands` vector must not silently return the wrong band
