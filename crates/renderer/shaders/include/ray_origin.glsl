#ifndef BYRO_RAY_ORIGIN_GLSL
#define BYRO_RAY_ORIGIN_GLSL

// Scale-aware floating-point ray-origin offset from Wachter & Binder,
// Ray Tracing Gems ch. 6 (2019). Moving by a fixed world-space epsilon is
// not stable across Bethesda's interior, exterior, and Starfield coordinate
// ranges: the epsilon either rounds away or skips nearby geometry.
//
// The bit step MUST be evaluated in camera-relative space. Applying the
// reference 256-ULP step directly to an absolute coordinate of 1,000,000
// moves a ray origin by 16 world units and can jump completely past a nearby
// blocker. Relative coordinates stay within one render-origin cell plus ray
// reach; adding the origin back rounds the result to an actually representable
// absolute point (at least one absolute-coordinate ULP when needed).
vec3 offsetRayOrigin(vec3 p, vec3 n) {
    const float ORIGIN = 1.0 / 32.0;
    const float FLOAT_SCALE = 1.0 / 65536.0;
    const float INT_SCALE = 256.0;

    vec3 relativePoint = p - renderOrigin.xyz;
    ivec3 normalOffset = ivec3(INT_SCALE * n);
    vec3 stepped = vec3(
        intBitsToFloat(floatBitsToInt(relativePoint.x)
            + (relativePoint.x < 0.0 ? -normalOffset.x : normalOffset.x)),
        intBitsToFloat(floatBitsToInt(relativePoint.y)
            + (relativePoint.y < 0.0 ? -normalOffset.y : normalOffset.y)),
        intBitsToFloat(floatBitsToInt(relativePoint.z)
            + (relativePoint.z < 0.0 ? -normalOffset.z : normalOffset.z))
    );
    vec3 relativeOffset = vec3(
        abs(relativePoint.x) < ORIGIN
            ? relativePoint.x + FLOAT_SCALE * n.x : stepped.x,
        abs(relativePoint.y) < ORIGIN
            ? relativePoint.y + FLOAT_SCALE * n.y : stepped.y,
        abs(relativePoint.z) < ORIGIN
            ? relativePoint.z + FLOAT_SCALE * n.z : stepped.z
    );
    return relativeOffset + renderOrigin.xyz;
}

// Offset to the side of a geometric surface selected by the outgoing ray.
// Callers do not have to preserve a particular normal orientation: this
// helper flips it when the ray leaves through the opposite face.
vec3 offsetRayOriginForDirection(vec3 p, vec3 n, vec3 direction) {
    vec3 orientedNormal = dot(n, direction) >= 0.0 ? n : -n;
    return offsetRayOrigin(p, orientedNormal);
}

#endif
