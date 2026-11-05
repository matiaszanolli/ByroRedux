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
    uint  occluderSurface; // #6250 — the selected shadow ray's BLOCKER's
                       // stable surface ID (0 when the ray found no blocker
                       // this frame). Next frame's reprojection taints the
                       // pixel when this surface appears in binding 25's
                       // moved set, so a mover's first moving frame — and a
                       // teleport — clears the deep tail on the pixels it
                       // used to shadow. std430 stride 36 B.
};

#endif // BYRO_RESERVOIR_GLSL
