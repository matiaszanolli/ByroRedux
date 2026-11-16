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
