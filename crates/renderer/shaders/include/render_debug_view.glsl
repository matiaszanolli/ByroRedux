// Main-pass participation in the structured correctness views
// (`GpuCamera.renderDebug.x`, `RENDER_DEBUG_*`) — #4979.
//
// Every structured view except FINAL is a raw frame-graph oracle: composite
// passes attachment 0 straight through (`rawDebug` in composite.frag) and
// presentation clamps it as a categorical / scalar value. A main-pass
// fragment shader that does not implement the active view must therefore
// not write its lit HDR colour there. `triangle.frag` recedes to flat grey
// under the water views, which it does not own; `water.frag` and
// `groundcover_blade.frag` apply this same rule to every view they do not
// own.
#ifndef BYRO_RENDER_DEBUG_VIEW_GLSL
#define BYRO_RENDER_DEBUG_VIEW_GLSL

// The flat grey a non-participating surface paints (triangle.frag's
// water-view recession uses the same value).
#define RENDER_DEBUG_NON_PARTICIPANT_GREY vec3(0.08)

// True when `mode` is a raw structured view. FINAL and LEGACY_FLAGS keep the
// normal output (legacy bits are triangle.frag's own ablations);
// COMPOSITE_TERM and VOLUMETRIC_TERM are drawn by composite from the normal
// frame; an out-of-range mode is painted magenta by composite regardless.
// A caller tests its own views first and recedes for the rest.
bool renderDebugSurfaceRecedes(uint mode) {
    return mode != RENDER_DEBUG_FINAL
        && mode != RENDER_DEBUG_LEGACY_FLAGS
        && mode != RENDER_DEBUG_COMPOSITE_TERM
        && mode != RENDER_DEBUG_VOLUMETRIC_TERM
        && mode <= RENDER_DEBUG_MODE_MAX;
}

#endif
