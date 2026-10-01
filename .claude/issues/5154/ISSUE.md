# null: RENDERING (known): near-field colors oversaturate when auto-exposure adapts to low light

labels: bug, renderer, shaders
state: OPEN

**Reporter**: the maintainer (2026-09-30) — 'Colors in the closer range get extremely saturated when retina gets adjusted for low light. This is a known issue. It's a known issue that we need to tackle.'

**Live evidence (2026-09-30, MarkarthWarrens interior, RTX 4070 Ti)**: driving the P4 route in a near-black RT interior, near-field wall surfaces render with extreme saturation and noise-amplified mottle while the eye-adaptation (EV100 auto-exposure meter) lifts exposure for the dark scene; the same cell at native TAA with the meter still active stays watchable but retains the saturation character. A capture with the stage-36 objective pair presenting is retained in the p4-quest-route smoke artifacts.

**Character**: the exposure lift pushes mid-tones into the steep region of the tone curve, so per-channel deltas magnify into visible hue shifts (the classic dark-scene saturation blowout), and denoiser residual amplifies with it. Suspect the auto-exposure path needs a desaturation-to-lift compensation (or an EV-dependent chroma compress) between the meter and the tonemapper — AgX's own coercive scheme should partially handle this, so the interaction between `exposure_meter`'s EV100 lift and the AgX/ACES paths is the place to start.

**Secondary observation from the same session**: the persisted upscaler setting (FSR native-aa, whose RCAS sharpening runs at 1.0x) amplified the effect substantially vs TAA; the exterior smokes already pin TAA for live frames. Root-causing the adaptation/saturation interaction is the real fix — the upscaler is only a multiplier.

**Repro**: `cargo run --release -- --esm <Skyrim>/Data/Skyrim.esm --cell MarkarthWarrens --bsa … --bench-hold`, look at any near-field surface in the torch-lit corridor after the meter settles.
