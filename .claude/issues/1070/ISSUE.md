# Issue #1070: F-WAT-10 — traceWaterRay constant colour needs tracking comment

**State**: OPEN | **Severity**: LOW | **Domain**: renderer (water shader)
**Location**: `crates/renderer/shaders/water.frag`

Short-term fix: add TODO(M38-Phase2/#1070) comment in traceWaterRay explaining
that the constant colour return is a deliberate design trade-off (no MaterialBuffer
/vertex/index SSBO bindings on the water pipeline) and documenting the path to fix it.
