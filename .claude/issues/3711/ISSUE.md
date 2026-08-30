# #3711: NIF-2026-08-30-D1-01: the #395 sizeless-stream drift detector fires 4,280 times on an Oblivion corpus with zero real drift — 100% false-positive rate

**Labels**: bug, nif-parser, medium, nif, game:oblivion
**Filed**: 2026-08-30 (audit-publish)

---

**Report**: `docs/audits/AUDIT_NIF_2026-08-30.md` · **Severity**: MEDIUM · **Dimension**: Stream Position
**Game affected**: Oblivion (`bsver <= 11`) — every `no_block_sizes` file; by construction it cannot fire on the block-sized games.

## Location
- `crates/nif/src/lib.rs` — `drift_warning` (currently `:917`), armed at `:524-543`

## Description
The `no_block_sizes` path has no header-driven recovery anchor, so #395 added a heuristic drift detector as its only guard: for each successfully parsed block it compares consumed size against prior parses of the same type in the same file and warns when the new value is more than 2 bytes from every prior.

The heuristic keys on a property that is **not stable** for most Oblivion block types — consumed size varies legitimately with embedded string length and child/property counts. Its `max - min > 2` variance escape hatch is evaluated over the priors only, so a type whose first two instances happen to agree arms the check for every later instance that does not.

## Evidence
Debug build (the detector is `#[cfg(debug_assertions)]`), full Oblivion corpus of 9,612 NIFs across all nine archives:

```
4,280 "Stream drift suspect" warnings
    0 truncations, 0 hard failures, 0 recovered blocks, 0 real-parser drift
```

Top emitters are exactly the variable-length types: `NiSourceTexture` 1,187 · `NiTriStrips` 1,094 · `NiMaterialProperty` 914 · `NiNode` 342 · `NiTriShape` 119. Two representative warnings:

```
block 187 'NiSourceTexture' (offset 203754) consumed 68 bytes,
  but 5 prior parse(s) of this type all consumed 72±1 bytes (median 72)
block  38 'NiMaterialProperty' (offset 355279) consumed 75 bytes,
  but 3 prior parse(s) of this type all consumed 79±2 bytes (median 80)
```

A 4-byte swing on `NiSourceTexture` is one texture path four characters shorter than its siblings; a 4-5 byte swing on `NiMaterialProperty` is the block name. Both are correct parses. **False-positive rate on this corpus: 100%** (4,280 warnings, 0 true positives).

## Impact
The one instrumentation surface guarding the only game where a parser drift cascades silently is unusable as shipped — ~0.45 warnings per file, so anyone who enables it learns to ignore the bucket, which is precisely what would hide a real drift. Defence-in-depth gap, not live corruption: no vanilla Oblivion file currently drifts.

## Related
#395, #324 (the sibling sizeless-recovery mechanism sharing `parsed_size_cache`). The companion coverage gap on the same sizeless path is filed alongside this one (D3-01).

## Suggested Fix
Key the detector on something invariant rather than total consumed size — the natural candidate is a per-type *fixed-field* byte count (consumed minus the summed length of the variable-length fields the parser already read), which the parser knows and the heuristic does not. Failing that, restrict it to types whose on-disk size genuinely is constant and require `prior.len() >= 5` before arming, then assert a count of 0 against this 9,612-file corpus in a test.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the `parsed_size_cache` sizeless-recovery path shares this data)
- [ ] **TESTS**: A regression test pins this specific fix — assert 0 warnings over the Oblivion corpus
