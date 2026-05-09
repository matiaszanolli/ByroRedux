# Golden frames

Reference PNGs for the visual-regression tests in
[`tests/golden_frames.rs`](../golden_frames.rs).

## Workflow

```bash
# Run the goldens (requires Vulkan device).
cargo test --release -p byroredux -- --ignored golden

# Regenerate a baseline after an intentional visual change.
BYROREDUX_REGEN_GOLDEN=1 cargo test --release -p byroredux -- --ignored cube_demo_golden_frame
```

When a test fails it saves `<baseline>.actual.png` here next to the
baseline so you can do a side-by-side visual diff in any image viewer
before deciding whether to:

- **Fix the regression** in renderer code, or
- **Regenerate the baseline** (only if the visual change was intentional)

## Conventions

- One scene per file. Keep names descriptive (`cube_demo_60f.png`,
  not `test1.png`).
- Resolution matches the engine's default (1280×720 unless overridden).
- Don't commit `*.actual.png` — they're test artefacts.

## When to add a new golden

A scene is worth a golden test when it exercises a renderer path
that's hard to verify by inspection — RT shadows, volumetric
inscatter, transparent material stacks, etc. Aim for the smallest
scene that exercises the path; the cube demo is the baseline-coverage
test, and additional scenes should add NEW coverage rather than
overlap.
