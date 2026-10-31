// Combustion transport: sanitizers, differentials, boundary tests, wind,
// BFECC, and the per-cell update (advect, react, cool, dissipate). Storage
// agnostic — the one seam is `samplePreviousTransport` (declared below, defined
// by the including shader). Shared by the froxel inject pass and the local-grid
// sim pass (tracking #6531, Phase 1 #6533). Moved verbatim from
// volumetrics_inject.comp. All rates come from combustion.rs via
// shader_constants.glsl.
//
// Needs, before the include: shader_constants.glsl, volumetrics_bindings.glsl,
// combustion_sources.glsl.
#ifndef BYRO_COMBUSTION_TRANSPORT_GLSL
#define BYRO_COMBUSTION_TRANSPORT_GLSL

vec4 sanitizedChemistry(vec4 chemistry) {
    if (any(isnan(chemistry)) || any(isinf(chemistry))) {
        return vec4(0.0, AMBIENT_TEMPERATURE_K, 0.0, 0.0);
    }
    chemistry.x = clamp(chemistry.x, 0.0, 1.0);
    chemistry.y = chemistry.y > 1.0
        ? clamp(chemistry.y, AMBIENT_TEMPERATURE_K, COMBUSTION_MAX_TEMPERATURE_K)
        : AMBIENT_TEMPERATURE_K;
    chemistry.z = clamp(chemistry.z, 0.0, 0.20);
    chemistry.w = clamp(chemistry.w, 0.0, 64.0);
    return chemistry;
}

vec4 sanitizedDynamics(vec4 dynamics) {
    if (any(isnan(dynamics)) || any(isinf(dynamics))) {
        return vec4(0.0);
    }
    float maxSpeed = COMBUSTION_MAX_SPEED_MPS * WORLD_UNITS_PER_METER;
    float speed = length(dynamics.xyz);
    if (speed > maxSpeed) {
        dynamics.xyz *= maxSpeed / speed;
    }
    dynamics.w = clamp(
        dynamics.w,
        0.0,
        0.5 * COMBUSTION_MAX_SPEED_MPS * COMBUSTION_MAX_SPEED_MPS
    );
    return dynamics;
}

vec4 sanitizedOptical(vec4 optical) {
    if (any(isnan(optical)) || any(isinf(optical))) {
        return vec4(0.0);
    }
    optical.rgb = clamp(optical.rgb, vec3(0.0), vec3(0.20));
    optical.w = 0.0;
    return optical;
}
// SEAM — each including shader defines this AFTER the include, over its own
// storage for the previous-frame field (the froxel pass reprojects through
// its camera grid; a world-space local grid indexes its own volume). It must
// return sanitized state and false when the position has no history.
bool samplePreviousTransport(
    vec3 worldPos,
    out vec4 chemistry,
    out vec4 dynamics,
    out vec4 optical
);


float combustionActivity(vec4 chemistry, vec4 optical);

// One concern-specific differential derived from the six transported
// neighbours. Pressure expansion, rotational flow, and the combustion
// interface all consume this same world-space reconstruction rather than
// restating camera-grid finite differences independently.
struct CombustionDifferential {
    vec3 pressureGradient;
    vec3 vorticity;
    vec3 activityGradient;
    float velocityDivergence;
};

// Recover physical derivatives from the frustum-aligned froxel basis. A plain
// sum of axis differences would treat the perspective grid as orthogonal and
// turn camera orientation into combustion dynamics. With neighbour values D
// and basis columns B, scalar gradients solve Bᵀ∇f = D while the velocity
// Jacobian solves J B = Dᵤ.
bool previousCombustionDifferential(
    vec3 worldPosition,
    vec3 stepX,
    vec3 stepY,
    vec3 stepZ,
    out CombustionDifferential differential
) {
    differential.pressureGradient = vec3(0.0);
    differential.vorticity = vec3(0.0);
    differential.activityGradient = vec3(0.0);
    differential.velocityDivergence = 0.0;
    mat3 basis = mat3(stepX, stepY, stepZ);
    float basisDeterminant = determinant(basis);
    if (abs(basisDeterminant) <= 1.0e-8) {
        return false;
    }

    vec3 offsets[6] = vec3[6](
        stepX, -stepX,
        stepY, -stepY,
        stepZ, -stepZ
    );
    vec4 chemistry[6];
    vec4 dynamics[6];
    vec4 optical[6];
    for (int neighborIndex = 0; neighborIndex < 6; ++neighborIndex) {
        if (!samplePreviousTransport(
                worldPosition + offsets[neighborIndex],
                chemistry[neighborIndex],
                dynamics[neighborIndex],
                optical[neighborIndex])) {
            return false;
        }
    }

    mat3 inverseBasis = inverse(basis);
    mat3 inverseBasisTranspose = transpose(inverseBasis);
    vec3 pressureDifferences = 0.5 * vec3(
        dynamics[0].w - dynamics[1].w,
        dynamics[2].w - dynamics[3].w,
        dynamics[4].w - dynamics[5].w
    );
    differential.pressureGradient =
        inverseBasisTranspose * pressureDifferences;

    mat3 velocityDifferences = mat3(
        0.5 * (dynamics[0].xyz - dynamics[1].xyz),
        0.5 * (dynamics[2].xyz - dynamics[3].xyz),
        0.5 * (dynamics[4].xyz - dynamics[5].xyz)
    );
    mat3 velocityJacobian = velocityDifferences * inverseBasis;
    differential.vorticity = vec3(
        velocityJacobian[1][2] - velocityJacobian[2][1],
        velocityJacobian[2][0] - velocityJacobian[0][2],
        velocityJacobian[0][1] - velocityJacobian[1][0]
    );
    differential.velocityDivergence = velocityJacobian[0][0]
        + velocityJacobian[1][1]
        + velocityJacobian[2][2];

    vec3 activityDifferences = 0.5 * vec3(
        combustionActivity(chemistry[0], optical[0])
            - combustionActivity(chemistry[1], optical[1]),
        combustionActivity(chemistry[2], optical[2])
            - combustionActivity(chemistry[3], optical[3]),
        combustionActivity(chemistry[4], optical[4])
            - combustionActivity(chemistry[5], optical[5])
    );
    differential.activityGradient =
        inverseBasisTranspose * activityDifferences;

    return !any(isnan(differential.pressureGradient))
        && !any(isinf(differential.pressureGradient))
        && !any(isnan(differential.vorticity))
        && !any(isinf(differential.vorticity))
        && !any(isnan(differential.activityGradient))
        && !any(isinf(differential.activityGradient))
        && !isnan(differential.velocityDivergence)
        && !isinf(differential.velocityDivergence);
}

bool carriesCombustion(vec4 chemistry, vec4 optical) {
    return chemistry.x > 1.0e-4
        || chemistry.z > 1.0e-6
        || any(greaterThan(optical.rgb, vec3(1.0e-6)))
        || chemistry.w > 1.0e-4
        || chemistry.y > AMBIENT_TEMPERATURE_K + 2.0;
}

float combustionActivity(vec4 chemistry, vec4 optical) {
    float temperatureExcess = max(
        (chemistry.y - AMBIENT_TEMPERATURE_K) / AMBIENT_TEMPERATURE_K,
        0.0
    );
    float spectralScattering = max(optical.r, max(optical.g, optical.b));
    return clamp(
        chemistry.x + max(chemistry.z, spectralScattering) * 80.0
            + temperatureExcess * 0.12,
        0.0,
        1.0
    );
}

bool rigidBoundaryNormal(
    uint instanceIndex,
    uint primitiveIndex,
    out vec3 boundaryNormal
) {
    boundaryNormal = vec3(0.0);
    if (instanceIndex >= uint(boundaryInstances.length())) {
        return false;
    }
    GpuBoundaryInstance instance = boundaryInstances[instanceIndex];
    // Skinned BLAS vertices are deformed into a separate per-entity buffer;
    // the global SSBO remains bind pose. Fall back to the conservative ray
    // direction for those hits rather than publishing a confidently wrong
    // normal.
    if (instance.boneOffset != 0u) {
        return false;
    }

    uint indexCount = uint(boundaryIndexData.length());
    if (instance.indexOffset > indexCount) {
        return false;
    }
    uint remainingIndices = indexCount - instance.indexOffset;
    if (primitiveIndex >= remainingIndices / 3u) {
        return false;
    }
    uint firstIndex = instance.indexOffset + primitiveIndex * 3u;
    uvec3 localIndices = uvec3(
        boundaryIndexData[firstIndex],
        boundaryIndexData[firstIndex + 1u],
        boundaryIndexData[firstIndex + 2u]
    );
    if (any(greaterThanEqual(localIndices, uvec3(instance.vertexCount)))) {
        return false;
    }

    uint vertexCapacity = uint(boundaryVertexData.length()) / VERTEX_STRIDE_FLOATS;
    if (instance.vertexOffset > vertexCapacity
        || any(greaterThanEqual(
            localIndices,
            uvec3(vertexCapacity - instance.vertexOffset)
        ))) {
        return false;
    }
    uvec3 globalIndices = uvec3(instance.vertexOffset) + localIndices;
    uint base0 = globalIndices.x * VERTEX_STRIDE_FLOATS;
    uint base1 = globalIndices.y * VERTEX_STRIDE_FLOATS;
    uint base2 = globalIndices.z * VERTEX_STRIDE_FLOATS;
    vec3 local0 = vec3(
        boundaryVertexData[base0],
        boundaryVertexData[base0 + 1u],
        boundaryVertexData[base0 + 2u]
    );
    vec3 local1 = vec3(
        boundaryVertexData[base1],
        boundaryVertexData[base1 + 1u],
        boundaryVertexData[base1 + 2u]
    );
    vec3 local2 = vec3(
        boundaryVertexData[base2],
        boundaryVertexData[base2 + 1u],
        boundaryVertexData[base2 + 2u]
    );
    vec3 world0 = (instance.model * vec4(local0, 1.0)).xyz;
    vec3 world1 = (instance.model * vec4(local1, 1.0)).xyz;
    vec3 world2 = (instance.model * vec4(local2, 1.0)).xyz;
    vec3 unnormalized = cross(world1 - world0, world2 - world0);
    float lengthSquared = dot(unnormalized, unnormalized);
    if (!(lengthSquared > 1.0e-12)
        || isnan(lengthSquared)
        || isinf(lengthSquared)) {
        return false;
    }
    boundaryNormal = unnormalized * inversesqrt(lengthSquared);
    return true;
}

// Both backward advection and the forward velocity update use this one
// canonical solid-boundary query. Glass blocks transported matter even though
// it transmits light; effect cards do neither. A malformed/degenerate or
// skinned hit still blocks transport and receives a conservative normal that
// cancels motion into the committed surface.
bool combustionPathBlocked(
    vec3 sourcePosition,
    vec3 destinationPosition,
    out vec3 boundaryNormal
) {
    boundaryNormal = vec3(0.0);
    vec3 displacement = destinationPosition - sourcePosition;
    float distance = length(displacement);
    if (distance <= 0.04) {
        return false;
    }
    vec3 direction = displacement / distance;
    float inset = min(0.02, distance * 0.25);
    rayQueryEXT boundaryQuery;
    rayQueryInitializeEXT(
        boundaryQuery,
        topLevelAS,
        gl_RayFlagsOpaqueEXT | gl_RayFlagsTerminateOnFirstHitEXT,
        VISIBILITY_MASK_SOLID,
        sourcePosition,
        inset,
        direction,
        distance - inset
    );
    while (rayQueryProceedEXT(boundaryQuery)) {}
    if (rayQueryGetIntersectionTypeEXT(boundaryQuery, true)
        == gl_RayQueryCommittedIntersectionNoneEXT) {
        return false;
    }

    uint instanceIndex = uint(
        rayQueryGetIntersectionInstanceCustomIndexEXT(boundaryQuery, true));
    uint primitiveIndex = uint(
        rayQueryGetIntersectionPrimitiveIndexEXT(boundaryQuery, true));
    if (!rigidBoundaryNormal(instanceIndex, primitiveIndex, boundaryNormal)) {
        boundaryNormal = -direction;
    } else if (dot(boundaryNormal, direction) > 0.0) {
        boundaryNormal = -boundaryNormal;
    }
    return true;
}

vec3 atmosphericWindVelocity(vec3 worldPos);

// Semi-Lagrangian advection normally backtraces with the velocity already at
// the destination. That is insufficient at a plume's leading edge: the empty
// destination has zero velocity, so it can never discover the active cell
// immediately upwind. Extend only velocity (not chemistry) from an adjacent
// active cell whose flow points toward this destination. Chemistry still
// arrives through the ordinary backtrace, preserving mass-like interpolation
// and the solid-crossing rejection below.
bool incomingDynamicsFromNeighbors(
    vec3 worldPos,
    vec3 stepX,
    vec3 stepY,
    vec3 stepZ,
    out vec4 incomingDynamics
) {
    vec3 offsets[6] = vec3[6](
        stepX, -stepX,
        stepY, -stepY,
        stepZ, -stepZ
    );
    bool found = false;
    float bestScore = 0.0;
    incomingDynamics = vec4(0.0);
    for (int neighborIndex = 0; neighborIndex < 6; ++neighborIndex) {
        vec3 candidatePosition = worldPos + offsets[neighborIndex];
        vec4 candidateChemistry;
        vec4 candidateDynamics;
        vec4 candidateOptical;
        if (!samplePreviousTransport(
                candidatePosition,
                candidateChemistry,
                candidateDynamics,
                candidateOptical)
            || !carriesCombustion(candidateChemistry, candidateOptical)) {
            continue;
        }

        vec3 towardDestination = worldPos - candidatePosition;
        float neighborDistance = length(towardDestination);
        if (neighborDistance <= 1.0e-4) {
            continue;
        }
        float closingSpeed = dot(
            candidateDynamics.xyz + atmosphericWindVelocity(candidatePosition),
            towardDestination / neighborDistance
        );
        if (closingSpeed <= 0.04) {
            continue;
        }
        vec3 crossingNormal;
        if (combustionPathBlocked(
                candidatePosition,
                worldPos,
                crossingNormal)) {
            continue;
        }

        float activity = combustionActivity(candidateChemistry, candidateOptical);
        float score = closingSpeed * (0.25 + activity);
        if (!found || score > bestScore) {
            found = true;
            bestScore = score;
            incomingDynamics = candidateDynamics;
        }
    }
    return found;
}

// Weather wind is an external advection field, not a force written into the
// transported velocity image. Keeping the two separate is important: a gust
// can change between frames without being integrated repeatedly into an
// ever-growing velocity. The small spatial phase gives a coherent front to
// the gust instead of making every smoke column pulse in lockstep. A mild
// height shear lets the upper plume outrun its grounded source, which is what
// exposes the roll-up/vortex structure instead of translating a rigid blob.
vec3 atmosphericWindVelocity(vec3 worldPos) {
    vec2 direction = params.wind_params.xy;
    float directionLength = length(direction);
    if (directionLength <= 1.0e-5) {
        return vec3(0.0);
    }
    direction /= directionLength;
    float baseSpeed = max(params.wind_params.z, 0.0);
    float gustAmplitude = max(params.wind_params.w, 0.0);
    float gustFrequency = max(params.wind_gust.x, 0.0);
    float phase = params.temporal_params.z
        * gustFrequency
        * 2.0 * PI
        + dot(worldPos.xz, direction) * 0.008;
    float speed = max(baseSpeed + gustAmplitude * sin(phase), 0.0);
    float heightMetres = (worldPos.y - params.fog_reference.x)
        / WORLD_UNITS_PER_METER;
    float shear = mix(0.72, 1.18, smoothstep(-2.0, 18.0, heightMetres));
    return vec3(direction.x, 0.0, direction.y) * speed * shear;
}

// Single-pass BFECC (Selle et al. 2008) error correction for the transported
// dye fields. Semi-Lagrangian backtrace is unconditionally stable but
// dissipative: it low-pass filters chemistry and optics every step, which is
// why the procedural curl forcing below has to re-inject the detail it smears
// away. The correction estimates that one-step error by differencing the
// field against a forward/backward trace pair,
//   e = phi(x) - phi(y - v(y) dt),   y = x + v(x) dt,
// and adds strength * e to the backtraced result. Every term is a previous-
// grid sample, so this stays one dispatch over the existing frame-in-flight
// pair — no intermediate image and no intra-dispatch hazard. The corrected
// value is clamped to the min/max of the eight previous-grid texels feeding
// the source-position trilinear tap, the classic limiter that keeps the
// scheme unconditionally stable. Velocity is deliberately not corrected:
// feeding the error back into dynamics injects energy at curl
// discontinuities, while the dye channels carry all the visible dissipation.
void applyBfeccErrorCorrection(
    vec3 worldPos,
    vec3 destinationVelocity,
    vec3 destinationWind,
    vec4 probeChemistry,
    vec4 probeOptical,
    vec3 sourcePosition,
    inout vec4 chemistry,
    inout vec4 optical,
    float dt
) {
    float strength = COMBUSTION_BFECC_ERROR_CORRECTION_STRENGTH
        * params.wind_gust.y;
    if (strength <= 0.0) {
        return;
    }

    // The error trace must land in active medium on both ends; an empty
    // forward or error sample carries no dissipation estimate.
    vec3 forwardPosition = worldPos
        + (destinationVelocity + destinationWind) * dt;
    vec4 forwardChemistry;
    vec4 forwardDynamics;
    vec4 forwardOptical;
    if (!samplePreviousTransport(
            forwardPosition,
            forwardChemistry,
            forwardDynamics,
            forwardOptical)
        || !carriesCombustion(forwardChemistry, forwardOptical)) {
        return;
    }
    vec3 errorPosition = forwardPosition
        - (forwardDynamics.xyz + atmosphericWindVelocity(forwardPosition)) * dt;
    vec4 errorChemistry;
    vec4 errorDynamics;
    vec4 errorOptical;
    if (!samplePreviousTransport(
            errorPosition,
            errorChemistry,
            errorDynamics,
            errorOptical)
        || !carriesCombustion(errorChemistry, errorOptical)) {
        return;
    }

    vec4 chemistryError = probeChemistry - errorChemistry;
    vec4 opticalError = probeOptical - errorOptical;
    // A significant error on a segment crossing a solid would reintroduce
    // the wall bleed the source-side rejection in the caller prevents. The
    // TLAS trace is paid only when the normalized error is large enough to
    // matter; small bleeds are already bounded by the limiter clamp below.
    vec4 chemistryScale = vec4(1.0, 1000.0, 0.10, 8.0);
    vec3 opticalScale = vec3(0.10);
    vec4 scaledChemistryError = abs(chemistryError) / chemistryScale;
    vec3 scaledOpticalError = abs(opticalError.rgb) / opticalScale;
    float normalizedError = max(
        max(
            max(scaledChemistryError.x, scaledChemistryError.y),
            max(scaledChemistryError.z, scaledChemistryError.w)
        ),
        max(
            max(scaledOpticalError.r, scaledOpticalError.g),
            scaledOpticalError.b
        )
    );
    if (normalizedError > COMBUSTION_BFECC_ERROR_TRACE_THRESHOLD) {
        vec3 crossingNormal;
        if (combustionPathBlocked(errorPosition, forwardPosition, crossingNormal)) {
            return;
        }
    }

    // Limiter: the corrected value cannot leave the range of the eight
    // previous-grid texels that feed the source-position trilinear tap.
    vec3 sourceUvw;
    if (!reprojectHistory(sourcePosition, sourceUvw)) {
        return;
    }
    ivec3 size = textureSize(previousCombustionState, 0);
    ivec3 base = ivec3(floor(sourceUvw * vec3(size) - vec3(0.5)));
    vec4 chemistryMin = vec4(0.0, 1.0e9, 0.0, 0.0);
    vec4 chemistryMax = vec4(0.0, -1.0e9, 0.0, 0.0);
    vec4 opticalMin = vec4(1.0e9);
    vec4 opticalMax = vec4(-1.0e9);
    for (int corner = 0; corner < 8; ++corner) {
        ivec3 offset = ivec3(corner & 1, (corner >> 1) & 1, (corner >> 2) & 1);
        ivec3 coord = clamp(base + offset, ivec3(0), size - ivec3(1));
        vec4 cornerChemistry = texelFetch(previousCombustionState, coord, 0);
        vec4 cornerOptical = texelFetch(previousCombustionOptical, coord, 0);
        chemistryMin = min(chemistryMin, cornerChemistry);
        chemistryMax = max(chemistryMax, cornerChemistry);
        opticalMin = min(opticalMin, cornerOptical);
        opticalMax = max(opticalMax, cornerOptical);
    }
    chemistry = clamp(
        chemistry + strength * chemistryError,
        chemistryMin,
        chemistryMax
    );
    optical.rgb = clamp(
        optical.rgb + strength * opticalError.rgb,
        opticalMin.rgb,
        opticalMax.rgb
    );
}

// Advect in absolute world space even though storage is view-froxel aligned.
// RK2 backtracing reduces the directional bias of first-order Euler transport:
// sample velocity at the destination, estimate a midpoint, then use midpoint
// velocity for the final source position in the previous grid.
void transportCombustion(
    vec3 worldPos,
    vec3 segmentStart,
    vec3 segmentEnd,
    vec3 neighborStepX,
    vec3 neighborStepY,
    vec3 neighborStepZ,
    float dt,
    bool expired,
    bool transportOccupied,
    out vec4 chemistry,
    out vec4 dynamics,
    out vec4 optical
) {
    vec4 probeChemistry;
    vec4 destinationDynamics;
    vec4 probeOptical;
    // #4775 — once transport has lapsed the residual is dropped, not
    // carried: `dt == 0` alone would freeze the ≤ 3 % tail the linger
    // window was sized to cut, as a permanent world-anchored haze.
    bool hadHistory = !expired && samplePreviousTransport(
        worldPos,
        probeChemistry,
        destinationDynamics,
        probeOptical
    );
    // #4784 — an occupancy miss skips the whole RK2 block (the six-neighbour
    // gather, midpoint + source probes): this froxel's cell and its 26
    // neighbours carried no combustion last frame, so no transported mass can
    // arrive here this frame. Falling through to the carry below is exactly
    // the dt == 0 path the scene-wide gate (#3131) already takes.
    if (hadHistory && dt > 0.0 && transportOccupied) {
        bool destinationDynamicsExtended = false;
        if (combustionActivity(probeChemistry, probeOptical) < 0.08) {
            vec4 incomingDynamics;
            if (incomingDynamicsFromNeighbors(
                    worldPos,
                    neighborStepX,
                    neighborStepY,
                    neighborStepZ,
                    incomingDynamics)) {
                destinationDynamics = incomingDynamics;
                destinationDynamicsExtended = true;
            }
        }
        vec3 destinationWind = atmosphericWindVelocity(worldPos);
        vec3 midpoint = worldPos
            - (destinationDynamics.xyz + destinationWind) * (0.5 * dt);
        vec4 midpointChemistry;
        vec4 midpointDynamics;
        vec4 midpointOptical;
        if (destinationDynamicsExtended
            || !samplePreviousTransport(
                midpoint,
                midpointChemistry,
                midpointDynamics,
                midpointOptical)) {
            midpointDynamics = destinationDynamics;
        }
        vec3 midpointWind = atmosphericWindVelocity(midpoint);
        vec3 sourcePosition = worldPos
            - (midpointDynamics.xyz + midpointWind) * dt;
        vec4 sourceChemistry;
        vec4 sourceDynamics;
        vec4 sourceOptical;
        bool sourceInHistory = samplePreviousTransport(
            sourcePosition,
            sourceChemistry,
            sourceDynamics,
            sourceOptical
        );
        // Test the state being pulled into this destination, not the state
        // already living here. A previously empty cell on the far side of a
        // wall is exactly where a source-side check matters: gating on
        // `probeChemistry` lets active chemistry tunnel into that empty cell
        // without ever issuing the TLAS query.
        vec3 crossingNormal;
        bool blocked = sourceInHistory
            && carriesCombustion(sourceChemistry, sourceOptical)
            && combustionPathBlocked(sourcePosition, worldPos, crossingNormal);
        if (blocked) {
            chemistry = probeChemistry;
            dynamics = destinationDynamics;
            optical = probeOptical;
        } else if (sourceInHistory
            && carriesCombustion(sourceChemistry, sourceOptical)) {
            chemistry = sourceChemistry;
            dynamics = sourceDynamics;
            optical = sourceOptical;
            applyBfeccErrorCorrection(
                worldPos,
                destinationDynamics.xyz,
                destinationWind,
                probeChemistry,
                probeOptical,
                sourcePosition,
                chemistry,
                optical,
                dt
            );
        } else {
            chemistry = probeChemistry;
            dynamics = destinationDynamics;
            optical = probeOptical;
        }
    } else {
        chemistry = hadHistory
            ? probeChemistry
            : vec4(0.0, AMBIENT_TEMPERATURE_K, 0.0, 0.0);
        dynamics = hadHistory
            ? destinationDynamics
            : vec4(0.0);
        optical = hadHistory
            ? probeOptical
            : vec4(0.0);
    }

    chemistry = sanitizedChemistry(chemistry);
    dynamics = sanitizedDynamics(dynamics);
    optical = sanitizedOptical(optical);
    applyCombustionSources(
        worldPos,
        segmentStart,
        segmentEnd,
        chemistry,
        dynamics,
        optical,
        dt
    );

    if (dt > 0.0) {
        float ignition = smoothstep(
            COMBUSTION_IGNITION_TEMPERATURE_K - 90.0,
            COMBUSTION_IGNITION_TEMPERATURE_K + 180.0,
            chemistry.y
        );
        // `chemistry.x` is the canonical normalized fuel fraction. The
        // complementary fraction is available oxidizer, so a fuel-rich core
        // burns more slowly and produces more soot than a dilute edge. This
        // is shared thermochemistry, not a source/profile heuristic.
        float fuelFraction = clamp(chemistry.x, 0.0, 1.0);
        float oxidizer = 1.0 - fuelFraction;
        float reactionRate = COMBUSTION_REACTION_RATE_PER_SECOND
            * fuelFraction
            * oxidizer
            * ignition;
        float burnedFuel = min(fuelFraction, reactionRate * dt);
        chemistry.x -= burnedFuel;
        // Reaction heat approaches the canonical adiabatic equilibrium; it
        // is not a fixed kelvin increment. This makes the update independent
        // of timestep partitioning and, crucially, does not keep reheating an
        // explosion that entered the field hotter than an open flame.
        float temperatureDeficit = max(
            ADIABATIC_FLAME_TEMPERATURE_K - chemistry.y,
            0.0
        );
        float reactionHeatFraction = 1.0 - exp(
            -COMBUSTION_REACTION_HEAT_RESPONSE * burnedFuel
        );
        chemistry.y += temperatureDeficit * reactionHeatFraction;
        // Soot yield falls when oxidizer is abundant and rises in a rich,
        // oxygen-starved flame. Encode it directly as canonical total
        // extinction plus spectral scattering, so radiative transfer needs no
        // later age/material conversion.
        float sootYield = mix(
            COMBUSTION_RICH_SOOT_YIELD,
            COMBUSTION_LEAN_SOOT_YIELD,
            oxidizer
        );
        float sootSigmaT = burnedFuel * sootYield;
        chemistry.z += sootSigmaT;
        optical.rgb += sootSigmaT
            * COMBUSTION_SOOT_SINGLE_SCATTER_ALBEDO_RGB;
        float hotSootOxidation = smoothstep(
                COMBUSTION_SOOT_OXIDATION_START_TEMPERATURE_K,
                COMBUSTION_SOOT_OXIDATION_FULL_TEMPERATURE_K,
                chemistry.y
            )
            * oxidizer;
        float sootRetention = exp(
            -COMBUSTION_SOOT_OXIDATION_RATE_PER_SECOND
                * hotSootOxidation
                * dt
        );
        chemistry.z *= sootRetention;
        optical.rgb *= sootRetention;

        chemistry.x *= exp(
            -COMBUSTION_FUEL_VAPOUR_REMOVAL_PER_SECOND * dt
        );
        // Soot is transported matter, not another emissive time envelope.
        // Let advection and mixing disperse it; this mild settling/removal term
        // gives a ~15 s half-life instead of erasing an explosion cloud in the
        // first few seconds after its heat has cooled.
        chemistry.z *= exp(-COMBUSTION_AEROSOL_DISSIPATION_PER_SECOND * dt);
        optical.rgb *= exp(-COMBUSTION_AEROSOL_DISSIPATION_PER_SECOND * dt);
        chemistry.w *= exp(
            -COMBUSTION_RADIANCE_REMOVAL_PER_SECOND * dt
        );
        chemistry.y = AMBIENT_TEMPERATURE_K
            + (chemistry.y - AMBIENT_TEMPERATURE_K) * exp(
                -COMBUSTION_THERMAL_COOLING_PER_SECOND * dt
            );

        float activity = combustionActivity(chemistry, optical);
        CombustionDifferential differential;
        bool hasDifferential = false;
        if (dynamics.w > 1.0e-5 || activity > 0.01) {
            hasDifferential = previousCombustionDifferential(
                worldPos,
                neighborStepX,
                neighborStepY,
                neighborStepZ,
                differential
            );
        }

        // Positive divergence is resolved volumetric expansion. Dilute all
        // transported per-volume chemistry and radiance with that same local
        // Jacobian so an expanding fireball cannot keep a constant-density
        // sphere while its velocity field grows. Compression is left to
        // advection and bounded source injection rather than amplifying a
        // noisy finite difference.
        if (hasDifferential) {
            float dilutionRate = clamp(
                differential.velocityDivergence,
                0.0,
                COMBUSTION_MAX_DILUTION_RATE_PER_SECOND
            );
            float dilution = exp(-dilutionRate * dt);
            chemistry.x *= dilution;
            chemistry.z *= dilution;
            chemistry.w *= dilution;
            optical.rgb *= dilution;
        }

        // Specific overpressure q is transported beside velocity but remains
        // source-agnostic after injection. Recover -grad(q) in world space so
        // every pressure-producing source expands through the same dynamics
        // contract, then let the short pressure phase decay before buoyancy
        // owns the cooled plume. q is stored in m²/s² while the grid basis is
        // expressed in world units, hence the squared world-unit conversion.
        if (dynamics.w > 1.0e-5) {
            if (hasDifferential) {
                vec3 pressureAcceleration = -differential.pressureGradient
                    * (WORLD_UNITS_PER_METER * WORLD_UNITS_PER_METER);
                float pressureAccelerationLength = length(pressureAcceleration);
                float maxPressureAcceleration =
                    COMBUSTION_MAX_PRESSURE_ACCELERATION_MPS2
                    * WORLD_UNITS_PER_METER;
                if (pressureAccelerationLength > maxPressureAcceleration) {
                    pressureAcceleration *= maxPressureAcceleration
                        / pressureAccelerationLength;
                }
                dynamics.xyz += pressureAcceleration * dt;
            }
            dynamics.w *= exp(
                -COMBUSTION_OVERPRESSURE_DISSIPATION_PER_SECOND * dt
            );
        }

        // Semi-Lagrangian advection damps curl most aggressively at a thin
        // fire/smoke interface. Use the same transported-neighbour
        // differential to restore rotational motion there. This is an
        // interface force, not an explosion/flame profile branch: any hot or
        // optically active transported medium follows the same contract.
        if (hasDifferential && activity > 0.01) {
            float interfaceGradientLength =
                length(differential.activityGradient);
            float vorticityLength = length(differential.vorticity);
            float meanStepWorld = (
                length(neighborStepX)
                    + length(neighborStepY)
                    + length(neighborStepZ)
            ) / 3.0;
            float interfaceContrast = clamp(
                interfaceGradientLength * meanStepWorld,
                0.0,
                1.0
            );
            if (interfaceGradientLength > 1.0e-7
                && vorticityLength > 1.0e-4
                && interfaceContrast > 0.005) {
                vec3 interfaceNormal = differential.activityGradient
                    / interfaceGradientLength;
                // A pressure-dominated front should remain an expanding
                // blast, not immediately turn its discretization curl into
                // tangential motion. As transported q decays, confinement
                // fades in continuously and rolls the decompressed hot-gas /
                // smoke interface without knowing which source created it.
                float decompressedWeight = 1.0 - smoothstep(
                    0.04,
                    0.40,
                    dynamics.w
                );
                float confinementWeight = activity
                    * decompressedWeight
                    * smoothstep(0.005, 0.18, interfaceContrast);
                vec3 vorticityAcceleration = cross(
                    interfaceNormal,
                    differential.vorticity
                ) * (
                    COMBUSTION_VORTICITY_CONFINEMENT_SPEED_MPS
                        * WORLD_UNITS_PER_METER
                        * confinementWeight
                );
                float vorticityAccelerationLength =
                    length(vorticityAcceleration);
                float maxVorticityAcceleration =
                    COMBUSTION_MAX_VORTICITY_ACCELERATION_MPS2
                    * WORLD_UNITS_PER_METER;
                if (vorticityAccelerationLength > maxVorticityAcceleration) {
                    vorticityAcceleration *= maxVorticityAcceleration
                        / vorticityAccelerationLength;
                }
                dynamics.xyz += vorticityAcceleration * dt;
            }
        }

        float temperatureExcess = max(
            (chemistry.y - AMBIENT_TEMPERATURE_K) / AMBIENT_TEMPERATURE_K,
            0.0
        );
        float buoyantAcceleration = min(temperatureExcess, 7.0)
            * COMBUSTION_THERMAL_BUOYANCY_ACCELERATION_MPS2
            * WORLD_UNITS_PER_METER;
        dynamics.y += buoyantAcceleration * dt;
        float aerosolLift = clamp(
            chemistry.z * COMBUSTION_AEROSOL_LIFT_EXTINCTION_SCALE,
            0.0,
            1.0
        ) * COMBUSTION_AEROSOL_LIFT_ACCELERATION_MPS2
            * WORLD_UNITS_PER_METER;
        dynamics.y += aerosolLift * dt;

        if (activity > 0.0) {
            vec3 metres = worldPos / WORLD_UNITS_PER_METER;
            float coarseWaveNumber = 2.0 * PI
                / COMBUSTION_TURBULENCE_COARSE_EDDY_SCALE_METERS;
            float detailWaveNumber = 2.0 * PI
                / COMBUSTION_TURBULENCE_DETAIL_EDDY_SCALE_METERS;
            vec3 coarseCurl = curlField(
                metres * coarseWaveNumber
                    + vec3(
                        0.06,
                        -COMBUSTION_TURBULENCE_COARSE_RISE_SPEED_MPS,
                        0.04
                    ) * (coarseWaveNumber * params.temporal_params.z)
            );
            const float detailRotation = 0.7417649321;
            vec3 detailCurl = rotateY(
                curlField(
                    rotateY(metres, detailRotation) * detailWaveNumber
                        + vec3(
                            -0.08,
                            -COMBUSTION_TURBULENCE_DETAIL_RISE_SPEED_MPS,
                            0.11
                        ) * (detailWaveNumber * params.temporal_params.z)
                ),
                -detailRotation
            );
            vec3 turbulentAcceleration = (
                coarseCurl * 0.72 + detailCurl * 0.42
            ) * (1.1 * WORLD_UNITS_PER_METER * activity);
            dynamics.xyz += turbulentAcceleration * dt;
        }
        dynamics.xyz *= exp(
            -COMBUSTION_VELOCITY_DAMPING_PER_SECOND * dt
        );

        // Semi-Lagrangian no-through rejection above protects the empty side
        // of a wall. This forward probe handles the complementary source-side
        // boundary condition: remove only the inward normal component of the
        // updated flow and retain tangential slip with mild wall friction.
        if (carriesCombustion(chemistry, optical)
            && dot(dynamics.xyz, dynamics.xyz) > 1.0e-8) {
            vec3 boundaryNormal;
            vec3 predictedPosition = worldPos
                + (dynamics.xyz + atmosphericWindVelocity(worldPos)) * dt;
            if (combustionPathBlocked(
                    worldPos,
                    predictedPosition,
                    boundaryNormal)) {
                float inwardSpeed = min(dot(dynamics.xyz, boundaryNormal), 0.0);
                dynamics.xyz -= boundaryNormal * inwardSpeed;
                dynamics.xyz *= exp(-COMBUSTION_WALL_FRICTION_PER_SECOND * dt);
            }
        }
    }

    chemistry = sanitizedChemistry(chemistry);
    dynamics = sanitizedDynamics(dynamics);
    optical = sanitizedOptical(optical);
}

LocalMedium combustionMedium(vec4 chemistry, vec4 optical) {
    LocalMedium medium = emptyLocalMedium();
    chemistry = sanitizedChemistry(chemistry);
    optical = sanitizedOptical(optical);
    float fuelVapourSigmaT = chemistry.x * COMBUSTION_FUEL_VAPOUR_SIGMA_T;
    float sigmaT = chemistry.z + fuelVapourSigmaT;
    if (sigmaT <= 1.0e-7) {
        return medium;
    }

    vec3 sigmaS = optical.rgb
        + fuelVapourSigmaT * vec3(COMBUSTION_FUEL_VAPOUR_SINGLE_SCATTER_ALBEDO);
    sigmaS = clamp(sigmaS, vec3(0.0), vec3(sigmaT));
    vec3 sigmaA = vec3(sigmaT) - sigmaS;
    medium.extinction = sigmaT;
    medium.scattering = sigmaS;

    float thermalLuma = chemistry.w
        * blackbodyVisibleRadianceRatio(chemistry.y, 1850.0);
    vec3 thermalRadiance = blackbodyChromaticity(chemistry.y) * thermalLuma;
    medium.emission = sigmaA * thermalRadiance;
    return medium;
}

#endif
