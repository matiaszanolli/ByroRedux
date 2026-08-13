# REN-D16-03: memory-budget.md says bloom pyramid isn't FIF-doubled; BloomPipeline actually allocates one full pyramid per frame-in-flight

- **Severity**: MEDIUM
- **Dimension**: 16 — Bloom / Memory-Lifecycle
- **Location**: `docs/engine/memory-budget.md` ("### Bloom" section + the "Bloom pyramid" row of the VRAM Rough Budget table); `crates/renderer/src/vulkan/bloom.rs` (`BloomFrame`, `BloomPipeline::frames`, `BloomFrame::new`)
- **Description**: The doc says the pyramid is "recomputed every frame with no history — not FIF-doubled, unlike everything else on this page" and budgets ~3.5 MB at 1080p / ~13.8 MB at 4K. The code allocates `MAX_FRAMES_IN_FLIGHT` independent `BloomFrame`s. Being per-FIF is in fact required: `dispatch()` rewrites descriptor bindings and mips with no pre-barrier, sound only because each slot's images are exclusive and fence-gated (#931's rationale).
- **Evidence**: `bloom.rs` — `for frame_idx in 0..MAX_FRAMES_IN_FLIGHT { … partial.frames.push(frame); }`, descriptor-pool sizing multiplied by `MAX_FRAMES_IN_FLIGHT`. Byte math at 1920×1080 render extent, B10G11R11_UFLOAT_PACK32 (4 B/px): ≈5.52 MB per FIF → ~11.0 MB (doc: ~3.5 MB). At 3840×2160: ≈22.1 MB per FIF → ~44.1 MB (doc: ~13.8 MB).
- **Impact**: ~3.2× understatement on both budget rows. Small in absolute terms but a wrong invariant in a document audits treat as authoritative — "not FIF-doubled" could wrongly justify collapsing `frames` to one pyramid, reintroducing the cross-frame WAR #931's barrier reduction depends on being absent.
- **Related**: #1872 (added the row), #931 (per-FIF-exclusivity argument), REN-D5-02 / REN-D16-02 (sibling drift in the same doc), #2679.
- **Suggested Fix**: Correct the section to "one pyramid per frame-in-flight (required for #931's barrier reduction)", double the two figures, update the VRAM Rough Budget row.

## Completeness Checks
- [ ] Doc-only fix, no code change required

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2801
