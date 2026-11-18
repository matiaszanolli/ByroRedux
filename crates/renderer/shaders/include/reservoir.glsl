// The one GLSL declaration of the ReSTIR reservoir record: `triangle.frag` writes
// it and `shadow_filter.comp` decodes it. Rust holds no mirrored struct, only the
// byte stride `RESERVOIR_STRIDE` (`vulkan/restir.rs`), which every field being a
// 4-byte scalar keeps equal to this record's std430 stride. Struct-only, so any
// stage can include it.
#ifndef BYRO_RESERVOIR_GLSL
#define BYRO_RESERVOIR_GLSL

struct Reservoir {
    uint  lightAndSurface; // low 10b light index, high 22b surface ID
    float W;           // unbiased contribution weight (w_sum / (M * pHat))
    float M;           // effective sample count (capped)
    float histLenAndDepth; // bitcast packHalf2x16(history length, camera distance)
    float accumR;      // accumulated direct-shadow ratio (shaded / unshadowed,
    float accumG;      //   #4942) — R, G, B; triangle.frag rescales it by
    float accumB;      //   the current frame's unshadowed sum
    float pad0;        // geometric normal: octEncode → packSnorm2x16 → float
                       // bits. Consumed by temporal + spatial rejection;
                       // kept the struct at 32 B pre-#6250.
    uint  occluderAndShadowSequence; // low 22b: stable blocker ID (#6250)
                       // for mover taint, zero when there is no blocker.
                       // high 10b: shadow sequence key carried by validated
                       // receiver history. Zero uses the screen pixel.
                       // std430 stride 36 B.
};

#endif // BYRO_RESERVOIR_GLSL
