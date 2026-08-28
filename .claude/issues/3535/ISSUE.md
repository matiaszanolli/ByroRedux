# #3535 — SPT-2026-08-28-D5-01: tags 12002 / 12003 are the only FixedBytes dictionary entries with no corpus evidence recorded in format-notes.md

**Labels**: low, speedtree, terrain-exterior, tech-debt, doc-rot, documentation
**Filed from**: `docs/audits/AUDIT_SPEEDTREE_2026-08-28.md` (`/audit-publish`, 2026-08-28)

---

**Severity**: LOW
**Dimension**: Tag Dictionary
**Source**: `docs/audits/AUDIT_SPEEDTREE_2026-08-28.md` — SPT-2026-08-28-D5-01

**Location**: `crates/spt/src/tag.rs:128-131`, vs. `crates/spt/docs/format-notes.md:403-413`

## Description

Every other fixed-size dictionary entry carries a recorded corpus observation with a confidence
figure in the format-notes table — `8003`/`8005`/`8009` at `format-notes.md:405` ("fixed 52-byte
payloads, 100 % confidence"), `13008` at `:412` ("modal 11-byte payload"), `13013` at `:413`
("modal 7-byte payload"). The two `12xxx` entries do not:

```rust
// crates/spt/src/tag.rs:128-131
// 16 bytes — tag 12002 (4 × f32 = matrix row?).
12002 => SptTagKind::FixedBytes(16),
// 20 bytes — tag 12003.
12003 => SptTagKind::FixedBytes(20),
```

`grep -n "1200[0-9]" crates/spt/docs/format-notes.md` returns exactly one line — `:360`, which
lists `12000` and `12001` among the **bare** markers. `12002` and `12003` appear nowhere in the
observation log: no histogram, no confidence, no sample offset. The `(4 × f32 = matrix row?)`
gloss is an unsupported interpretation sitting in the same comment as the load-bearing size.

## Evidence

- The grep result above.
- `format-notes.md:342-414` (the "Recovered tag → payload-size table" and its
  `#### 52-byte fixed payload` / `#### Other notable tags` subsections, where every other
  `FixedBytes` entry is justified).
- `tag.rs:207-208` — the unit test pins the sizes but, being derived from the same source, cannot
  corroborate them.

## Impact

Documentation/evidence gap, not a demonstrated defect — the corpus gate passes at
100 % / 100 % / 96.46 %, so if either size were wrong in a way that desynced the walker on
vanilla content it would almost certainly already show as an extra unknown-tag bail. But the
gate only counts `Unknown` bails; a wrong-but-plausible size that happens to land on another
valid tag would pass it silently, and a wrong fixed size is exactly the Dimension-1 desync
trigger this dimension exists to spot-check. Under the project's No-Guessing policy, two
dictionary entries whose derivation cannot be reconstructed are a liability for the Phase 2 tail
decoder that will have to trust them.

## Related

- #1821 — the earlier format-notes byte-alignment correction.
- The `format-notes.md` 2026-05-09 dictionary table this omission sits in.

## Suggested Fix

Re-run `cargo run -p byroredux-spt --features recon --example spt_tagmap` (and
`spt_transitions`) over the three BSAs and add a `12002` / `12003` row to `format-notes.md`'s
payload-size table with the observed histogram and confidence, exactly as the `8003` / `13008` /
`13013` rows have. If the histogram does not support a single fixed size, demote the entries to
`Unknown` — a clean walker bail is the contract the placeholder relies on, and is strictly safer
than a size the log cannot justify. Drop or evidence the `matrix row?` gloss either way.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — every other `SptTagKind::FixedBytes` / `ArrayBytes` entry in `tag.rs` audited for a matching `format-notes.md` observation row
- [ ] **TESTS**: A regression test pins this specific fix (`tag.rs:207-208` updated to whatever the histogram supports, including a demotion to `Unknown`)
