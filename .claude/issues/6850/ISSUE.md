# #6850: Cross-cell actor travel — tracking (PRD cross-cell-actor-travel)
State: OPEN
Labels: enhancement ai game:fnv save-load 

Tracking issue for the APPROVED PRD [Cross-cell actor travel](https://github.com/matiaszanolli/ByroRedux/blob/main/docs/prd/cross-cell-actor-travel.md) (approved by the owner 2026-11-11; decisions Q1–Q3 taken as recommended).

An actor whose package or script destination lies in another space (an interior cell, or another worldspace) gets there: through the load door while watched, offscreen while not, and stays in its new space across cell loads and saves. Found by the FNV new-game route: VCG02 stage 10 sends `SunnyRef` (0x00104E85) out of `GSProspectorSaloonInterior` to `VCG02SunnyOutsideMarkerREF` (0x0010A201), which the engine cannot do today.

## Phases
- [ ] Phase 0: #6851 — census, verify items, design (no engine code)
- [ ] Phase 1: #6852 — relocation ledger, cross-space MoveTo, logical identities
- [ ] Phase 2: #6853 — watched travel through one load door (VCG02 Sunny)
- [ ] Phase 3: #6854 — offscreen arrival, multi-hop, followers, census sweep

