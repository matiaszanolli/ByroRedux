# M-LIGHT A3 direct-light baseline — 2026-11-16

Owner selected direct light for A3. This is a provisional measurement, not
a pass: the new signal has no frozen floor or threshold yet. The engine was a
release build of `d9d11dc34` on an NVIDIA RTX 4070 Ti, driver 580.178.04;
the capture harness was modified in the working tree. Camera pan, tier 3,
one seed (0), and the `direct_only` view. The held control for each
post-stop capture is sampled at the same age from a stationary start.

| Measurement | Temporal error | Held control error | Excess |
|---|---:|---:|---:|
| Worst during motion (frame 112) | 0.029446 | 0.00344 | 0.026003 |
| 16 frames after stop | 0.014144 | 0.0138 | 0.000366 |

The full per-frame results and captures are in
`/tmp/byro-mlight-a3-direct/`. The first run's manifest incorrectly lists
two engine seeds and an offline reference because it used the old manifest
writer; this run actually used seed 0, single-frame moving and held
captures, and engine parked references. The harness now records these
fields correctly. A second run with seed 1 is in
`/tmp/byro-mlight-a3-direct-seed1/`: worst during-motion excess 0.026049
and post-stop-16 excess 0.000383. The two excesses differ by 0.000046 and
0.000017 respectively. This pair suggests the during-pan difference is
stable, but it is not a complete per-pixel noise-floor study and does not
attribute the cause. The old `final`-image A3 thresholds do not apply to
this direct-light measurement.

The focused `scripts/m-light/a3_motion_attribution.py` comparison at frame
112, seed 0, used a separate parked reference and age-matched held control
for each condition (`/tmp/byro-mlight-a3-motion/attribution.tsv`). Normal
rendering measured moving error 0.029446, held error 0.003443 and excess
0.026003. With `DBG_DISABLE_TEMPORAL` (`0x40000`) the moving and held errors
were both 0.094401, with excess below 1e-8. Disabling temporal reuse removes
the motion-dependent excess but substantially increases sampling error.
The switch disables both reservoir selection reuse and the shadow-ratio EMA,
so the experiment does not distinguish those mechanisms or justify shipping
with temporal reuse disabled. The experiment uses one seed and one pose.

One possible contributor is the different history mode: while the camera
moves the EMA uses the moving floor of 0.1 and cap of 16; a held camera can
use the much longer parked tail. For independent samples, an EMA with weight
0.1 has a standard deviation about `sqrt(0.1 / 1.9)` of a single sample;
applied to the no-temporal error 0.0944, that rough estimate is 0.0217, of
the same order as the observed moving error 0.0294. This is an inference, not
an attribution: the samples are correlated, the reference has noise, and the
moving path also reprojects history. A mode-matched stationary control is
needed before treating the 0.026 excess as reprojection bias.

On 2026-11-16 a diagnostic build from `d9d11dc34` forced only the shadow
ratio EMA's `sceneStatic` choice to false, so the stationary receiver also
uses the moving cap and alpha floor. Both fragment variants were rebuilt;
the shipping shader was not changed. At frame 112 and seed 0, using the
same normal parked reference and mask as the focused run above, moving error
was 0.029446 and held error 0.012772, leaving excess 0.016674
(`/tmp/byro-a3-mode-result/`). This is 0.009329 lower than the original
0.026003 excess: different EMA windows explain part of the comparison, but
not all of it at this frame. The camera history mode still differs in other
parts of the renderer, and the captures use one seed and pose, so the
remaining difference is not by itself proof of a reprojection defect. The
short-window shader is a diagnostic, not a proposed production setting. The
modified and normal moving-frame EXRs were bit-identical in this run, so the
observed reduction came from changing the held control rather than the
moving capture.

A `restir_history` debug-view capture of the same short-window diagnostic at
frame 112 (`/tmp/byro-a3-history-views/`) indicates that 98.88% of the
487,781 pixels in the lit mask reached the moving cap of 16; the held view
also reads 16 throughout that mask. The 1.12% below the cap account for
2.72% of the moving view's luminance squared error against the parked
reference. Widespread history rejection therefore does not explain the
remaining error in this one frame. This is a separate debug-view capture,
not a per-pixel history trace from the direct capture; the view's effect on
the renderer and additional seeds and poses have not been ruled out.

The short-window comparison was repeated with seed 1 and each frame's own
seed-1 parked reference (`/tmp/byro-a3-mode-seed1/`). At frame 64 the moving
and held errors were 0.028426 and 0.012287 (excess 0.016139); at frame 112
they were 0.029489 and 0.012732 (excess 0.016757). The latter is close to
the seed-0 excess 0.016674. These additional points reproduce the residual,
but do not isolate its mechanism or establish a complete noise floor.

As a coarse localization check, pixels in the top 10% of the parked
reference's luminance-gradient magnitude account for 12.3% of moving and
15.1% of held luminance squared error in the seed-0, frame-112 short-window
comparison. The residual is therefore not concentrated only at the
highest-gradient pixels. This gradient also includes reference noise and
surface shading; it is not a shadow-edge segmentation or a bias test.

The shadow sampler rotates its low-discrepancy sequence by screen pixel
(`ditheredSequence2` in `math_common.glsl`). Reprojecting a moving surface
can change that rotation along its history; the held surface retains one
rotation. An isolated diagnostic kept the short-window override and used
`uvec2(0)` as the shadow sampler's pixel for every receiver, giving all
pixels the same rotation. Both shader variants were rebuilt in a temporary
`d9d11dc34` source snapshot; the working-tree shader was not changed. Against
the same seed-1 parked references and masks, frame 64's moving / held errors
were 0.017033 / 0.014464 (excess 0.002569), and frame 112's were 0.014192 /
0.010442 (excess 0.003751). The corresponding per-pixel-rotation,
short-window excesses were 0.016139 and 0.016757. Captures and logs:
`/tmp/byro-a3-rotation/`.

This supports the hypothesis that changing the sequence rotation along a
reprojected path contributes to the moving excess. The control also makes
screen-wide shadow samples correlated and changes the spatial noise itself,
so it neither isolates every cause nor establishes a production fix. A
temporally stable, spatially decorrelated sampler would need its own accuracy,
ghosting, and performance measurements before replacing the current one.

A second temporary prototype kept the short-window override, carried a
10-bit rotation key through validated reservoir history in the unused high
bits of the 22-bit blocker field, and initialized that key from pixel x/y
modulo 32. The blocker id was masked back to 22 bits on read; the reservoir
stride was unchanged. With seed 1, frame 64's moving / held errors were
0.015468 / 0.012476 (excess 0.002991), and frame 112's were 0.016120 /
0.012962 (excess 0.003158); captures and logs are in
`/tmp/byro-a3-carried/`. This is promising for temporal stability but the
simple modulo key has a measurable spatial drawback: at frame 112, held
luminance error's horizontal 32-pixel-offset correlation was -0.231 versus
+0.086 for the per-pixel-rotation short-window control, on pairs inside the
same lit mask. It introduces a repeating spatial structure. This one-seed,
two-frame experiment is not an acceptance test; the prototype stays out of
the shipping shader pending a spatially sound encoding, moving-blocker and
disocclusion checks, and the other shadow gates.

Replacing the modulo initialization in that prototype with a fixed hash of
the screen pixel removed the measured 32-pixel periodicity in this held
frame: offset-32 correlation was +0.001 (baseline +0.086, modulo -0.231).
At seed 1 the hashed prototype's moving / held errors were 0.015457 /
0.012484 at frame 64 (excess 0.002973), and 0.016037 / 0.012966 at frame
112 (excess 0.003071). Captures: `/tmp/byro-a3-hash/`. This is a candidate
for further testing, not a ship decision: it changes the spatial sampling
distribution and carries keys across reprojection, so its moving-occluder,
disocclusion, convergence, full A3/A14, content and performance behavior
remain to be measured. The experiment still forces the short EMA window in
both conditions and has not tested the production deep parked tail.
On the moving frame the hashed prototype's error correlation was +0.302 at
one pixel and +0.241 at 32 pixels, versus -0.199 and -0.043 for the
short-window baseline. This is a change in the character of residual error,
not proof of a visible artifact; spatial review remains necessary.

The hashed prototype was then rebuilt with the normal deep parked tail
restored. Against the original seed-1 parked references, frame 64's moving /
held errors were 0.015457 / 0.004984 (excess 0.010473), versus the original
0.028426 / 0.00509 (excess about 0.02334); frame 112's were 0.016037 /
0.003248 (excess 0.012789), versus 0.029489 / 0.00344 (excess about
0.02605). Captures: `/tmp/byro-a3-hash-normal/`. This reduces the measured
excess but does not meet any newly approved budget, because none has been
set. The remaining A3 frames, A14 and the other tests above still apply.

The current release renderer's A14 baseline was also remeasured with seed 0
as part of a same-run candidate comparison: excess 0.079677 against the
existing 0.05 limit, hold floor 0.048200 (`/tmp/byro-a14-baseline/`). The
normal-history hashed candidate measured 0.079785, floor 0.048200
(`/tmp/byro-a14-candidate/`). Both fail the existing gate. The difference
of 0.000109 is a single-seed comparison, not a formal noise-floor or
non-regression determination. This prototype does not resolve the measured
moving-blocker error.

A focused baseline diagnostic at A14's worst gated frame, 80, captured
`direct_only` for the moving, held and converged parked blocker with the
same exported floor mask and seed 0 (`/tmp/byro-a14-direct-diagnostic/`).
The direct moving error was 0.135983, held error 0.007154, excess 0.128830.
The existing A14 acceptance measure remains the final image: direct light
has a different mean normalization and no approved A14 budget. This diagnostic
nevertheless shows a substantial moving-blocker discrepancy already exists
in direct light; slow indirect convergence alone cannot explain it.
With `DBG_DISABLE_TEMPORAL` (`0x40000`), the moving and held direct frames
both measured 0.134643 against that same parked reference (excess 0),
`/tmp/byro-a14-no-temporal/`. This removes the motion-dependent difference
but leaves single-frame error far above the normal held error. The flag also
disables selection reuse, so it does not isolate the shadow-ratio EMA and is
not an acceptable fix by itself.

The baseline `restir_history` diagnostic at frame 80 marks 45.12% of the
exported floor mask as mover-tainted; those pixels account for 99.42% of
the moving direct-light luminance squared error against the parked reference
(`/tmp/byro-a14-history-views/`). The mean signed moving-minus-parked
luminance is -0.03861 within that marked area and -0.00108 outside it.
The view is a separate capture, not a trace from the direct frame, but it
localizes the discrepancy to the mover-tainted area for this pose and seed.

An isolated shader experiment raised the EMA update floor from 0.1 to 0.3
only when that history is mover-tainted, with the normal parked mode otherwise
unchanged. At frame 80, using the baseline parked references and floor mask,
direct error fell from 0.135983 to 0.058004 and final-image error from
0.127877 to 0.056321 (`/tmp/byro-a14-alpha/`). This is a single-frame
diagnostic, not an A14 pass: held controls and the whole A14 sequence are
being measured, and increased temporal noise and other mover/disocclusion
behavior require review. No production shader has been changed.
At this frame, the prototype's held errors were unchanged from baseline:
0.007154 direct and 0.048200 final (`/tmp/byro-a14-alpha-held/`). Its
final-image excess at frame 80 is therefore 0.008120, versus baseline
0.079677. The full seed-0 A14 prototype run passed its existing threshold:
worst excess 0.009508 (limit 0.05), measured floor 0.046880
(`/tmp/byro-a14-alpha-gate/manifest.tsv`), versus the same-machine baseline
0.079677 and floor 0.048200. This does not measure temporal noise under the
faster moving tail or establish the other shadow and content gates.

At frame 112, a diagnostic absolute-difference image
(`/tmp/byro-mlight-motion-error.png`, red = moving error, green = held error,
10× linear scale) shows structured fine bands over the receiver. The largest
1% of the lit-mask pixels contribute about 14.4% of the moving squared error;
the mean signed moving-minus-parked difference is 0.000348 on that mask.
This localizes the observation but does not by itself identify its source.
