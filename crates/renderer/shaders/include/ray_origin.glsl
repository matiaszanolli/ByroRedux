#ifndef BYRO_RAY_ORIGIN_GLSL
#define BYRO_RAY_ORIGIN_GLSL

// Scale-aware floating-point ray-origin offset from Wachter & Binder,
// Ray Tracing Gems ch. 6 (2019). Moving by a fixed world-space epsilon is
// not stable across Bethesda's interior, exterior, and Starfield coordinate
// ranges: the epsilon either rounds away or skips nearby geometry. This
// advances the point by a bounded number of representable floats instead,
// with a small additive fallback around the origin.
vec3 offsetRayOrigin(vec3 p, vec3 n) {
    const float ORIGIN = 1.0 / 32.0;
    const float FLOAT_SCALE = 1.0 / 65536.0;
    const float INT_SCALE = 256.0;

    ivec3 normalOffset = ivec3(INT_SCALE * n);
    vec3 stepped = vec3(
        intBitsToFloat(floatBitsToInt(p.x)
            + (p.x < 0.0 ? -normalOffset.x : normalOffset.x)),
        intBitsToFloat(floatBitsToInt(p.y)
            + (p.y < 0.0 ? -normalOffset.y : normalOffset.y)),
        intBitsToFloat(floatBitsToInt(p.z)
            + (p.z < 0.0 ? -normalOffset.z : normalOffset.z))
    );
    return vec3(
        abs(p.x) < ORIGIN ? p.x + FLOAT_SCALE * n.x : stepped.x,
        abs(p.y) < ORIGIN ? p.y + FLOAT_SCALE * n.y : stepped.y,
        abs(p.z) < ORIGIN ? p.z + FLOAT_SCALE * n.z : stepped.z
    );
}

// Offset to the side of a geometric surface selected by the outgoing ray.
// Callers do not have to preserve a particular normal orientation: this
// helper flips it when the ray leaves through the opposite face.
vec3 offsetRayOriginForDirection(vec3 p, vec3 n, vec3 direction) {
    vec3 orientedNormal = dot(n, direction) >= 0.0 ? n : -n;
    return offsetRayOrigin(p, orientedNormal);
}

#endif
