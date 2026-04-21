# Investigation — Issue #489 (FNV-5-F3)

## Domain
Docs / perf baseline — `ROADMAP.md`, `byroredux/src/main.rs` (bench flag already exists).

## Findings

### `--bench-frames` already exists and does what #489 asked for

Existing `--bench-frames N` flag (`byroredux/src/main.rs:47-54,518-555`) already:
- Runs N frames headless after cell load
- Prints `bench:` line with: frames, avg_fps, min/max_fps, avg/min/max_ms, entities, meshes, textures, draws
- Exits cleanly

No new flag needed. The audit author missed its existence.

### Ran actual bench on FNV Prospector Saloon

`cargo run --release -- --esm FalloutNV.esm --cell GSProspectorSaloonInterior --bsa 'Fallout - Meshes.bsa' --textures-bsa 'Fallout - Textures.bsa' --bench-frames 300` at commit **bee6d48** (2026-04-20):

```
bench: frames=300 avg_fps=251.6 min_fps=154.3 max_fps=300.8
       avg_ms=3.97 min_ms=3.32 max_ms=6.48
       entities=1200 meshes=777 textures=208 draws=773
```

### Deltas vs ROADMAP's stale numbers

| Metric | ROADMAP claim | Actual (bee6d48) | Δ |
|---|---|---|---|
| avg_fps | 48 (pre-M37.5) / 85 (post-lighting) | **251.6** | +5.2× / +3.0× |
| entities | 809 | **1200** | +48% |
| textures | 199 | **208** | +5% |
| draws | 784 | **773** | ~flat |
| avg_ms | ~21 / ~12 | **3.97** | -5.3× / -3.0× |

Entity count jumped because post-M18 record coverage (MSTT/FURN/DOOR/ACTI/CONT/LIGH/ACHR) catches more placements that previously fell through.

FPS jump is real: M31.5 RIS + M36 BLAS compaction + M37 SVGF + M37.5 TAA all paid for themselves while also batching BLAS builds and collapsing duplicate ESM parses (#360/#374/#381).

### Stale references in ROADMAP

Multiple ROADMAP lines point at #456 / #457 as "pending re-bench" but both closed as FO3 audit cleanup without actually running a bench. Need to replace with concrete bee6d48 numbers.

## Fix

1. Update `ROADMAP.md:27-30` Prospector Saloon line with actual numbers + commit anchor.
2. Update `ROADMAP.md:52-54` frametime baseline.
3. Update `ROADMAP.md:403` entity count (809 → 1200).
4. Update `ROADMAP.md:444-447` M22 result with the actual number.
5. Drop stale `#456` / `#457` references; replace with the bee6d48 bench line.
6. Update `ROADMAP.md:33-38` FO3 Megaton claim with a note that it needs the same treatment (separate follow-up — data not re-captured here).

## Scope
1 file. No code. No tests affected.
