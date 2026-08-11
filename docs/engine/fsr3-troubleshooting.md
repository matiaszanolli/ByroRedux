# FSR 3.1 troubleshooting

Operational guide for the FSR upscaler path. Design rationale lives in
[`fsr3-upscaler-integration-plan.md`](fsr3-upscaler-integration-plan.md); this
is the "something looks wrong, what do I check" companion.

## First: find out what is actually running

```
byro> ctx.upscaler
  fsr3/quality · FSR 3.1.4 (fp16) · 853x480 -> 1280x720 · 31.8 MB SDK working memory (4.6 MB aliasable)
  gpu_upscale_ms = 0.163
```

Nearly every FSR question is answered or narrowed by this one line. In
particular, **`--upscaler fsr3` does not guarantee FSR is dispatching.** Two
failure modes degrade to a native blit and keep rendering:

| `ctx.upscaler` says | Meaning |
| --- | --- |
| `fsr3/quality · FSR 3.1.4 …` | FSR is dispatching. |
| `fsr3/quality · native HDR blit · …` | Context creation failed at startup. Look for `FSR context creation failed:` in the log. |
| `… — DISPATCH FAILED (…), on native blit` | A dispatch was rejected mid-session and FSR is latched off for this swapchain generation. |

The degradation is deliberate — a rejected dispatch used to take the whole
frame down — but it is quiet by design, so it will not announce itself as a
visual bug. Check here before investigating anything else.

A latched failure clears on the next resize or preset change, since both
rebuild the context. `r.upscaler taa` then `r.upscaler fsr3 quality` is the
quickest way to re-arm it deliberately.

## Symptom: the whole frame smears when the camera moves

Motion-vector sign, scale, or Y convention. The engine stores
`current_uv - previous_uv` in normalized UV space; FSR wants
`previous - current` in pixels, and the boundary adapter supplies
`motionVectorScale = (-render_width, -render_height)` to convert. Get the sign
wrong and *every* pixel reprojects the wrong way.

Reproduce and bisect with the deterministic camera:

```bash
cargo run --release -- --cornell --upscaler fsr3 --fsr-quality quality \
  --bench-frames 60 --bench-mode renderer-stepped --bench-camera pan \
  --screenshot /tmp/pan.png
```

`pan` is the cleanest probe: motion is near-uniform and horizontal, so a sign
error is unmistakable rather than subtle. The numeric contract is pinned by
`motion_adapter_converts_current_uv_minus_previous_to_fsr_pixels` in
`upscaling.rs` — if that test passes and frames still smear, the problem is
upstream of the adapter (the G-buffer motion attachment itself), not in the
FSR boundary.

## Symptom: transparent objects trail or ghost

Reactive / transparency-and-composition mask coverage. Both are render-pass
attachments 6 and 7, cleared to zero and MAX-blended by transparent draws;
opaque geometry masks its writes off entirely.

Things that legitimately write no mask, and so are expected to ghost until the
carried phase-4 work lands:

- **The Scaleform/Ruffle UI overlay.** It is still composited *before* the
  upscale, so it goes through temporal reconstruction. Marking it reactive
  would paper over that; the fix is moving it after upscale.
- **Anything alpha-tested.** Cutouts have coherent depth and motion, so they
  are correctly reconstructed from those — a reactive mask would only throw
  away history they can legitimately use.

If a specific *alpha-blended* material ghosts, check that its draw actually
reaches the blend pipeline (`INSTANCE_FLAG_ALPHA_BLEND`), since the mask write
in `triangle.frag` is gated on `isAlphaBlend`.

## Symptom: it looks soft / detail is missing

Expected to a degree — the reduced presets reconstruct from fewer samples. Put
a number on it before treating it as a defect:

```bash
cargo test --release -p byroredux --test upscaler_quality -- --ignored --nocapture
```

Measured worst case on Cornell (RTX 4070 Ti): SSIM 0.955 at Quality, 0.920 at
Performance, against the native TAA render. If the run reports numbers near
those, the softness is the preset working as designed; if SSIM has dropped a
point or more below the committed thresholds, something regressed.

Check the **mip bias** if softness looks uniform rather than
detail-dependent: FSR presets request `log2(render/output) - 1`, clamped to
the device's `maxSamplerLodBias`. A device with a tight clamp silently gets
less bias than requested, which reads as blur. The applied value is logged at
texture-registry recreation (`mip bias -1.586`).

## Symptom: FSR is slower than TAA

Check the preset. **Native AA (1.0×) is expected to be slower than TAA** — it
does no upscaling at all, so it pays FSR's reconstruction cost with none of the
pixel savings. Measured on Cornell it is ~6% slower end-to-end. It exists to
isolate reconstruction quality from upscaling quality, not as a performance
option.

For the reduced presets, compare the two numbers the benchmark separates:

```bash
scripts/fsr-bench-matrix.sh 3 300
```

`render rec.` is the gross saving from shading fewer pixels; `net rec.` is what
survives after paying for the upscale dispatch and the output-resolution
presentation pass. A large gap between them means the frame is dominated by
work that does not scale with render resolution — in which case a preset buys
less than the pixel-count ratio suggests, and the fix is elsewhere.

## Symptom: validation errors or a device loss under FSR

Check the FP16/FP32 permutation in `ctx.upscaler` first.

The SDK chooses its shader permutation by querying the **physical** device
(`ffx_vk.cpp` `GetDeviceCapabilitiesVK` enumerates *available* extensions
despite its comment claiming otherwise). That makes the engine's
`shaderFloat16` enable load-bearing: if a device advertises FP16 while the
engine leaves the feature disabled, the SDK dispatches FP16 shaders against a
device where the feature is off. A debug assertion in `create_logical_device`
catches divergence, but only in debug builds.

Also note the FP32 path is **unvalidated** — no device without `shaderFloat16`
was available. If you have one, exercise it and record the result.

## Gotchas that cost time

- **Upscaler comparisons use `renderer-stepped`, never `renderer-static`.** A
  parked camera converges away the disocclusion, reprojection, and camera-cut
  failures under test. `--bench-camera` is inert without `--bench-frames` and
  rejected as an unnamed hybrid unless paired with the stepped mode.
- **Bench CWD matters.** Bare `--bsa` names resolve against the working
  directory, not the `--esm` folder. Run from the game's `Data/`; otherwise
  archives silently fail to open, the scene loads near-empty, and the FPS
  figure is meaningless (and flattering).
- **Preset changes are not free.** Switching rebuilds every render-resolution
  target and resets temporal history, so the first frames after a switch are
  mid-recovery. Do not screenshot immediately after `r.upscaler`.
- **`pkill -f byroredux` kills your own shell** if the command line contains
  the pattern. Use `pgrep`, then `kill` the PID.
