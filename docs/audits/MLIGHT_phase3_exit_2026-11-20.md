# M-LIGHT Phase 3 exit: A11 re-anchor and A13 attribution, 2026-11-20

Follow-up to [the 2026-11-18 evidence](MLIGHT_phase3_exit_2026-11-18.md), under
the owner decisions of 2026-11-20 recorded in PRD revisions 40 and 41. RTX 4070
Ti (580.178.04), release builds, the Phase 1b poses and settled tiers
([MLIGHT_phase1b_baselines_688f42705.tsv](MLIGHT_phase1b_baselines_688f42705.tsv)),
TAA without jitter, 64 frames per anchor, seeds 0 and 1, 32 × 32 tiles with
edge tiles, margin 0.01.

## A11: what moved the content anchors

The full-phase A11 of 2026-11-18 failed against the Phase 3 entry anchors in
every scene. Each outside cause below was found by a `git bisect` between the
preserved bisect builds, judged on the scene's own anchor capture.

| Cause | Commit | Effect on the anchors | Disposition |
|---|---|---|---|
| Light animation seeded by `EntityId`; #6657's boot-time scratch entity re-seeded every flickering light | `a290c755f` | Whiterun 840 → 352 and Dugout 600 → 152 tiles once both sides are seeded by position | Fixed by #7091 (position seed), on both sides of the comparison |
| #7034: DDS alpha presence recorded at reservation, so first-load normal-map alpha applies | `73bfed85c` | Prospector mean luminance 0.006925 → 0.006003 (−13 %), every surface | Accepted by the owner as an intended outside fix |
| #6775: the bench applies its pinned pose as yaw / pitch | `a1316abc2` | Cornell 167 → 486 tiles, a sub-pixel view shift on every edge | Accepted by the owner as an intended outside fix |
| #6725: the authored fog power enters the legacy-ramp fit (VLM-HF-001) | `c306b0dc7` | Prospector 0.007247 → 0.006924 (−4.5 %) | Same class as the two above (another PRD's intended change); recorded in rev 41 |

The re-anchor control is the Phase 3 entry `75e4f511a` with #7091, a port of
#7034's reservation-time `has_alpha` (its 787-line commit does not cherry-pick
across six intervening refactors of `upload.rs`), #6775, and #6725's fog code.
The head is main `47ae285c6` with #7091 and PAF-012.
[Per-scene table](MLIGHT_phase3_A11_reanchor_2026-11-20.tsv):

| Scene | Tiles | Fixed entry, undeclared | Re-anchor control, undeclared | Re-anchor control, Phase 3 declarations |
|---|---:|---:|---:|---:|
| Cornell | 789 | 486 | 158 | 9 |
| Prospector | 545 | 522 | 84 | 84 |
| Whiterun | 920 | 352 | 39 | 29 |
| MedTek | 920 | 44 | 46 | 46 |
| Dugout | 851 | 152 | 147 | 145 |

What remains is not shaped like a Phase 3 mechanism (a shadow-edge or
penumbra change):

- Whiterun: almost all of it is one NPC in a different pose, plus one small
  patch; Phase 3 changes no animation.
- Dugout: small spots at light fixtures, up to −12 % in a tile.
- MedTek: a uniform +1 to +2 % over its 46 tiles.
- Prospector: 84 tiles whose excess over tolerance is a median 0.0026 (mean
  changes within ±0.1 %).
- Cornell: 9 tiles beyond the SHD-012 and far-sun declarations.

These were not bisected further. They lie in the 690 outside commits between
the entry and main, which carry 88 renderer changes.

## A13: Phase 3's own cost

The phase-exit A13 against the entry binary
([full matrix](MLIGHT_phase3_A13_paf012_2026-11-20.tsv), three interleaved runs)
keeps every settled tier and fails four wall-time cases: Prospector TAA +0.62
(ceiling 0.49), Whiterun TAA +0.84 (0.68), Whiterun FSR +0.65 (0.48), Dugout TAA
+0.83 (0.78). MedTek reads −1.57 and −0.70 ms over the same span, which is how much
outside work the wall delta carries.

[Bracket attribution](MLIGHT_phase3_A13_attribution_2026-11-20.tsv), three
interleaved runs, entry against main + #7091 without and with PAF-012:

| Case | `shadow_filter` without PAF-012 | `shadow_filter` with PAF-012 | `far_sun` | `main_render` | Q8 ceiling |
|---|---:|---:|---:|---:|---:|
| Prospector TAA, tier 0 | +0.240 | +0.009 | 0 | +0.66 to +0.94 | 0.488 |
| Dugout TAA, tier 0 | +0.311 | +0.008 | 0 | +1.16 to +1.27 | 0.776 |
| Whiterun TAA, tier 1 | +0.195 | +0.193 | 0 | +0.55 to +0.61 | 0.681 |
| Whiterun FSR, tier 3 | +0.249 | +0.248 | 0 | +0.07 to +0.40 | 0.483 |

- **PAF-012** (no blocker search at tier 0,
  [design](../engine/penumbra-aware-shadow-filter.md#34-filtering)) returns the
  filter to its entry cost at tier 0, where Prospector and Dugout settle under
  TAA. Tiers 1 to 3 keep the search, and its cost there is inside the ceiling.
- **The bracket Phase 3 adds**, `far_sun`, reads 0 in these interiors.
- **`main_render`** grows by 0.1 to 1.3 ms. Phase 3's own main-pass changes
  were each measured in isolation and found within the ceiling: SHD-012 in rev
  31 (7-run Whiterun FSR −0.04 ms) and the A3 sampling key in rev 35. The rest
  is the outside commits, with the earlier
  [bisect](MLIGHT_phase3_A13_bisect_2026-11-20.tsv) spreading it across ranges
  that hold no M-LIGHT code.
- **Wall-time noise.** The two head builds run identical code at Whiterun's
  tier 1, yet their wall deltas differ by 1.3 ms (+0.99 against +2.29), so a
  3-run wall delta cannot resolve a 0.5 ms ceiling here.
