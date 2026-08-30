# #3752: SPT-2026-08-30-D1-01: parse_spt's error contract is documented as two fatal conditions but has five, and three of them discard the entries already decoded

**Labels**: bug, low, speedtree, doc-rot
**Filed**: 2026-08-30 (audit-publish)

---

**Report**: `docs/audits/AUDIT_SPEEDTREE_2026-08-30.md` · **Severity**: LOW · **Dimension**: 1 (Walker Byte-Accounting)
**Game affected**: all `.spt` games

## Location
- `crates/spt/src/parser.rs` — the `parse_spt` contract docstring (`:39-46`) and `read_payload` (`:192-249`)
- `crates/spt/src/stream.rs` — `read_string_lp` (`:88-106`)

## Description
The contract says *"Returns `Err(io::Error)` only on truly fatal conditions — magic-header mismatch or stream underflow during a partially-read payload."*

**Three further `Err` paths exist**: `read_string_lp`'s > 64 KiB length cap, `read_payload`'s `count × stride` > 64 KiB array cap, and the defensive context-sensitive-kind arm. All three return `InvalidData`, and none is an underflow.

More substantively, **all five fatal paths throw away the whole `SptScene`** — including every `TagEntry` already decoded — whereas the in-range-unknown-tag path, which is the *same* "we can no longer trust the byte stream" situation, records `tail_offset` and returns everything decoded so far.

## Evidence
The two sanity caps are correct in themselves — both bound the *byte* count, not the element count, and the array cap is computed in `u64` before any allocation. The asymmetry is only in the failure handling:

```rust
// parser.rs — in-range unknown tag: keep everything, record where we stopped
scene.unknown_tags.push((tag, tag_offset));
scene.tail_offset = tag_offset;
return Ok(scene);

// parser.rs — oversized array: discard the whole scene
return Err(io::Error::new(io::ErrorKind::InvalidData, format!(...)));
```

Re-verified 2026-08-30: the docstring still enumerates only the two conditions.

## Impact
Bounded. The cell route degrades an `Err` to `SptScene::default()` (#3078) and the loose route does the same (#3195), so nothing crashes and the placeholder still renders — the cost is losing tag `2000`/`4003` on a file the walker could have kept the head of, which only matters for a TREE record with no ICON (6 of 142 vanilla Oblivion records).

The doc-vs-code drift is the more durable cost: a future caller reading the contract will not expect `InvalidData` from a well-formed-but-large payload.

## Related
#3078, #3195 (the two degrade sites that currently absorb this).

## Suggested Fix
Either update the contract docstring to enumerate all five conditions, or — **preferably** — treat the two sanity-cap breaches the way an unknown tag is treated: record the offset into a diagnostic field, set `tail_offset`, and return `Ok`. Leave the magic mismatch and the mid-payload underflow fatal.

## Completeness Checks
- [ ] **SIBLING**: both degrade sites (#3078 cell route, #3195 loose route) must still behave correctly if the `Err` set narrows
- [ ] **TESTS**: a fixture with an oversized array payload asserting the head is retained rather than the scene discarded
