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

## FNV depth-matched pose investigation, 2026-11-19

A release build based on `549c2dd3b` with uncommitted workspace changes
used cloud seed 0, native TAA without jitter, a static camera at
`2048,9056,-1848` facing `0,0,-1`, and the FNV `WastelandNV` grid
`0,0`. The `depth.sample` readback reports view-axis depths of
3,419.486 and 3,416.643 world units at image coordinates `(640,240)`
and `(720,240)` respectively. The two depth values come from the same
1280 × 720 capture. The corresponding 7 × 7 regions in 64-frame
volumetric-radiance EXRs have linear luminances 0.002643 and 0.002613
at 07:00, and 0.001203 and 0.001160 at noon. The exponential debug-view
mapping was inverted per channel before taking luminance.

Sampling all 49 pixels in each region rejects this pair: the first
region spans 3,403–3,460 world units, while the second spans
3,149–3,657. The matching center pixels concealed a foreground or
background depth discontinuity. The occluder has not been identified,
and this is not a VLM-SH-006 pass. Captures, both readbacks and logs are
in `/tmp/byro-vlm-depth.agreem`.

The same pose was then captured with and without exterior froxel sun
occlusion, using the same release binary and 64-frame radiance EXRs at
07:00. The comparison switch changes neither surface shadows nor
interior visibility. After inversion of the exponential debug mapping,
the 7 × 7 regions centered at `(640,240)` and `(720,240)` gained
0.000090 and 0.000081 linear luminance respectively when occlusion
was disabled; their ordinary on-state values were 0.002626 and
0.002596. Both regions therefore have similar occlusion response as
well as the previously noted depth mismatch: they do not identify a
shaft edge. A named caster remains unverified.

As a control, the same switch on the exterior shaft lab increased mean
linear luminance over its wall region (rows 335–519, columns 375–904)
by 0.027; the whole-frame mean change was 0.00817. A Show Sky interior
lab captured after the host restriction was added had bit-identical
on/off EXRs (zero differing pixels). The first version of the switch
also affected Show Sky interiors; that defect was corrected before the
control was repeated. The captures and `/tmp/byro-vlm-depth.agreem/occlusion-ab.sh`
are under `/tmp/byro-vlm-depth.agreem/occlusion-ab`. These are diagnostic
measurements, not VLM-SH-006 or VLM-SH-011 passes.

### Full-field follow-up, 2026-11-19

`depth.dump` now exports the renderer's entire depth readback as a
single-channel PFM. On release binary SHA-256
`de7fa2524c3e4c084132fbf3587619fa3943730bf8ed325e8fe3fdf4e4d24dfc`,
at the same FNV pose, its values at `(640,240)`, `(720,240)` and
`(200,400)` agree with separately captured `depth.sample` values to the
reported 0.001 world unit. The exported field contains 264,978 background
pixels and no invalid samples. It also reproduces the two rejected 7 × 7
depth ranges above. The readbacks are not synchronized to the radiance
frames; the check used a held static camera and paused world systems.

Searching this field for locally smooth patches identified two 7 × 7
regions on the distant wall at `(787,283)` and `(773,304)`. Their depth
ranges are 3,396.7–3,416.2 and 3,352.0–3,371.0 world units respectively
(their means differ by 1.3%). At 07:00, 64-frame radiance captures on the
same binary measured linear luminance 0.002769 and 0.002946 with sun
occlusion on, and 0.002931 and 0.002957 with it disabled. A repeated
occlusion-on capture was bit-identical. The locations were selected using
an earlier binary's difference field and verified on this binary, so this
is a diagnostic candidate rather than an independent acceptance test.
The depth capture and radiance captures are separate runs, and the object
casting the apparent differential occlusion is not yet identified. Neither
the named-occluder requirement nor the noon and floor-on/off requirements
are satisfied by this result. The depth field is in
`/tmp/byro-vlm-depth.agreem/full-field`, and captures, logs and an annotated
view are in `full-field-ab`.

At the same pose and with the same two candidate patches, additional
64-frame captures give `(B - A) / B` radiance contrasts of 0.0602 at
07:00 with the default haze floor and 0.0304 with the floor disabled.
At noon both floor settings give 0.0306 and the complete radiance images
are identical. The floor-on/off values are separate runs with the same
clock and cloud seed. These numbers are not yet the VLM-SH-006/-011
acceptance result: the caster is unidentified, and no finished-frame
non-shaft luminance comparison has been made. The new captures are in
`/tmp/byro-vlm-depth.agreem/full-field-haze`.

Finished-frame captures at this pose are in `full-field-haze-final`.
At noon the floor-on minus floor-off whole-frame mean RGB difference is
-0.00005 displayed 8-bit levels; the mean absolute difference is 0.00164
levels. A separate floor-on repeat differs from the first by 0.00076 mean
absolute levels. At 07:00 the floor-on minus floor-off whole-frame mean is
-0.040 levels, with a mean absolute difference of 0.662 levels. These
include the entire image, not a reviewed non-shaft mask; the captures
therefore do not establish the VLM-SH-011 tolerance or owner acceptance.

To investigate the candidate caster, a held-session probe read the 07:00
sun direction as `(0.964, 0.220, 0.148)` and the camera's vertical FOV as
45 degrees. It reconstructed points 1,000, 2,000 and 3,000 view-axis
units along each candidate's center ray, moved the camera there and looked
toward the sun. At all three sampled distances the resulting views for
both candidate rays show the terrain rising across the sunward crosshair;
satellite dishes and fencing are also nearby. The debug `pick` results
include large, overlapping rock and terrain bounding spheres and are not
triangle intersections. This narrows the investigation but does not
attribute the differential radiance to a particular caster or prove its
visibility over the full integrated ray. The views and logs are in
`/tmp/byro-vlm-depth.agreem/trace` and `trace-b`.

### Fallout 3 pose investigation, 2026-11-19

The current exterior content harness completed its three default-pose
captures in `MegatonWorld` (`-1,-7`), but its camera looks into nearby
ground and supports. At the elevated, level pose `(-2048,13568,26824)`,
the view contains buildings, wires and checkerboard-patterned terrain.
`tex.missing` reported no unresolved entity textures; that report does
not explain or validate the terrain appearance. The 07:00 environment
reported `WastelandClearMegaton`, GPU extinction 0.006337/m, haze floor
0.001369/m and sun direction `(0.903,0.402,0.148)`.

A full depth capture and 64-frame sun-occlusion on/off/repeat captures
at this pose loaded 4,696 entities and an active froxel grid. A search
found nearby 7 × 7 patches at `(395,430)` and `(458,458)` with mean
view-axis depths 4,769.6 and 4,781.1 units. Their sun-occlusion-off
increases in linear radiance were 0.000948 and 0.000111, respectively;
the repeated on capture was identical. However, their on-state radiance
is 0.02218 versus 0.01531, opposite the simple shadow-versus-lit ordering,
and both patches lie over the checkerboard surface. This pair is not a
shaft contrast measurement. Neither the caster nor the surface has been
established, and the separately captured depth is not synchronized with
the radiance. Baseline, pose, texture diagnostics, depth and valid on/off
captures are under `/tmp/byro-vlm-depth.agreem/fo3-content`, `fo3-poses`,
`fo3-textures`, `fo3-depth` and `fo3-ab-valid`.

### Skyrim ridge sweep investigation, 2026-11-19

The Whiterun ridge fixture was tried at increasing camera distances along
the original view direction. Loading each camera's own grid changes the
weather: the base grid `2,-4` resolves `SkyrimCloudyTU` (`0010A243`),
with GPU fog extinction 0.003347/m, while grid `0,-8` resolves
`SkyrimOvercastSnow` (`0004D7FB`) at 0.006570/m. The ridge is obscured in
the latter capture. A live `weather.force 0x0010A243 lock` switched the
intermediate grid back to the base weather and made the ridge visible.
Thus the weather change, not distance alone, confounds the unforced sweep.

Keeping grid `2,-4` fixed for stepped captures retains the ridge at the
base, 8,192- and 16,384-unit camera offsets. A 24,576-unit offset shows
strong dark bands and the 32,768-unit offset exposes large gaps in nearby
geometry, so the farther fixed-grid views do not establish a valid LOD
scene. Separate held-session depth fields and 64-frame scene-linear EXRs
for the first three offsets were sampled over 5 × 5 ridge and adjacent
sky patches. The approximate ridge depths were 93,176, 103,351 and
109,302 view-axis units; the absolute ridge-minus-sky linear luminance
differences were 0.01801, 0.01109 and 0.000034 respectively. These
selected image patches suggest decreasing contrast but are not a passing
VLM-AP-001 sweep: projecting the base patch's reconstructed world point
into the middle view lands on sky in the separate depth capture, so the
samples are not proven to track the same terrain point. The captures
also span only a small part of the required distant-LOD band. Evidence is
under `/tmp/byro-vlm-depth.agreem/skyrim-ridge-poses`,
`skyrim-ridge-intermediate`, `skyrim-ridge-fixedgrid`,
`skyrim-ridge-linear`, `skyrim-env-base`, `skyrim-env-mid2hex` and
`skyrim-env-fixedgrid`.

### FNV ridge sweep investigation, 2026-11-19

An elevated Goodsprings view at `(-67763,9516,-3567)`, looking along
`(0,0.03,1)`, exposes a distant ridge that the original seam fixture's
porch partly occludes. With grid `-17,0` held fixed, the same ridge remains
visible after moving 8,192 units backward; a 12,288-unit move at the same
height puts the camera inside rock. A sideways move at 16,384 units exposes
unloaded geometry. Raising the 16,384-unit view by 3,000 units and looking
along `(0,0.01,1)` clears the obstruction but changes both camera height
and sky direction.

For 5 × 5 patches near the same ridge point, separate held-session depth
fields report median view-axis distances of 142,890, 151,116 and 159,121
units. The latter two image positions were projected from the first point;
their median depths agree with its projected depths within about 64 units.
All 25 pixels in each adjacent sky patch are depth background. In separate
64-frame scene-linear captures, the absolute ridge-to-sky luminance
differences are 0.3473, 0.3255 and 0.4271 respectively. The first two
decrease; the raised third view increases. This is not a controlled
distance-only failure or pass: its height and viewing angle changed, and
the depth and radiance captures are not synchronized. The depth probes
used radius 1 at 07:00 while the radiance runs used radius 3 at noon;
their apparent depth agreement therefore does not prove that the same
LOD geometry was drawn in the radiance runs. The offsets also cover only
a small part of the distant-LOD band. Captures are in
`/tmp/byro-vlm-depth.agreem/fnv-ridge-poses`, `fnv-ridge-depth`,
`fnv-ridge-linear`, `fnv-ridge-high`, `fnv-ridge-high-depth`,
`fnv-ridge-high-linear`, `fnv-ridge-mid3` and `fnv-ridge-side`.

A fixed-height follow-up at `y=12516`, looking along `(0,0.01,1)`,
reconstructed one ridge point and projected it into five views. Depth
readbacks from a single frozen session tracked its predicted view-axis
distance from about 142,800 to 208,300 units, within 70 units at each
sample. At 273,900 and 339,400 predicted units the projected samples were
background, so this fixed-grid scene does not cover the full LOD band.
Separate initial linear runs at those first five views appeared
non-monotonic; same-session environment checks revealed that the farther
boots had selected `NVWastelandClear` rather than the base
`NVWastelandGS`, despite the common requested grid. Those unforced values
are confounded by weather and are not a distance-only result. Captures
are in `fnv-ridge-level`, `fnv-ridge-extended` and `fnv-ridge-unified`
under `/tmp/byro-vlm-depth.agreem`.

The five poses were then rerun with the clock paused at noon and
`NVWastelandGS` (`001237D7`) locked before the benchmark; each run had
1,200 stepped frames for the weather to settle. All five reported the
same forced weather and GPU extinction 0.002704/m after capture. In each
loaded session, a 64-frame scene-linear EXR preceded a held-session depth
readback. The projected ridge patches had median view-axis depths of
142,786, 150,994, 159,148, 175,610 and 208,406 units, and every pixel
in each 5 × 5 adjacent-sky patch was depth background. Their absolute
ridge-minus-sky linear luminance differences were 0.499702, 0.490984,
0.410898, 0.148554 and 0.470344: the sampled contrast is non-monotonic.
Independent repeats of the last two runs reproduced their contrast to
the printed precision and their depths within 60 units. Evidence is in
`fnv-ridge-locked`, `fnv-ridge-locked-middle`,
`fnv-ridge-locked-retry` and `fnv-ridge-locked-repeat` under the same
temporary directory. The first `fnv-ridge-locked/d64` capture is invalid:
its debug port was still in use and the weather command did not reach
the new engine; the dedicated-port retry is the value above.

This is a reproducible candidate failure of the literal VLM-AP-001
contrast rule over part of the band, not yet an isolated renderer defect.
The adjacent sky sample changes direction with the projected ridge point,
and may contain cloud contributions; the surface air term fades toward
the clear-sky radiance. The EXR average and depth readback are also
consecutive rather than the same frame. A matched clear-sky reference and
coverage of the remaining band are still needed before attributing the
reversal or accepting the full sweep.

An opt-in `BYRO_DISABLE_SKY_CLOUDS=1` diagnostic sets the renderer's
procedural cloud coverage and all four authored cloud-layer scales to
zero. On a rebuilt release, `sky.dump` confirmed these values at both
poses while the locked weather and sun direction remained the same. With
clouds disabled, the 175,532- and 208,469-unit ridge samples gave
ridge-to-adjacent-sky linear luminance differences of 0.153049 and
0.481416 respectively. The ordering remains non-monotonic, so visible
cloud layers alone do not explain the reversal. This diagnostic changes
the sky and cloud lighting and is not a substitute for a vanilla-weather
acceptance capture. Its EXRs, depth fields and state dumps are in
`/tmp/byro-vlm-depth.agreem/fnv-ridge-clear-sky`.

Turning off exterior froxel sun occlusion as well left both sampled
ridge and sky luminances unchanged to the printed precision, so that
shadow query does not explain this pair. A horizontal band of the
clear-sky capture was also nearly uniform within each view, but its
linear luminance was about 0.500 at the nearer pose and 0.626 at the
farther pose. `env.health` now reports the renderer's fog-height
reference: with camera Y 12,516 at both poses, it was 12,516 at the
nearer pose and 8,603 at the farther pose. The reference is derived from
a downward terrain query, with camera Y as its fallback; the report alone
does not say which branch supplied either value.

With the diagnostic `BYRO_FOG_HEIGHT_REFERENCE=12516` holding that
reference fixed, the same clear-sky, locked-weather comparison measured
contrasts 0.153049 and 0.134872 at depths 175,627 and 208,346 units.
The nearer result matches the unpinned run to the printed precision;
the farther contrast falls rather than rising. This isolates the
changing fog reference as a major contributor to the observed reversal,
but it does not prove the full VLM-AP-001 sweep or replace a capture
with the normal terrain-derived reference. The unshadowed, reference
report and pinned-reference runs are in `fnv-ridge-clear-unshadowed`,
`fnv-ridge-height-report` and `fnv-ridge-fixed-height` under the same
temporary directory.

The short exterior ground-query limit caused one concrete reference jump:
at the nearer FNV view a 4,096-unit ray missed ground and fell back to
camera Y 12,516, while a longer diagnostic ray found ground at 6,603;
the farther view found ground at 8,603 under either range. The renderer
now searches to the normal camera far plane for exterior cells, while
retaining the 4,096-unit limit for interiors and the camera-Y fallback
when no hit exists. A regression test covers both the distant exterior
hit and the unchanged interior fallback, and an `env.health` test covers
the resource-lock ordering.

On release binary SHA-256
`44548afe271214b6bce15bca25a67c7449e97f872515617b55c97933952be551`,
the production query reported references 6,603 and 8,603 at the two
views. With locked weather and the clear-sky diagnostic, their measured
ridge-to-sky contrasts were 0.566749 and 0.481416, respectively. These
are two points in a diagnostic scene, not the full vanilla VLM-AP-001
sweep. The existing FNV and Skyrim fog-grid seam gate passed on this
binary: the largest regional change beyond repeat noise was 1.57 displayed
levels against the 2.5-level tolerance. The production pair and seam
captures are in `fnv-ridge-production` and `seam-long-ground` under
`/tmp/byro-vlm-depth.agreem`.

The same on/off comparison on the Oblivion `Tamriel` grid `0,0`, at
07:00 with the default static camera at `2048,3380,-1848` facing
`0,-0.447214,-0.894427`, produced a whole-frame mean linear-luminance
difference of 0.000000607 and a 99th percentile of 0.00000594. These
captures use the corrected host restriction. They do not establish a
named caster or a depth-matched shaft contrast; this static pose and
measurement cannot be equated with the earlier stepped-camera canopy
experiment. The EXRs and logs are in the same `occlusion-ab` directory.
Repeating with `renderer-stepped` and its static camera produced the same
logged camera pose, a whole-frame mean difference of 0.000000629 and a
99th percentile of 0.00000618. That repeat did not use the earlier
experiment's boosted sun-only dust; it does not identify the canopy's
shadow or establish the default-floor contrast.

## Current-build sealed controls, 2026-11-19

The A9 sealed-control rerun is in progress at
`/tmp/byro-vlm-controls-20261119`, using a release binary based on
`549c2dd3b69f` with uncommitted workspace changes (SHA-256
`9c0dce046bc643480e00a79e1e824a68aff5018a7ef2c0be9f5b531dabe20424`).
The exterior-occlusion diagnostic and haze overrides were unset. The first
control, FNV `RocketLabTop`, passed all four sampled poses: none had a
qualifying day-minus-night region, and repeat-noon noise p99 ranged from
6.86 to 27.64 RGB luminance levels. This is one cell, not a full A9
control pass. FNV `NCRPrisonBlockB` **failed** one of four poses: 0.444%
of the frame exceeded the 3-level threshold, against repeat-noon p99 of
1.21 levels. The affected pixels occupy x=338–428 and y=597–719; their
mean noon, midnight and repeat-noon luminances were 26.46, 22.29 and
25.81. The cause and repeatability are not established. FNV
`2EOVault21GiftShop`, `HELIOSOneDeck` and `HiddenValleyBunker1`, FO3
`Tenpenny01`, and Oblivion `AnvilCastleSmugglersCave`, `TestCave01` and
`WarehouseCaveGeneric` and `ImperialSewers03`, and FO4 `Vault111Cryo`
also passed, each with a
zero qualifying region.
Skyrim `HillgrundsTomb01` passed with a 0.0245% region, below the 0.1%
harness threshold. Thus 11 of 12 controls pass, and NCR Prison fails.
The per-cell records are in `results.json`; the wrapper's pipeline exit
status did not propagate the harness failure, so the recorded result is
read from those records rather than the wrapper exit code. A same-binary
repeat at the failing pose also failed: 0.1076% above the 3.86-level
threshold, with repeat-noon noise p99 1.93. Of the repeat's 992 qualifying
pixels, 989 lie within the original failing mask. A noon/noon/noon control
and a midnight/noon/midnight sequence were then taken at the same target.
Over the original mask, mean `(first + repeat)/2 - middle` luminance was
3.845 levels in the original, 3.210 on repeat, 1.433 with noon in all
three captures, and -2.082 with midnight/noon/midnight. The sign reversal
is consistent with a sun-correlated change, but the all-noon capture's
whole-frame repeat p99 was 14.85 levels, so fixture variation remains a
confound. A separate froxel-sun-off control is needed before attributing
the change to sun visibility. A subsequent same-binary on/off pair
(SHA-256 `b81f34c0d3c515ceb5312e49481306545ec16b9a6764f4b7b0f300c245d4f5f0`)
disabled only the froxel sun term in the off run. In the original failing
mask, mean noon-minus-midnight was 4.797 levels with the sun term on and
4.909 with it off; repeat-noon p99 within that mask was 2.21 and 2.72
levels. Both whole-frame harness results were PASS because frame-wide
repeat p99 was about 15.6 levels. This pair shows that non-sun variation
can produce the apparent change at this pose; it does not turn the two
earlier failed gate runs into passes or prove every possible cause. Captures
are in `ncr-sun-on` and `ncr-sun-off` under the run directory. On the
exterior shaft lab, the same diagnostic reduced linear luminance over
the wall region from 0.16128 to 0.06644 when switched off, confirming
that it can remove a live sun term. Lab captures are in
`/tmp/byro-vlm-depth.agreem/sun-term-ab`.

Holding `LightFlicker` at its initial phase did not eliminate the variation:
the sun-on and sun-off runs still had whole-frame repeat p99 of 15.57 and
14.92 levels. A live inspection found no `LightFlicker` entities, while
the cell contained 12 `AnimationPlayer` entities. Two `light.dump` reads
showed the same 11 lights with identical reported positions, colours and
intensities. This excludes the `LightFlicker` system as the observed cause
in this cell; it does not identify which other animation or temporal effect
is responsible. The unused experimental freeze switch was removed. Logs
and captures are in `ncr-frozen-on`, `ncr-frozen-off` and
`ncr-light-inspect` under the run directory.

A `renderer-static` benchmark followed by `--bench-hold` did not freeze
the capture: the hold returns to live interactive mode and repeat-noon p99
was 16.85 levels. A separate native-TAA, ray-jitter-disabled run still had
repeat p99 14.85 levels. Neither change isolates the varying source or
supersedes the two failures. The captures are in `ncr-static` and
`ncr-native` under the same run directory.

Authored `Homogeneous` and `LightShaft` volumes sample a time-varying
procedural density pattern. A diagnostic that held that pattern at its
initial phase reduced the NCR repeat-noon p99 to 1.0 level, but the
failing target still exceeded the 3-level threshold over 0.2655% of the
frame with froxel sun on and 0.3318% with it off. The masks overlap in
2,081 pixels. This accounts for much of the broader temporal variation,
but a separate non-sun difference remains. These runs used the same
dirty release binary (SHA-256
`a7a8af2592dc2e29cc61bea0c9efe60331a6a82a7da43e50cc6859165015e35c`)
and are in `ncr-frozen-pattern-on` and `ncr-frozen-pattern-off`. With
the pattern fixed and froxel sun off, mean `(first + repeat)/2 - middle`
over the original failing mask was -0.119 levels for noon/noon/noon and
-3.310 for midnight/noon/midnight, compared with +3.418 for
noon/midnight/noon. This is a reproducible time-dependent change outside
the direct froxel sun term, not merely the procedural pattern animation.
The cause remains unverified. These controls are in
`ncr-pattern-noon-control` and `ncr-pattern-reversed-control`.

Noon and midnight state dumps in `ncr-pattern-state` report the same
cell fog and the same 11 local light records; the reported difference is
the portal sun direction and radiance. The CPU grid filter had still used
the unmasked portal sun to admit swept candidates when the diagnostic
zeroed the shader's sun input. Aligning the filter with the uploaded
radiance is correct, but did not remove the effect: `ncr-portal-gated`
still fails with sun off and the density pattern fixed (0.345% above
3 levels, repeat p99 1.0). Portal sweeping alone therefore does not
explain it.

The radiance view samples the integrated volume at the visible surface's
depth. A native-resolution noon/midnight recheck (`ncr-depth-native`)
found changed depth at several pixels in the original failing region:
at `(380,620)` the noon view-axis depth was 570.149 world units and the
midnight depth 344.870; at `(360,650)` it was 536.458 and 347.424.
At `(360,620)` both read 570.149. The camera position and reported
orientation stayed fixed, but the surface seen by the rays changed.
Depth captures were separate readbacks shortly before the screenshots,
not the screenshots' exact frames. This is direct evidence of a depth
confound at the sampled target and a plausible explanation of the
non-sun radiance change; it does not prove that every pixel or every
earlier run has the same cause. The A9 gate still needs a depth-stable
comparison, rather than treating the unguarded run as a valid pass.
The harness now also checks camera yaw and pitch, as well as position,
after both the midnight and repeat-noon captures; an unreadable camera
pose is invalid. A live recheck at this target passed that camera guard
but still failed the radiance threshold, confirming that a stationary
camera alone does not rule out moving geometry.

Finished-frame captures at the same target in `ncr-surface` show an NPC
moving into the lower-left region containing the depth samples between the
noon and midnight views. These finished frames and depth readbacks are
separate captures near the radiance frames, not synchronized attachments;
they identify a concrete moving occluder at this pose but do not establish
the exact per-pixel depth for each earlier radiance frame.
As a diagnostic only, excluding the conservative image rectangle
`x=300..459, y=560..719` around that actor from the two fixed-pattern
captures leaves 97.22% of the frame. Neither the sun-on nor sun-off image
has any remaining pixel above the 3-level threshold; the largest remaining
changes are 2.68 and 2.89 levels. This hand-drawn exclusion is not a
same-frame depth mask and is not counted as an A9 pass.

An explicit held-session diagnostic now pauses world systems while
continuing rendering and debug commands. On dirty release binary
`96f8ac8dc17ec4edfcaefb1ae63031c4c9857cf16e25306ef3d2eec7e64b5493`
(SHA-256), NCR's failing target passed with zero qualifying region in a
native-resolution run, and all nine sampled depths were identical at noon
and midnight. A full four-pose run through the regular harness also passed:
its largest qualifying region was 0.00315% of the frame, below 0.1%,
with repeat-noon p99 1.0 level. `time.set` changed the portal sun as
intended. As a positive control, Nellis Hangar 1 at one target passed the
day/night shaft test with 92.75% qualifying region and repeat-noon p99
1.72 levels under the same paused-world mode. Captures and logs are in
`ncr-paused-world`, `ncr-paused-full` and `nellis-paused-world` under the
run directory. The depth check samples nine pixels rather than a full
same-frame depth mask, and these results do not establish an opening
inventory or the other remaining PRD gates.

For the opening inventory, a separate paused-world capture in Nellis
Hangar 1 places the camera at `(-18, 500, -400)`, yaw 0 degrees and pitch
60 degrees. The finished frame shows an irregular sky opening surrounded
by the hangar roof and its structural beams, viewed from below. Its
volumetric-radiance view changes between noon and midnight: the mean RGB
difference is 2.92 displayed 8-bit levels, versus 0.15 between repeated
noon captures. These are whole-frame diagnostic comparisons, including
the UI, not the A9 region metric or a depth-matched shadow measurement.
The images and run log are under
`/tmp/byro-vlm-controls-20261119/nellis-interior-verified` and
`nellis-interior-probe`. This establishes one visible roof opening in
one hangar; it does not verify every converted aperture or any FO4 cell.
The same pose in `NellisHangar2` and `NellisHangar1b` also shows a roof
break, partly obscured by nearby geometry in the latter. Their whole-frame
mean noon-minus-midnight radiance differences are 2.91 and 2.77 levels,
against repeat-noon differences of 0.20 and 0.19, respectively. Captures
are in the corresponding `nellis2-interior-verified` and
`nellis1b-interior-verified` directories. These visual observations do not
by themselves associate each converted aperture with a particular opening
or prove that the visible sky connects to exterior world geometry.
The earlier positive-control `cam.tp 259` points at the entity origin,
which differs from the transformed center of its fog bounds, and its
finished frame is close to the roof. Its radiance measurement alone should
not be used as evidence of a camera pose inside the hangar or of the
opening's surrounding geometry.

An initial FO4 inventory probe captured all ten runtime `LightShaft`
entities in `ConcordMuseum01` on the same binary, along with their full
component and transform dumps, under `opening-ConcordMuseum01` in the run
directory. Most `cam.tp` views sit against or outside walls, so these
images are not sufficient opening evidence. For one candidate (runtime
entity 9921 in that run, `plane046:0`, `AuthoredMesh`), the global origin
was `(902.22, 743.69, -88.00)` and the fog-box center was offset in local
coordinates by `(-1.16, -84.12, 0.00)`. Four surrounding views are in
`opening-views-ConcordMuseum01`; the view at `(752, 720, -88)`, yaw -90
degrees, shows an arched window with sky-colored background from a room.
This locates a candidate window near the shaft, but the capture has not
established the transformed fog volume's overlap with that window or a
clear ray to outdoors. Neither this candidate nor the other nine is yet
counted as verified under VLM-INT-001.

The same probe captured 12 `TrinityTower01` shaft entities and their
component dumps in `opening-TrinityTower01`. The automatic target views
are mostly obscured by nearby grilles and walls; none is counted as an
outdoor-border verification. The placement dumps are a starting point
for selecting interior views, not a substitute for the required geometry
evidence.

Separately, VLM-INT-005's overflow warning was deduplicated by count,
not cell: two consecutive cells dropping the same count could suppress
the second warning. The renderer now tracks the current interior root
per renderer instance, reports the first overflow of a cell once, and
continues to keep the nearest apertures. Tests cover equal overflows in
different cells, changing counts within a cell, and the count returned
by the composite builder. The sealed-control binary above predates this
logging-only correction.

For VLM-HF-003's original-engine weather comparison, the installed
Fallout 4 executable was tried in a temporary Proton prefix with the
installed data mounted read-only. Proton stopped at a Steam client
assertion before Fallout 4 launched; no original-engine capture was
obtained. No Steam client was running when checked. The failed probe,
log and screenshot are in `/tmp/byro-vlm-original-fo4`.
