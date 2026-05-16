# REN-D13-003: Caustic initialize_layouts uses deprecated TOP_OF_PIPE

**GitHub**: #1100
**Domain**: renderer (Vulkan sync)
**Severity**: LOW

## Root Cause
caustic.rs:654: src_stage = TOP_OF_PIPE in initialize_layouts cmd_pipeline_barrier.
Same pattern in gbuffer.rs:359 (#949). TOP_OF_PIPE as srcStageMask on layout 
transitions is deprecated in Vulkan 1.3; generates validation noise on some drivers.

## Fix (2 files)
- caustic.rs:654: TOP_OF_PIPE → NONE
- gbuffer.rs:359: TOP_OF_PIPE → NONE (sibling fix, same issue)
ash 0.38 confirms PipelineStageFlags::NONE exists.
