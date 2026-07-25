# 2167: PERF-D4-02: memory-budget.md scene-buffer table omits ~73 MB of resident scene SSBOs (52% undercount)

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/2167
**Labels**: bug, medium, performance

---

## Severity
MEDIUM

## Dimension
SSBO Sizing & Per-Frame Upload (Dim 4) — `/audit-performance` 2026-07-25

## Location
`docs/engine/memory-budget.md:15-28`; actual allocations at `crates/renderer/src/vulkan/scene_buffer/buffers.rs:437-590`

## Description
The authoritative scene-buffer table claims "~140 MB across all copies". Three buffer families allocated by `33d9a468` are missing entirely, and the bone footnote undercounts by a factor of ~2.7x: the doc's footnote claims "3 x 12.6 MB ~= 37.8 MB" for the bone family; the real bone family is **eight** 12.58 MB allocations ~= 100.6 MB (palette x2 FIF, host-visible staging x2 FIF, device-copy x2 FIF, persistent bind-inverses, bind-inverse upload staging).

## Evidence

| Buffer | Per-FIF | Total | In doc? |
|---|---|---|---|
| instance | 29.36 MB | 58.72 MB | yes |
| previous_model (new, `33d9a468`) | 16.78 MB | 33.55 MB | NO |
| indirect | 5.24 MB | 10.49 MB | yes |
| material | 4.92 MB | 9.83 MB | yes |
| bone_device + staging + device-copy (x2 FIF each) | 3 x 12.58 MB | 75.5 MB | folded into a wrong footnote |
| bind_inverses_persistent | -- | 12.58 MB | NO |
| bind_inverse_upload_staging | -- | 12.59 MB | NO |

**Actual total ~= 213.4 MB. Documented ~= 140 MB.**

## Impact
`memory-budget.md` is the cited authority for VRAM planning against the 6 GB RT-minimum target and is explicitly named in `_audit-common.md` as "prefer over re-deriving facts from source" — every downstream headroom calculation inherits the 73 MB undercount. A future `MAX_INSTANCES` bump would silently cost 1.75x what the table implies (112 B + 64 B per slot, not 112 B), because the previous-model SSBO scales with the same constant invisibly.

## Related
Same failure mode as closed #1814 (ReSTIR reservoirs absent) and closed #1872 (denoiser images absent) — both fixed by adding rows. Also PERF-D3-01 and PERF-D3-02 (both filed separately, same root cause: doc hasn't tracked the last two sessions of SSBO additions — recommend fixing in one documentation pass).

## Suggested Fix
Add rows for `previous_model`, `bind_inverses_persistent`, and `bind_inverse_upload_staging`; split the single "Bone-palette SSBO" row into the three real per-FIF bone buffers; correct footnote 1 and the ~140 MB total to ~213 MB. Consider a `scene_buffers_total_bytes()` accessor pinned by a test so the doc figure can't drift again.

## Completeness Checks
- [ ] **TESTS**: Add a `scene_buffers_total_bytes()` accessor pinned by a test so the doc figure can't silently drift again
