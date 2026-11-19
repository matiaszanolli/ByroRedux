# M55 shaft and fog-seam measurements — 2026-11-15

Engine: release build of `d9d11dc34`, NVIDIA GeForce RTX 4070 Ti, driver
580.178.04. The capture harness changes were uncommitted. `BYRO_CLOUD_SEED=0`
was used for every run. These are gate results, not phase acceptance.

`docs/smoke-tests/exterior-shafts.sh` passed: the open contrast was 0.1536,
solid contrast 0.0022, and repeated-open contrast noise 0.000000. The
alpha-cutout hole mean was 129.16 versus 129.14 for the geometry-hole control
and 93.71 for the solid card; the cutout-to-geometry gap was under 0.1%,
within the 2% limit. All eight frames were 1280×720. Captures and logs:
`/tmp/byro-shaft-smoke.s1tGom`.

`docs/smoke-tests/fog-grid-seam.sh all`, using an isolated X display and
per-capture settings, passed on FNV and Skyrim. The largest distant-region
mean shift after subtracting repeat noise was 1.58 of 255 levels, below the
script's 2.5-level tolerance: FNV hills right -1.576 and Skyrim far ridge
+1.136. Repeat mean absolute pixel difference was 0.0108 levels in FNV and
less than 0.0001 in Skyrim; the FNV maximum pixel difference was 25.3, so
individual pixels are not deterministic even though regional means are.
Captures and logs: `/tmp/byro-fog-seam.LWopKT`.

The first seam run (`/tmp/byro-fog-seam.BpCBDS`) is invalid: its FNV far
frame resized to 1920×1043, while the fixed analysis regions assume
1280×720. The harness now rejects such frames, isolates settings and returns
skip rather than success if either game is absent from the combined run.
The tolerance remains an engine choice awaiting owner confirmation. This
result does not measure VLM-AP-001's distance-controlled ridge sweep, the
exterior content contrast, the interior coverage or the M-LIGHT gates.

On 2026-11-16, `docs/smoke-tests/interior-godrays.sh` passed on the same
release binary with an isolated X display: open versus sealed centre delta
45.8 and side delta 22.4 RGB levels; sky through the opening 127.3 versus
36.3 on the roof. Both captures were 320×180. Logs and captures:
`/tmp/byro-godray-smoke.x3HadD`. This is the A10 lab regression, not the A9
real-cell or door-transition coverage.

Also on 2026-11-16, the A9 FNV `NellisHangar1` door route passed after
exiting to `WastelandNV` and returning to the same interior. The run found
32 `SkyAperture` planes and 32 homogeneous volumes; its best pose had
15.66% of pixels above the day/night threshold, mean qualifying delta 25.3
levels, and repeat-noise p99 10.85 levels. Command:
`BYRO_INTERIOR_ONLY=NellisHangar1 BYRO_INTERIOR_ROUTE=door BYRO_CLOUD_SEED=0 docs/smoke-tests/interior-shafts-content.sh fnv`,
with `DISPLAY` and `WAYLAND_DISPLAY` unset so the harness started its own
Xvfb. Captures and `results.json`: `/tmp/byro-interior-shafts.4ATVAS`.
This is one cell and route; it does not establish the full A9 pass count or
the outdoor-border geometry evidence.

After the authored enable-table initialization fix described below, a fresh
NellisHangar1 door-route run also passed: best region 16.00%, mean qualifying
delta 25.21 levels, repeat-noise p99 10.77, with 32 `SkyAperture` and 32
homogeneous volumes. Results: `/tmp/byro-interior-shafts.Q7UEnW`.
In the follow-up two-cell door batch, NellisHangar1b passed with a 15.56%
best-pose region, mean qualifying delta 25.31 levels and repeat p99 10.85;
NellisHangar2 passed with an 85.12% region, mean qualifying delta 14.18 and
repeat p99 3.14. Both return doors led to the intended cells. Captures and
batch results: `/tmp/byro-interior-shafts.xydjjB`.

The corrected-build Nellis direct-route batch is recorded in
`/tmp/byro-interior-shafts.Caxyam`. NellisHangar1 passed with a 23.43%
best-pose region, mean qualifying delta 18.24 levels and repeat p99 5.42.
NellisHangar1b passed with a 16.50% region, mean delta 20.79 levels and
repeat p99 6.21. NellisHangar2 passed with a 14.55% region, mean delta 22.91
levels and repeat p99 9.85. All three therefore passed on both measured
routes on this build; this does not prove their apertures border outdoors.

FO4 `CambridgePD01` direct boot, 2026-11-16, also passed: 6 `LightShaft`
and 32 homogeneous volumes, Show Sky on; the best pose had 41.64% of pixels
above the day/night threshold, mean qualifying delta 16.8 levels and repeat
noise p99 6.72. Command: `BYRO_INTERIOR_ONLY=CambridgePD01
BYRO_INTERIOR_ROUTE=direct BYRO_CLOUD_SEED=0
docs/smoke-tests/interior-shafts-content.sh fo4` with an isolated X display.
Captures and `results.json`: `/tmp/byro-interior-shafts.a1k7c8`.

The same cell's door route passed on the same build and cloud seed:
`BYRO_INTERIOR_ONLY=CambridgePD01 BYRO_INTERIOR_ROUTE=door
BYRO_CLOUD_SEED=0 docs/smoke-tests/interior-shafts-content.sh fo4`, with an
isolated X display. It found 6 `LightShaft` and 24 homogeneous volumes (30
total), versus 6 and 32 (38 total) on direct boot. Its best sampled pose had
1.80% of pixels above the threshold, mean qualifying delta 11.9 levels and
repeat noise p99 3.21. The best poses were different entities, so the
41.64% and 1.80% areas are not a controlled brightness comparison. The
volume-count discrepancy confirms the earlier transition observation still
needs investigation. Door captures and `results.json`:
`/tmp/byro-interior-shafts.l1229y`. These runs predate the harness's addition
of stable named-entity counts, so they do not establish the size of the
entity-count difference.

A same-build inventory-only repeat records 7,500 named entities on direct
boot and 2,578 after the round trip (the door run also starts with 7,500).
Comparing volume name, profile, local centre and extinction identifies the
eight missing homogeneous volumes: two `chemistrysebottle01:4`, two
`mistmesh004:0`, and one each of `chemistrysetbeaker03:0`,
`glowmesh078:0`, `chemistrysetbeaker04:0` and
`chemistrysegraduatedcylinder01:2 - l2_details:2`. No new volume appeared.
Inventory JSON: `/tmp/byro-interior-shafts.QC8jdE/results.json` (direct),
`/tmp/byro-interior-shafts.ZqEXS2/results.json` (door). These counts are
named entities, not the engine's total entity count, and the mesh names do
not prove which placement references failed to load.

A further door-route inventory with `byroredux::cell_loader=info` recorded
2,095 reference entities at initial load and 1,526 on return. Both passes
read 2,172 placed references, skipped 768 precombined references, and made
1,042 NIF cache accesses. The count of references withheld by an XESP
enable parent rose from 1 to 506; 18 NPCs were reported spawned initially
and none on return. The inventory again found 30 volumes. Full log and
snapshot: `/tmp/byro-interior-shafts.xKpgte`. This establishes a major
enable-state difference but does not establish that every missing volume is
caused by it.

There is a relevant initialization-order asymmetry: the interior reference
loader consults `AuthoredDisabledReferences`, but that resource is installed
by `populate_scene_runtime` after the interior reference load completes.
On a first boot without that resource, enable-parent resolution lacks the
authored table; on return the table is already present. Thus the initial
inventory may include content that should have started disabled.

The working-tree fix stages the authored enable table before the first
interior reference pass and restores the previous table on cancellation. A
release build with that fix was checked on the same GPU and seed: the door
run recorded 2,578 named entities both before exit and after re-entry and
30 volumes after re-entry (`/tmp/byro-interior-shafts.XrNnnx`). A separate
direct boot also recorded 2,578 named entities and 30 volumes
(`/tmp/byro-interior-shafts.luPXyB`). Comparing volume name, profile, source,
local centre and extinction found no additions or removals. This confirms
that the observed inventory discrepancy was eliminated in this cell; it does
not establish every placement's correctness or the full M55 real-cell gate.
The previous direct-boot brightness result used the incorrect initial state.
Its corrected repeat passed VLM-INT-002: the best sampled pose had 18.48%
of pixels above the day/night threshold, mean qualifying delta 19.35 RGB
levels and repeat-noise p99 8.57; the camera remained fixed during each
capture pair. Captures and results: `/tmp/byro-interior-shafts.0iKKJb`.
The corrected door route also passed: the best sampled pose had 1.81% of
pixels above threshold, mean qualifying delta 11.94 levels and repeat-noise
p99 3.21; `/tmp/byro-interior-shafts.Gdxivs` contains the captures and
results. The direct and door routes sample different poses, so these region
fractions are not a controlled brightness comparison.

Further corrected-build FO4 door-route measurements were collected in
`/tmp/byro-interior-shafts.rXDB85`. ConcordMuseum01 passed with a 99.72%
best-pose qualifying region, mean delta 8.81 levels and repeat p99 2.00.
DBTechHighSchool01 passed with a 3.58% region, mean delta 9.99 levels and
repeat p99 3.07. zInstituteRotunda passed with a 100.00% region, mean delta
29.80 levels and repeat p99 2.00. UniversityPoint01 passed with a 22.25%
region, mean delta 15.89 levels and repeat p99 3.00. These in-scatter
results do not establish that each marked opening borders outdoors; the
geometry evidence remains open.

## Current-build recheck, 2026-11-18

Release build `9bbc76bc88a1`, NVIDIA GeForce RTX 4070 Ti, driver
580.178.04, cloud seed 0, isolated X displays. The interior lab passed
with a centre delta of 45.8 and a side delta of 22.4 RGB levels. The
exterior lab passed with open contrast 0.1536, solid contrast 0.0022,
and zero repeat contrast noise. Its alpha-cutout mean was 129.16 against
129.14 for the geometry-hole control.

The fog-grid seam gate passed on both FNV and Skyrim. The largest
regional difference past the repeat floor was 1.57 of 255 levels against
the 2.5-level gate. FNV's repeat maximum was 45.3 levels at individual
pixels, while its mean absolute repeat difference was 0.0134 levels.
Captures and logs are in `/tmp/byro-vlm-current.ZU5tPW`.

The exterior lab golden-frame test failed at the current build: 2.72%
of pixels differed by more than 8 levels, versus its 1% limit, and the
maximum channel difference was 72, versus its limit of 32. The
difference is concentrated in the sky and slat edges. A build at the
golden's original revision `f413cd15e` reproduced the stored image
within two levels. A build before cloud initialization `d9d11dc34`
differs from the current capture by at most one level, so the later
cloud initialization did not cause this change. The TAA silhouette
change `6926fddab` accounts for some edge differences, while its parent
already differs from the golden on 2.44% of pixels. A build before the
intentional reversed-Z depth change `09940503b` matches the stored image
within one level; its child produces the 2.44% difference. The golden
was refreshed for those changes after the owner accepted the current
shaft sequence. Independent reruns passed with cloud seeds 0 and 1. The original golden,
historical captures and logs are retained in
`/tmp/byro-vlm-current.ZU5tPW`.

The current TrinityTower01 direct-entry run found 12 `LightShaft`
volumes and 47 homogeneous volumes. Its strongest of four sampled poses
had 0.0335% of pixels above the day/night threshold, against the 0.1%
requirement, and repeat noise p99 was 2.79 levels. This is a failed A9
sample, with four poses and no door route measured. Results and captures
are in `/tmp/byro-vlm-current.ZU5tPW/trinity-direct`.

An expanded diagnostic sampled all 12 shaft volumes in the same cell.
Its best pose exceeded the unchanged threshold over 0.616% of the frame,
with a mean qualifying delta of 8.21 levels and repeat-noise p99 of 3.57.
All poses had a fixed camera and non-frozen captures. This shows the
four-pose sample missed a passing view. The diagnostic and its 36
captures are in `/tmp/byro-vlm-current.ZU5tPW/trinity-all-poses`.

The updated harness first takes the four-pose sample and, if it fails,
measures the remaining shaft or aperture volumes. Its Trinity direct
rerun passed on an additional pose with 0.128% of the frame above the
threshold, mean qualifying delta 9.40 levels and repeat-noise p99 4.28.
The result is close to the 0.1% bar. The fixed-camera and frozen-frame
checks passed for all 12 poses. Results and captures are in
`/tmp/byro-vlm-current.ZU5tPW/trinity-adaptive`. This checks direct entry
and does not prove that its volumes border outdoor openings.

A current-build direct-entry batch passed the other four FO4 cells
measured here. The best qualifying regions were 88.63% at
`ConcordMuseum01`, 3.28% at `DBTechHighSchool01`, 100.00% at
`zInstituteRotunda` and 26.75% at `UniversityPoint01`. Repeat-noise p99
was 6, 4, 2 and 4 levels respectively. The 4/4 batch used four poses per
cell, and its results and captures are in
`/tmp/byro-vlm-current.ZU5tPW/fo4-direct`. Cambridge also passed its
current-build direct run, with a 41.74% qualifying region and repeat-noise
p99 of 6.79 levels. Its results are in
`/tmp/byro-vlm-current.ZU5tPW/cambridge-direct`. Together with Trinity,
the current-build FO4 direct-entry count is 6/6. These direct runs do not
establish the opening geometry.

The FNV direct-entry batch also passed all three Nellis hangars. Their
best qualifying regions were 22.96%, 17.85% and 14.50% for
`NellisHangar1`, `NellisHangar1b` and `NellisHangar2` respectively.
Repeat-noise p99 was 5.42, 5.92 and 9.63 levels. Each cell carried 32
marked apertures. Results and captures are in
`/tmp/byro-vlm-current.ZU5tPW/fnv-direct`. The current-build direct
count for listed shaft cells is FNV 3/3 and FO4 6/6.

The three Nellis hangars also passed the current-build door round trip,
at 16.02%, 16.20% and 85.83% qualifying regions respectively. Their
repeat-noise p99 values were 10.77, 10.77 and 3.14 levels. The
destination-selected transitions, results and captures are in
`/tmp/byro-vlm-current.ZU5tPW/fnv-door`.

The five FO4 cells with exterior door routes passed the same round trip.
Best qualifying regions were 99.78% at Concord Museum, 3.49% at DB
Technical High School, 100.00% at the Institute rotunda, 1.70% at
Cambridge Police Station and 15.84% at University Point. Repeat-noise
p99 was 2.00, 3.79, 3.00, 3.72 and 4.00 levels respectively. Each
cell's named-entity count matched before and after the round trip.
Results and captures are in `/tmp/byro-vlm-current.ZU5tPW/fo4-door`.
Trinity Tower also passed on a separate door run: it exited to
Commonwealth `(0,-6)` and returned to `TrinityTower01`. Its best region
was 0.241%, repeat-noise p99 was 2.21 levels, and its named-entity count
was 6,567 both before and after the round trip. Results and captures are
in `/tmp/byro-vlm-current.ZU5tPW/trinity-door`. This resolves the
earlier no-route observation for this run. The current-build door count
for listed shaft cells is FNV 3/3 and FO4 6/6.

For VLM-SH-007, the current exterior lab produced 12 finished frames
at 15-frame intervals with a pinned camera and frozen clock. Over the
wall body (rows 335–519 and columns 375–904), the mean absolute change
between consecutive frames ranges from 0.609 to 2.290 RGB levels.
The 4 fps review animation is
`/tmp/byro-vlm-current.ZU5tPW/shaft-review.gif`, and its source frames
and log are alongside it. The owner accepted the sequence on 2026-11-18.

The hardened exterior content harness completed the FNV fixture at all
three hours. Its default camera points into a nearby slope, so those
frames cannot establish contrast across a named occluder. A level camera
at `2048,8556,-1848`, facing `0,0,-1`, shows a building and fence
above the slope. The captures are in
`/tmp/byro-vlm-current.ZU5tPW/exterior-fnv-level`. The scene still
needs a depth-matched lit and shadowed sample before VLM-SH-006 can be
judged. A second camera 500 units higher exposes more of the building
and fence in `/tmp/byro-vlm-current.ZU5tPW/fnv-pose-probe/elevated.png`;
the screenshot alone does not establish matched depths.
