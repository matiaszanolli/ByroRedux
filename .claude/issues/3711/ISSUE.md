# #3711 — NIF-2026-08-30-D1-01: the #395 sizeless-stream drift detector fires 4,280 times on an Oblivion corpus with zero real drift

**Severity**: MEDIUM · **Dimension**: Stream Position
**Location**: `crates/nif/src/lib.rs` — `drift_warning`

## Premise, re-verified against real data

Re-ran the exact evidence in a throwaway probe (`crates/nif/examples/
_tmp_drift_threshold_probe.rs`, deleted after use per #3746) against this
machine's real Oblivion GOTY install (base + all eight DLC archives —
`BYROREDUX_OBLIVION_DATA` unset, default path resolved): **9,612 NIFs,
4,280 warnings, 0 real drift, 100% false-positive rate** — exact match to
the issue's own numbers, confirming the premise held at current HEAD.

## What was tried and measured before implementing anything

1. **Raise `prior.len()` alone** (the issue's own "failing that" fallback,
   tried first as the smaller change): `>= 5` → 870 warnings; `>= 10` →
   303 warnings. Reduces but never reaches zero — a long enough same-file
   repeat run of a genuinely variable-length type (`NiTriStrips`,
   `NiMaterialProperty` both still appear at threshold 10) eventually
   beats any fixed sample-count requirement, because the underlying
   distribution really does vary; a threshold only delays the first false
   agreement; it can't prevent one.
2. **Per-type classification, measured not guessed**: extended the probe
   to capture the crate's own unconditional per-block `trace!` line
   (`"Block {i} '{type}': offset {o}, consumed {c} bytes"`, already fired
   for every block — no internal API changes needed) and computed, per
   type, the **maximum within-file spread ever observed anywhere in the
   9,612-file corpus**. The result was cleanly bimodal: 59 types NEVER
   exceed a 2-byte spread even with hundreds of same-file repeats
   (`NiAlphaProperty`, `NiZBufferProperty`, every PSys modifier/controller,
   every Havok collision-object/constraint wrapper, …); every other type
   ranges from a spread of 3 up to 1,285,752 (`NiTriStripsData`). No type
   sits ambiguously in between. Structurally this tracks exactly what
   you'd expect: the fixed bucket is blocks with no embedded strings and
   no `Vec<T>` whose count varies per instance.

## Fix

Implemented the issue's primary suggested direction ("restrict it to
types whose on-disk size genuinely is constant"), using the measured
59-type list rather than the sample-threshold fallback (which the
measurement above proved architecturally can't reach zero):

- Added `FIXED_SIZE_BLOCK_TYPES: &[&str]` (59 entries, sorted for
  `binary_search`), with a doc comment explicitly marking it as a
  **measurement** against a specific corpus, not a spec-derived guess —
  per the no-guessing policy, warns against ever adding an entry from the
  NIF spec alone without re-measuring.
- `drift_warning` gained a `type_name: &str` parameter and now only
  evaluates its priors-agreement heuristic for a type on that list — every
  other type returns `None` immediately, regardless of how well its priors
  happen to agree. The existing ±2-byte agreement check is kept as a
  second line of defense for the allowlisted types (if the allowlist's
  premise is ever wrong for some future NIF variant, this still silently
  no-ops rather than spamming).
- Updated the one call site (`dispatch_blocks`) to pass `type_name`.

## SIBLING (issue's own checklist item)

Checked the other `parsed_size_cache` consumer — the #324 recovery path
(same file, a few lines below `drift_warning`'s call site): when a block's
primary parse returns `Err`, it uses the cache's *median* as a skip-size
recovery hint. Different risk shape from the warning heuristic this issue
targets: it only ever runs on a genuine parse failure (not spuriously on
every successful parse), and a wrong recovery there produces cascading
downstream parse errors rather than log noise — the failure mode is
self-announcing, not a silent false-positive-warning problem. The issue's
own "Related" section already tracks this as a separate, adjacent gap
(#3712 / D3-01), not something this fix's scope covers.

## TESTS (issue's own checklist item — "assert a count of 0 against this 9,612-file corpus in a test")

- `crates/nif/src/tests.rs` — rewrote the `drift_warning` unit tests to
  pass a `type_name`, added
  `drift_warning_silent_for_a_type_not_on_the_allowlist` (the actual fix:
  even a textbook "looks fixed-size" cache must never fire for a type not
  on the list), and `fixed_size_block_types_allowlist_is_sorted_for_binary_search`
  (pins the `binary_search` precondition directly).
- `crates/nif/tests/oblivion_stream_drift_corpus.rs` (new,
  `#[ignore = "needs Oblivion game data on disk"]`) — the literal
  "assert 0 against the 9,612-file corpus" test the issue asked for. Opens
  all nine archives directly (base + eight DLC) rather than going through
  `Game::mesh_archives()`'s Oblivion-restricted, all-or-nothing gate (see
  #2334/#3712) — a drift-warning count is additive, so running against
  just the base archive on a non-GOTY install is still a meaningful
  partial check, not a misleading one. **Run and confirmed: 9 archives,
  9,612 NIFs, 0 drift warnings.**
- Verified the guard actually catches a regression (this session's
  established quality bar): deliberately inserted `"NiSourceTexture"`
  (the single largest pre-fix false-positive emitter, 1,187 of 4,280) into
  `FIXED_SIZE_BLOCK_TYPES` at its correct sorted position, reran the
  corpus test — it failed, reproducing **exactly 1,187** warnings (the
  first ten messages logged match the original evidence's shape), then
  reverted and confirmed 0 again.

## Verification

- `cargo check -p byroredux-nif --tests`: clean.
- `cargo test -q -p byroredux-nif`: 1,221 lib tests + all integration
  suites passing, 0 failing.
- `cargo test -q --no-fail-fast` (full workspace): **7086 passing, 0
  failing** (+2 new non-ignored tests; the corpus test is `#[ignore]`d
  like its siblings but was run manually against real data above).
