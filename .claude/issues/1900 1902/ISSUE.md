# #1900: NIF-D3-02 — Per-game clean-rate matrix is stale-low; Starfield parse_real_nifs floors erode with it

**Severity**: low
**Dimension**: NIF audit 2026-07-06 (nif-deep suite)
**Location**: `docs/engine/nif-parser.md` § "Per-game NIF coverage"; `docs/engine/game-compatibility.md`;
`crates/nif/tests/parse_real_nifs.rs` (Starfield min_clean floors)

## Description
Both reference matrices understate the live parser by 2-4 points per
affected game (Oblivion doc 96.24% vs live 99.93%; FO4 doc 96.46% vs live
100%; FO76 doc 97.34% vs live 100%; Starfield Meshes01 doc 97.21% vs live
100%). The FO4 floors were correctly refreshed to 0.995 (#1457); the
Starfield floors were not — `Meshes01.ba2` sits at `min_clean: 0.970`
against a live 100%, ~3 points of unguarded regression headroom.

## Suggested Fix
Refresh both matrices from a live 7-game sweep and tighten the five
Starfield `min_clean` floors to match FO4's #1457 treatment (live-measured
minus ~0.5% margin). Update Oblivion's "~149 NetImmerse files" note to
"6 v3.3.0.13 marker files".

## Note
Live game data was available on this machine, so a full 7-game sweep was
run rather than just documenting the gap. The sweep also surfaced that
`ROADMAP.md`'s "authoritative" Fallout 76 row and an Oblivion
recoverable-rate footnote were themselves stale (same 97.34%/99.99% drift),
plus two further docs (`architecture.md`, `starfield-esm-phase0-baseline.md`)
citing the same stale figures — all fixed in the same pass after confirming
scope with the user (7 files touched, above the skill's 5-file threshold).

---

# #1902: NIF-D6-01 — BhkMultiSphereShape::parse fills Vec<[f32;4]> with a per-element push loop

**Severity**: low
**Dimension**: NIF audit 2026-07-06 (nif-deep suite; allocation hygiene)
**Location**: `crates/nif/src/blocks/collision/shape_primitive.rs:58-65`

## Description
The sphere array is a contiguous run of `[f32;4]` (cx,cy,cz,r) with no
per-element transform — byte-identical to what `read_ni_color4_array` /
`read_pod_vec::<[f32;4]>` produces. The code did
`allocate_vec::<[f32;4]>(num_spheres)` then a per-element loop of four
`read_f32_le()` + `push`, the "direct allocate-then-loop-and-fill" shape
the `read_pod_vec<T>` discipline (#833/#873) was meant to collapse for POD
arrays.

## Suggested Fix
`let spheres = stream.read_ni_color4_array(num_spheres as usize)?;` — one
`read_exact` instead of N per-element reads. Byte-for-byte equivalent on
the LE host the crate already requires.
