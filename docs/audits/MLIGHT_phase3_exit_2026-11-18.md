# M-LIGHT Phase 3 exit, 2026-11-18

The far-field sun pass is implemented for RLT-CON-004. Its compute
dispatch uses the current opaque depth to estimate how much sunlight to
remove from an exterior receiver beyond the traced fade. The receiver's
sun radiance and the FSR transparency mask occupy separate channels of a
single attachment. The pass has its own `gpu_far_sun` timestamp.
[Renderer integration](../engine/shadow-pipeline-tradeoffs.md#far-field-sun-shadows-rlt-con-004)

## Far-field validation

Release captures on the RTX 4070 Ti (580.178.04), 1280 × 720, tier 3,
TAA without jitter, 16 frames, seed 0, with Vulkan synchronization
validation enabled. The `far-sun` Cornell rung contains a low sun,
an opaque caster and a blue fill light. A changed pixel loses more than
0.001 in at least one linear RGB channel.

| Case | Comparison | Darkened pixels | Peak linear loss |
|---|---|---:|---:|
| Caster inside the traced distance | `filtered_direct` vs. all direct shadows disabled | 21,623 | 0.11542 |
| On-screen caster beyond the traced distance | `filtered_direct` vs. `direct_only` | 422 | 0.11542 |
| Beyond the traced distance, shadows disabled | `filtered_direct` vs. `direct_only` | 0 | 0 |
| Off-screen caster beyond the traced distance | `filtered_direct` vs. `direct_only` | 0 | 0 |

The 422 affected far pixels had no channel gain above 0.001. Of these,
191 retained blue radiance above 0.01. The far fixture's median
`gpu_far_sun` sample was 0.018 ms. No Vulkan validation errors appeared
in the seven captures. FSR Quality also ran with synchronization
validation enabled on MedTek.
All six rows pass when the release CLI runs the fixture as a gate.
[CON-004 manifest](MLIGHT_phase3_CON004_2026-11-18.tsv)

## Reference gates

The [A2/A4/A5 manifest](MLIGHT_phase3_A2A4A5_2026-11-18.tsv) is from the
final release build on the same GPU, with 64 engine frames and two seeds.
Mitsuba 3.9.1 used 1,024 reference samples per pixel for the local-light
penumbra and 16,384 for the sun and contact rungs. A2 reports maximum
width errors of 0.025% for the local light and 0.416% for the sun. All
eight judged A4 rows and all ten judged A5 rows pass. The A5 plank reads
0.00214966 mean and 0.0100475 p95 against limits of 0.00216 and 0.0113.
V9 reproduces the animated proxy mesh and its frame-45 yaw.
[Earlier A3 and A14 release measurements](MLIGHT_A3_exit_2026-11-18.tsv)

## A11 content

The original Phase 3 entry anchors were recovered from `75e4f511a`'s
local release worktree. The five TAA poses and tiers came from the Phase
1b manifest. Each capture averaged 64 frames, with the entry's seed 0 and
seed 1 setting the noise floor. The comparison uses 32 × 32 tiles, with
partial edge tiles included and the 0.01 margin unchanged.

The preserved entry executable reproduced all five archived entry
captures with zero tiles outside tolerance. The Phase 2 exit captures,
recorded after `a89ca367e`, already differ from those entry anchors.
Comparing the saved seed-0 images with the entry's frozen two-seed
floor gives 331 / 545 tiles outside tolerance in Prospector,
609 / 920 in Whiterun and 384 / 851 in Dugout. Cornell and MedTek
both have zero. This comparison uses the undeclared 0.01 margin.
[Phase 2 capture metadata](MLIGHT_phase2_exit_metadata.txt)

The November 6 `a33220cf4` capture retained RLT-SHD-012 and reverted
RLT-SHD-003 and RLT-SHD-007. It already differed from entry across 55
Prospector, 818 Whiterun and 446 Dugout tiles after applying the existing
SHD-012 declaration. Those differences were present with the earlier
filter reach and sun radius.

In Whiterun, the mean of the 64-frame `direct_only` capture drops
from 0.033443242 at entry to 0.029022601 at `a33220cf4`. The mean
of `indirect_only` drops from 0.198224893 to 0.197415496. Repeating
the direct captures with direct shadows disabled gives 0.051386428
and 0.044272247, a 13.84% decrease. This locates the broad early
darkening in the unshadowed direct term.

| Scene | Far-sun change vs. immediate control | Pre-far-sun vs. Phase 3 entry | Final vs. Phase 3 entry |
|---|---:|---:|---:|
| Cornell | 0 / 789 | 328 / 790 | 328 / 790 |
| Prospector | 0 / 545 | 521 / 545 | 521 / 545 |
| Whiterun | 0 / 920 | 840 / 920 | 840 / 920 |
| MedTek | 0 / 920 | 44 / 920 | 45 / 920 |
| Dugout | 0 / 851 | 599 / 851 | 600 / 851 |

The isolated comparison passes all five scenes. Both phase-entry columns
use the existing SHD-012 declaration and fail A11. The differences need
attribution under RLT-OPS-004 before the Phase 3 exit can pass.

## A13 cost and quality tiers

The [full matrix](MLIGHT_phase3_A13_2026-11-18.tsv) compares the far-sun
build against the preserved Phase 3 entry executable at `75e4f511a`.
The release candidate's SHA-256 begins `c1497ad41d98` and the entry
control's begins `d3e97db554db`. The matrix predates the blocker-width
cache experiment. It uses Wayland mailbox presentation, 300 stepped
orbit frames per timing sample, the Phase 1b ray tier pinned, and the
median of three interleaved runs. The adaptive run counts the 300 frames
after a 240-frame warm-up. Cornell is report-only under Q8.

All ten cases meet their baseline settled ray tier. Three game cases
exceed their cost allowance, all in TAA.

| Scene | Observed increase (ms) | Q8 allowance (ms) |
|---|---:|---:|
| Prospector | 0.660 | 0.488 |
| Whiterun | 0.780 | 0.681 |
| Dugout | 0.780 | 0.776 |

A trial that cached blocker widths in workgroup memory raised the
Prospector `shadow_filter` median from 0.538 to 0.897 ms and the wall
median from 10.10 to 10.37 ms in three interleaved runs. The final
working tree uses the original blocker-width reads.

The [far-sun-only retest](MLIGHT_phase3_far_sun_cost_2026-11-18.tsv)
uses the pre-far-sun A3 release candidate as its control. The same three
TAA cases increased by 0.160, 0.340, and 0.570 ms respectively. Each
increase is inside its case's Q8 allowance. The phase-entry comparison
still exceeds budget in the cases shown above.
