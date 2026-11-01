// Combustion medium model: authored local media, profile geometry, source
// evaluation (flame / smoke / explosion), and the noise helpers they use.
// Pure functions of world position plus the set-0 bindings from
// volumetrics_bindings.glsl; nothing here knows how the transported field is
// stored. Shared by the froxel inject pass and the local-grid sim pass
// (tracking #6531, Phase 1 #6533). Moved verbatim from volumetrics_inject.comp.
//
// Needs, before the include: shader_constants.glsl, volumetrics_bindings.glsl.
// Its prototypes `warpFogDomain` / `sampledDensity` are defined by the
// froxel shader (they sample its density noise volumes).
#ifndef BYRO_COMBUSTION_SOURCES_GLSL
#define BYRO_COMBUSTION_SOURCES_GLSL

#include "include/combustion_medium.glsl"

const float PI = 3.14159265358979323846;
// Rec. 709 luma weights come from the generated header's `LUMA_REC709`,
// pinned to `byroredux_core::radiometry::LINEAR_SRGB_LUMA` so CPU and GPU agree
// on what "brightness" means (#4347).
const float COMBUSTION_WALL_FRICTION_PER_SECOND = 0.35;
// FOG_VOLUME_CLUSTER_DIM / MAX_FOG_VOLUMES_PER_CLUSTER come from the
// #include'd shader_constants.glsl header (generated from
// shader_constants_data.rs — #2229 / REN-D3-02). Previously hand-declared
// here as local literals with no shared source with the Rust-side
// `vulkan::volumetrics` copies.

vec3 warpFogDomain(vec3 worldMetres, float timeSeconds);
float sampledDensity(vec3 metres, float baseScale, float detailScale);

vec3 rotateByQuaternion(vec3 value, vec4 q) {
    return value + 2.0 * cross(q.xyz, cross(q.xyz, value) + q.w * value);
}


// Light-space transmittance march budget (`transportedCombustionTransmittance`).
const int COMBUSTION_LIGHT_TRANSMITTANCE_STEPS = 8;
const float COMBUSTION_LIGHT_TRANSMITTANCE_REACH_METERS = 2.0;

// Cheap deterministic phase offset so nearby fireplaces do not inhale and
// flare in lockstep. The source center is stable in absolute space and already
// bounded by the renderer's absolute-coordinate precision guard.
float localVolumeSeed(vec3 center) {
    vec3 p = fract(center * vec3(0.1031, 0.1030, 0.0973));
    p += dot(p, p.yzx + 33.33);
    return fract((p.x + p.y) * p.z);
}

// Profile numbers are generated from core::FogProfile. Keep classification in
// one named predicate so local-medium behavior never grows another parallel
// table of midpoint literals.
bool isLocalProfile(float actual, float expected) {
    return abs(actual - expected) < 0.5;
}

// The one place a raw `profile_params.x` lane is bounded to the generated
// profile range. The upper bound is the HIGHEST generated profile
// (FOG_VOLUME_PROFILE_LIGHT_SHAFT); every classification site must read the
// lane through here. #4828: `evaluateCombustionSource` and
// `flameSourceSampleRatio` each kept a private clamp whose upper bound was
// still EXPLOSION_NUCLEAR after LIGHT_SHAFT (6.0) was added above it, so
// clamp(6.0, .., 5.0) = 5.0 turned every LightShaft / SkyAperture volume into
// a nuclear-explosion combustion source.
float volumeProfileKind(GpuFogVolume volume) {
    return clamp(
        volume.profile_params.x,
        FOG_VOLUME_PROFILE_HOMOGENEOUS,
        FOG_VOLUME_PROFILE_LIGHT_SHAFT
    );
}

bool isExplosionProfile(float profileKind) {
    return isLocalProfile(profileKind, FOG_VOLUME_PROFILE_EXPLOSION)
        || isLocalProfile(profileKind, FOG_VOLUME_PROFILE_EXPLOSION_OIL)
        || isLocalProfile(profileKind, FOG_VOLUME_PROFILE_EXPLOSION_NUCLEAR);
}

bool isNuclearExplosionProfile(float profileKind) {
    return isLocalProfile(profileKind, FOG_VOLUME_PROFILE_EXPLOSION_NUCLEAR);
}

bool isTransportedProfile(float profileKind) {
    return !isLocalProfile(profileKind, FOG_VOLUME_PROFILE_HOMOGENEOUS)
        && !isLocalProfile(profileKind, FOG_VOLUME_PROFILE_LIGHT_SHAFT);
}

// Local structure is source-relative and independent of weather coverage.
// Smoke advects slowly in broad coherent cells; flame uses a faster, finer
// field. Returning both bands lets each profile combine them without adding
// any texture samples over the old two-sample path.
vec2 sampleLocalTurbulence(
    vec3 normalizedOffset,
    float profileKind,
    float seed,
    float timeSeconds
) {
    bool isExplosion = isExplosionProfile(profileKind);
    bool isNuclear = isNuclearExplosionProfile(profileKind);
    bool isFlame = isLocalProfile(profileKind, FOG_VOLUME_PROFILE_FLAME);
    bool isSmoke = isLocalProfile(profileKind, FOG_VOLUME_PROFILE_SMOKE);
    float baseFrequency = isNuclear ? 0.34 : (isExplosion ? 0.58 : (isFlame ? 0.72 : (isSmoke ? 0.42 : 0.31)));
    float detailFrequency = isNuclear ? 0.82 : (isExplosion ? 1.35 : (isFlame ? 1.67 : (isSmoke ? 0.96 : 0.83)));
    float baseRise = isNuclear ? 0.045 : (isExplosion ? 0.11 : (isFlame ? 0.32 : (isSmoke ? 0.075 : 0.013)));
    float detailRise = isNuclear ? 0.10 : (isExplosion ? 0.25 : (isFlame ? 0.63 : (isSmoke ? 0.14 : 0.025)));
    vec3 phase = vec3(seed, fract(seed * 1.6180339), fract(seed * 2.4142136));
    vec3 slowCoords = normalizedOffset * baseFrequency + phase
        + vec3(0.013, -baseRise, 0.009) * timeSeconds;
    vec3 detailCoords = rotateByQuaternion(
        normalizedOffset * detailFrequency,
        vec4(0.0, 0.258819, 0.0, 0.965926)
    ) + phase.zxy * 1.73 + vec3(-0.021, -detailRise, 0.017) * timeSeconds;
    float base = texture(baseDensityNoise, slowCoords).r;
    float detail = texture(detailDensityNoise, detailCoords).r;
    return clamp(vec2(base, detail), vec2(0.0), vec2(1.0));
}

// Replace the symmetric ellipsoid only for particle-derived media. Authored
// fog meshes keep their exact primitive. Smoke broadens and wanders as it
// rises; flame remains planted at the fuel and collapses into a moving tip.
float localProfileRadius(
    vec3 normalizedOffset,
    float authoredRadius,
    float profileKind,
    float seed,
    float timeSeconds,
    float explosionAge,
    float explosionAgeSeconds
) {
    if (isLocalProfile(profileKind, FOG_VOLUME_PROFILE_HOMOGENEOUS)
        || isLocalProfile(profileKind, FOG_VOLUME_PROFILE_LIGHT_SHAFT)) {
        return authoredRadius;
    }

    if (isExplosionProfile(profileKind)) {
        bool isNuclear = isNuclearExplosionProfile(profileKind);
        // Expansion is a physical-time event, not a fraction of the authored
        // cooling lifetime. A long-lived 8 s effect must still become a full
        // fireball in roughly 0.35 s rather than remaining a bead at age 0.02.
        float expansionTime = isNuclear
            ? EXPLOSION_EXPANSION_TIME_SECONDS * 1.8
            : EXPLOSION_EXPANSION_TIME_SECONDS;
        float expansion = 1.0 - exp(
            -explosionAgeSeconds / max(expansionTime, 0.01)
        );
        float radius = mix(0.10, 0.96, expansion);
        float phase = seed * 2.0 * PI;
        vec2 wobble = mix(0.015, 0.12, explosionAge) * vec2(
            sin(phase + timeSeconds * 1.7),
            cos(phase * 1.37 + timeSeconds * 1.3)
        );
        float buoyantLift = smoothstep(0.35, 2.2, explosionAgeSeconds) * 0.18;
        vec3 displaced = normalizedOffset - vec3(wobble.x, buoyantLift, wobble.y);
        float fireball = length(displaced) / max(radius, 0.04);
        if (!isNuclear) {
            return fireball;
        }

        // Once the flash has lifted, replace the spherical boundary with a
        // cap-over-stem union. Both pieces live inside the authored source
        // bound; transport then carries that shaped mass outward and upward.
        float mushroomMature = smoothstep(0.30, 1.25, explosionAgeSeconds);
        float capCenter = mix(0.08, 0.42, mushroomMature);
        float capThickness = mix(0.30, 0.38, mushroomMature);
        float capRadius = mix(0.48, 0.88, mushroomMature);
        float capRadial = length(normalizedOffset.xz - wobble) / capRadius;
        float cap = max(
            abs(normalizedOffset.y - capCenter) / capThickness,
            capRadial
        );
        float stemBottom = mix(-0.40, -0.78, mushroomMature);
        float stemTop = mix(0.18, 0.42, mushroomMature);
        float stemCenter = (stemBottom + stemTop) * 0.5;
        float stemHalfHeight = (stemTop - stemBottom) * 0.5;
        float stemRadius = mix(0.27, 0.18, mushroomMature);
        float stem = max(
            abs(normalizedOffset.y - stemCenter) / max(stemHalfHeight, 0.05),
            length(normalizedOffset.xz - wobble * 0.45) / stemRadius
        );
        // `cap` and `stem` are distance-like masks: values below one are
        // inside. Taking their minimum therefore forms the volumetric union
        // (the mushroom silhouette), rather than a fragile intersection.
        return mix(fireball, min(cap, stem), mushroomMature);
    }

    float height = clamp(normalizedOffset.y * 0.5 + 0.5, 0.0, 1.0);
    float phase = seed * 2.0 * PI;
    vec2 centerline;
    float radius;
    if (isLocalProfile(profileKind, FOG_VOLUME_PROFILE_FLAME)) {
        float swayAmplitude = mix(0.015, 0.15, height);
        centerline = swayAmplitude * vec2(
            sin(phase + height * 5.2 + timeSeconds * 1.7),
            cos(phase * 1.37 + height * 4.1 + timeSeconds * 1.3)
        );
        radius = mix(0.92, 0.18, pow(height, 1.18));
    } else {
        float driftAmplitude = mix(0.01, 0.19, height);
        centerline = driftAmplitude * vec2(
            sin(phase + height * 3.1 + timeSeconds * 0.24),
            cos(phase * 1.21 + height * 2.7 + timeSeconds * 0.19)
        );
        radius = mix(0.46, 1.0, smoothstep(0.0, 1.0, height));
    }

    float radial = length(normalizedOffset.xz - centerline) / max(radius, 0.05);
    return max(abs(normalizedOffset.y), radial);
}

float localDensityProfile(
    vec3 normalizedOffset,
    float profileRadius,
    float edge,
    float profileKind,
    vec2 turbulence,
    float seed,
    float timeSeconds,
    float explosionAge,
    float explosionAgeSeconds
) {
    float boundary = 1.0 - smoothstep(1.0 - edge, 1.0, profileRadius);
    if (isLocalProfile(profileKind, FOG_VOLUME_PROFILE_HOMOGENEOUS)
        || isLocalProfile(profileKind, FOG_VOLUME_PROFILE_LIGHT_SHAFT)) {
        float genericNoise = turbulence.x * 0.72 + turbulence.y * 0.28;
        return boundary * mix(0.55, 1.15, genericNoise);
    }

    if (isExplosionProfile(profileKind)) {
        float core = 1.0 - smoothstep(0.10, 0.94, profileRadius);
        bool isNuclear = isNuclearExplosionProfile(profileKind);
        float shellCenter = mix(0.52, 0.76, smoothstep(0.0, 0.72, explosionAge));
        float shell = 1.0 - smoothstep(0.10, 0.27, abs(profileRadius - shellCenter));
        float smokeTransition = smoothstep(0.20, 0.72, explosionAge);
        float clumps = mix(
            0.34,
            1.48,
            smoothstep(0.20, 0.82, turbulence.x * 0.64 + turbulence.y * 0.36)
        );
        // Keep a visible, hot perimeter during the first part of the blast.
        // The shell becomes the cooled aerosol body later, but a very small
        // early coefficient made the explosion read as a bright pinprick
        // surrounded by opacity that did not emit.
        float evolvingBody = mix(core + shell * 0.40, shell + core * 0.16, smokeTransition);
        float nuclearCap = isNuclear
            // Use physical seconds here as well as in the geometry path. A
            // nuclear source can have an 8 s authored lifetime, but its cap
            // should begin forming around one second, not at 24% of that
            // lifetime (which would delay the mushroom until 1.9 s).
            ? smoothstep(0.35, 1.25, explosionAgeSeconds)
                * (1.0 - smoothstep(4.8, 7.2, explosionAgeSeconds))
            : 0.0;
        float capBody = max(evolvingBody, nuclearCap * 0.75);
        float dissipate = isNuclear
            ? 1.0 - smoothstep(0.94, 1.0, explosionAge)
            : 1.0 - smoothstep(0.80, 1.0, explosionAge);
        return boundary * capBody * clumps * dissipate;
    }

    float height = clamp(normalizedOffset.y * 0.5 + 0.5, 0.0, 1.0);
    float rootFade = smoothstep(-1.0, -0.78, normalizedOffset.y);
    if (isLocalProfile(profileKind, FOG_VOLUME_PROFILE_SMOKE)) {
        float billowNoise = turbulence.x * 0.76 + turbulence.y * 0.24;
        float occupied = smoothstep(0.24, 0.78, billowNoise);
        float rolls = 0.5 + 0.5 * sin(
            height * 13.0 + seed * 2.0 * PI + timeSeconds * 0.48
                + turbulence.y * 2.4
        );
        return boundary * rootFade
            * mix(0.32, 1.38, occupied)
            * mix(0.82, 1.16, rolls);
    }

    float lickNoise = turbulence.x * 0.43 + turbulence.y * 0.57;
    float split = 0.5 + 0.5 * sin(
        atan(normalizedOffset.z, normalizedOffset.x) * 3.0
            + normalizedOffset.y * 7.0
            - timeSeconds * 2.1
            + seed * 2.0 * PI
    );
    float threshold = mix(0.20, 0.58, height);
    float licks = smoothstep(threshold - 0.18, threshold + 0.24, lickNoise + split * 0.13);
    float hotCore = 1.0 - smoothstep(0.0, 0.42, profileRadius);
    return boundary * rootFade * (mix(0.24, 1.42, licks) + hotCore * 0.28);
}


// Resolve the one camera-centred cluster containing a world-space sample.
// Keeping this lookup and the primitive-shape evaluation shared prevents the
// analytic homogeneous path and the transported combustion source path from
// growing subtly different interpretations of the same canonical volume.
//
// #2242 (REN-D16-04) — this `fogVolumeCount == 0u` check is not a fragile
// lone guard against a stale cluster buffer surviving a cell transition.
// There is no persistent cluster state to go stale: `fogVolumeCount`
// (`VolumetricsPipeline::dispatch`, volumetrics.rs) is rewritten every
// single frame, either explicitly zeroed when this frame's fog-volume list
// is empty, or set fresh by `build_fog_volume_clusters` — which itself
// resets EVERY cluster entry's `count` to 0 before repopulating from this
// frame's volumes, every call, with no incremental/partial update (#4792:
// the touched clusters' `offset`s are rewritten too, since the index list
// is packed densely per frame). The `fog_cluster_buffers`/
// `fog_cluster_index_buffers` GPU writes are skipped as an optimisation
// exactly when `fogVolumeCount` is also unconditionally zeroed, so this
// early-out always gates the stale/skipped-write case correctly. A cell
// transition simply means next frame's `collect_fog_volumes`
// (byroredux/src/render/fog_volumes.rs) returns a different `Vec` from live
// ECS state — the whole pipeline is a full per-frame rebuild, not an
// incremental one.
bool localFogCluster(vec3 worldPos, out FogClusterEntry cluster) {
    if (fogVolumeCount == 0u || params.local_volume_grid.w <= 0.0) {
        return false;
    }

    vec3 gridCoord = (worldPos - params.local_volume_grid.xyz)
        * params.local_volume_grid.w;
    if (any(lessThan(gridCoord, vec3(0.0)))
            || any(greaterThanEqual(gridCoord, vec3(float(FOG_VOLUME_CLUSTER_DIM))))) {
        return false;
    }
    uvec3 cell = min(uvec3(gridCoord), uvec3(FOG_VOLUME_CLUSTER_DIM - 1u));
    uint clusterIndex = cell.x
        + cell.y * FOG_VOLUME_CLUSTER_DIM
        + cell.z * FOG_VOLUME_CLUSTER_DIM * FOG_VOLUME_CLUSTER_DIM;
    cluster = fogClusters[clusterIndex];
    return true;
}

// #4784 — the fog-cluster cell a world position falls in. Same mapping as
// `localFogCluster`, but always clamped into the grid: the occupancy test is
// a conservative membership question, and froxels outside the cluster grid
// (beyond fog far) edge-clamp to the border cells whose occupancy the host
// and GPU marks already govern.
uint combustionClusterIndex(vec3 worldPos) {
    vec3 gridCoord = clamp(
        (worldPos - params.local_volume_grid.xyz) * max(params.local_volume_grid.w, 0.0),
        vec3(0.0),
        vec3(float(FOG_VOLUME_CLUSTER_DIM) - 1.0)
    );
    uvec3 cell = min(uvec3(gridCoord), uvec3(FOG_VOLUME_CLUSTER_DIM - 1u));
    return cell.x
        + cell.y * FOG_VOLUME_CLUSTER_DIM
        + cell.z * FOG_VOLUME_CLUSTER_DIM * FOG_VOLUME_CLUSTER_DIM;
}

// #4784 — dilated occupancy test: true when this cell OR any of its 26
// neighbours held combustion last frame. The one-frame mask lag plus this
// one-cell dilation bounds the plume's reach conservatively (max transport
// step is far below one cluster cell), so skipping the transport block on a
// false here cannot drop mass that would have arrived.
bool combustionNeighborhoodOccupied(uint clusterIndex) {
    const uint plane = FOG_VOLUME_CLUSTER_DIM * FOG_VOLUME_CLUSTER_DIM;
    uvec3 cell = uvec3(
        clusterIndex % FOG_VOLUME_CLUSTER_DIM,
        (clusterIndex / FOG_VOLUME_CLUSTER_DIM) % FOG_VOLUME_CLUSTER_DIM,
        clusterIndex / plane
    );
    for (int z = -1; z <= 1; ++z) {
        for (int y = -1; y <= 1; ++y) {
            for (int x = -1; x <= 1; ++x) {
                ivec3 neighbour = ivec3(cell) + ivec3(x, y, z);
                if (any(lessThan(neighbour, ivec3(0)))
                    || any(greaterThanEqual(
                        neighbour,
                        ivec3(FOG_VOLUME_CLUSTER_DIM)))) {
                    continue;
                }
                uint index = uint(neighbour.x)
                    + uint(neighbour.y) * FOG_VOLUME_CLUSTER_DIM
                    + uint(neighbour.z) * plane;
                if (combustionOccupancyIn[index] != 0u) {
                    return true;
                }
            }
        }
    }
    return false;
}

// The portal cluster indexes authored cone source rings and window planes
// swept down the current sun direction. It is independent of static density
// bounds: a real shaft can move across the room as the sun moves. A cone only
// marks a candidate for a fully clear outdoor ray. An authored window plane
// also marks the boundary of the interior: an opaque shell behind that plane
// need not hide its explicitly authored opening.
bool localSkyAperture(vec3 worldPos, vec3 sunDirection, out float nearestWindowReach) {
    nearestWindowReach = DIRECTIONAL_SHADOW_TRACE_DISTANCE + 1.0;
    FogClusterEntry cluster;
    if (!localFogCluster(worldPos, cluster)) return false;
    uint candidateCount = min(cluster.portalCount, MAX_FOG_PORTALS_PER_CLUSTER);
    bool coneCandidate = false;
    for (uint i = 0u; i < candidateCount; ++i) {
        uint volumeIndex = fogClusterIndices[cluster.portalOffset + i];
        if (volumeIndex >= fogVolumeCount) continue;
        GpuFogVolume volume = fogVolumes[volumeIndex];
        if ((volume.center_shape.w != float(FOG_VOLUME_SHAPE_BOX)
                && volume.center_shape.w != float(FOG_VOLUME_SHAPE_CONE))
            || !isLocalProfile(volume.profile_params.x, FOG_VOLUME_PROFILE_LIGHT_SHAFT)) {
            continue;
        }
        vec3 localOrigin = rotateByQuaternion(
            worldPos - volume.center_shape.xyz,
            volume.inverse_rotation
        );
        vec3 localSun = rotateByQuaternion(sunDirection, volume.inverse_rotation);
        if (volume.center_shape.w == float(FOG_VOLUME_SHAPE_CONE)) {
            if (localSun.y <= 1.0e-4) continue;
            float reach = (volume.half_extents_extinction.y - localOrigin.y) / localSun.y;
            vec2 apertureHit = localOrigin.xz + reach * localSun.xz;
            float sourceRadius = volume.half_extents_extinction.z;
            if (reach >= 0.05 && reach <= DIRECTIONAL_SHADOW_TRACE_DISTANCE
                && dot(apertureHit, apertureHit) <= sourceRadius * sourceRadius) {
                coneCandidate = true;
            }
        } else {
            if (volume.profile_params.w < 0.5) continue;
            if (abs(localSun.z) <= 1.0e-4) continue;
            float reach = -localOrigin.z / localSun.z;
            vec2 apertureHit = localOrigin.xy + reach * localSun.xy;
            if (reach >= 0.05 && reach <= DIRECTIONAL_SHADOW_TRACE_DISTANCE
                && all(lessThanEqual(abs(apertureHit),
                    volume.half_extents_extinction.xy))) {
                nearestWindowReach = min(nearestWindowReach, reach);
            }
        }
    }
    return coneCandidate || nearestWindowReach <= DIRECTIONAL_SHADOW_TRACE_DISTANCE;
}

bool evaluateLocalFogVolume(
    GpuFogVolume volume,
    vec3 worldPos,
    out vec3 normalizedOffset,
    out vec3 localOffset,
    out float profileKind,
    out float densityProfile,
    out vec2 localTurbulence
) {
    vec3 extents = max(volume.half_extents_extinction.xyz, vec3(1.0e-4));
    localOffset = rotateByQuaternion(
        worldPos - volume.center_shape.xyz,
        volume.inverse_rotation
    );
    normalizedOffset = localOffset / extents;
    // #4955 — exact generated ids, not `x.5` thresholds that silently
    // re-route if `FogShape` is reordered.
    float shape = volume.center_shape.w;
    float normalizedRadius;
    if (shape == float(FOG_VOLUME_SHAPE_CONE)) {
        // The cone narrows from its bottom radius at local y=-halfHeight to
        // its top radius at +halfHeight. Store both radii in the existing
        // extents lanes, avoiding a second GPU primitive ABI.
        float axial = normalizedOffset.y;
        float t = clamp(axial * 0.5 + 0.5, 0.0, 1.0);
        float radius = mix(extents.x, extents.z, t);
        normalizedOffset.z = localOffset.z / extents.x;
        normalizedRadius = max(abs(axial), length(localOffset.xz) / max(radius, 1.0e-4));
    } else if (shape == float(FOG_VOLUME_SHAPE_BOX)) {
        normalizedRadius = max(
            abs(normalizedOffset.x),
            max(abs(normalizedOffset.y), abs(normalizedOffset.z))
        );
    } else if (shape == float(FOG_VOLUME_SHAPE_SPHERE)) {
        normalizedRadius = length(localOffset) / extents.x;
    } else {
        normalizedRadius = length(normalizedOffset);
    }
    profileKind = volumeProfileKind(volume);
    float timeSeconds = params.temporal_params.z;
    float seed = localVolumeSeed(volume.center_shape.xyz);
    float explosionAge = clamp(volume.profile_params.y, 0.0, 1.0);
    float explosionAgeSeconds = explosionAge * max(volume.profile_params.z, 0.0);
    float profileRadius = localProfileRadius(
        normalizedOffset,
        normalizedRadius,
        profileKind,
        seed,
        timeSeconds,
        explosionAge,
        explosionAgeSeconds
    );
    if (profileRadius >= 1.0) {
        return false;
    }

    float edge = clamp(volume.albedo_edge.w, 1.0e-3, 1.0);
    localTurbulence = sampleLocalTurbulence(
        normalizedOffset,
        profileKind,
        seed,
        timeSeconds
    );
    densityProfile = localDensityProfile(
        normalizedOffset,
        profileRadius,
        edge,
        profileKind,
        localTurbulence,
        seed,
        timeSeconds,
        explosionAge,
        explosionAgeSeconds
    );
    return densityProfile > 1.0e-6;
}

// Returns only directly-authored homogeneous media. Smoke, flame, and
// explosions enter the transported thermochemical field below; rendering
// their old analytic body as well would double-count extinction and pin the
// plume to its emitter instead of allowing advection to move it.
LocalMedium sampleLocalMedium(vec3 worldPos) {
    LocalMedium medium = emptyLocalMedium();
    FogClusterEntry cluster;
    if (!localFogCluster(worldPos, cluster)) {
        return medium;
    }
    uint primitiveCount = min(cluster.count, MAX_FOG_VOLUMES_PER_CLUSTER);

    for (uint i = 0u; i < primitiveCount; ++i) {
        uint volumeIndex = fogClusterIndices[cluster.offset + i];
        if (volumeIndex >= fogVolumeCount) {
            continue;
        }
        GpuFogVolume volume = fogVolumes[volumeIndex];
        vec3 normalizedOffset;
        vec3 localOffset;
        float profileKind;
        float densityProfile;
        vec2 localTurbulence;
        // Transported profiles are sourced from the combustion history
        // fields. Reject them before radius, turbulence and density sampling.
        float authoredProfile = volumeProfileKind(volume);
        if (isnan(authoredProfile)
                || isinf(authoredProfile)
                || isTransportedProfile(authoredProfile)) {
            continue;
        }
        if (!evaluateLocalFogVolume(
                volume,
                worldPos,
                normalizedOffset,
                localOffset,
                profileKind,
                densityProfile,
                localTurbulence)) {
            continue;
        }
        float sigmaT = max(volume.half_extents_extinction.w, 0.0) * densityProfile;
        vec3 albedo = clamp(volume.albedo_edge.rgb, vec3(0.0), vec3(1.0));
        vec3 emissiveRadiance = max(volume.emission_temperature.rgb, vec3(0.0));
        medium.extinction += sigmaT;
        medium.scattering += sigmaT * albedo;

        // Emission source term of the radiative transfer equation:
        //   dL/ds |_emission = sigma_a * L_e,   sigma_a = sigma_t * (1 - albedo)
        //
        // Per channel, because soot's absorption is spectrally selective and
        // because `albedo` is already an RGB triple here. Multiplying by the
        // locally evaluated sigma_a (not by a constant) is what gives a flame
        // visible internal structure: the same procedural density field that
        // shapes its silhouette also shapes where it glows brightest.
        //
        // This is why fire needs no separate render path. `integrate` already
        // treats this buffer's rgb as "radiance added per unit length" and
        // multiplies by slab thickness and accumulated transmittance, which
        // is exactly the emission integral. Fire composites, denoises, and
        // reprojects through the machinery fog already uses.
        medium.emission += sigmaT * (vec3(1.0) - albedo) * emissiveRadiance;
    }
    return medium;
}

// The transported field is intentionally deterministic at each froxel centre,
// but a persistent flame's source boundary is often only a few froxels wide.
// Re-evaluate that canonical source profile at the jittered lighting sample and
// use the centre/sample density ratio as an unbiased sub-froxel coverage hint.
// This shapes only the planted flame boundary; advected chemistry outside it
// remains entirely owned by transport and cannot snap back to the emitter.
float flameSourceSampleRatio(vec3 fieldWorldPos, vec3 sampleWorldPos) {
    FogClusterEntry cluster;
    if (!localFogCluster(fieldWorldPos, cluster)) {
        return 1.0;
    }

    float centerDensitySum = 0.0;
    float sampleDensitySum = 0.0;
    uint primitiveCount = min(cluster.count, MAX_FOG_VOLUMES_PER_CLUSTER);
    for (uint i = 0u; i < primitiveCount; ++i) {
        uint volumeIndex = fogClusterIndices[cluster.offset + i];
        if (volumeIndex >= fogVolumeCount) {
            continue;
        }
        GpuFogVolume volume = fogVolumes[volumeIndex];
        float profileKind = volumeProfileKind(volume);
        if (!isLocalProfile(profileKind, FOG_VOLUME_PROFILE_FLAME)) {
            continue;
        }

        vec3 normalizedOffset;
        vec3 localOffset;
        float evaluatedProfile;
        float centerDensity;
        vec2 localTurbulence;
        if (!evaluateLocalFogVolume(
                volume,
                fieldWorldPos,
                normalizedOffset,
                localOffset,
                evaluatedProfile,
                centerDensity,
                localTurbulence)) {
            continue;
        }
        centerDensitySum += centerDensity;

        float sampleDensity;
        if (evaluateLocalFogVolume(
                volume,
                sampleWorldPos,
                normalizedOffset,
                localOffset,
                evaluatedProfile,
                sampleDensity,
                localTurbulence)) {
            sampleDensitySum += sampleDensity;
        }
    }
    if (centerDensitySum <= 1.0e-6) {
        return 1.0;
    }
    return clamp(sampleDensitySum / centerDensitySum, 0.0, 1.5);
}

// Return the [0,1] interval where a normalized line segment lies inside a
// unit sphere. Sphere and ellipsoid FogBounds both become a unit sphere after
// division by their authored half extents.
bool intersectUnitSphereSegment(vec3 start, vec3 end, out vec2 interval) {
    vec3 direction = end - start;
    float a = dot(direction, direction);
    if (a <= 1.0e-10) {
        interval = vec2(0.0, 1.0);
        return dot(start, start) <= 1.0;
    }
    float b = dot(start, direction);
    float c = dot(start, start) - 1.0;
    float discriminant = b * b - a * c;
    if (discriminant < 0.0) {
        return false;
    }
    float root = sqrt(max(discriminant, 0.0));
    interval = vec2((-b - root) / a, (-b + root) / a);
    interval = clamp(interval, vec2(0.0), vec2(1.0));
    return interval.y > interval.x;
}

// Slab intersection against a normalized [-1,1]^3 box.
bool intersectUnitBoxSegment(vec3 start, vec3 end, out vec2 interval) {
    vec3 direction = end - start;
    float entry = 0.0;
    float exit = 1.0;
    for (int axis = 0; axis < 3; ++axis) {
        if (abs(direction[axis]) <= 1.0e-8) {
            if (abs(start[axis]) > 1.0) {
                return false;
            }
            continue;
        }
        vec2 crossings = vec2(
            (-1.0 - start[axis]) / direction[axis],
            (1.0 - start[axis]) / direction[axis]
        );
        float axisEntry = min(crossings.x, crossings.y);
        float axisExit = max(crossings.x, crossings.y);
        entry = max(entry, axisEntry);
        exit = min(exit, axisExit);
        if (exit <= entry) {
            return false;
        }
    }
    interval = clamp(vec2(entry, exit), vec2(0.0), vec2(1.0));
    return interval.y > interval.x;
}

// Conservative gather for an emitter that may be thinner than one froxel
// depth slab. Point-sampling only the slab centre deterministically loses a
// Bethesda-scale torch/explosion whenever it falls between two centres. We
// intersect the entire camera-ray segment with the canonical primitive,
// evaluate three deterministic interior points, and multiply by the occupied
// segment fraction. The resulting coefficient preserves optical depth instead
// of inflating a sub-slab source to fill the whole froxel.
bool evaluateCombustionSource(
    GpuFogVolume volume,
    vec3 segmentStart,
    vec3 segmentEnd,
    out vec3 sourceWorldPos,
    out vec3 normalizedOffset,
    out float profileKind,
    out float densityProfile,
    out float stateProfile,
    out vec2 turbulence
) {
    profileKind = volumeProfileKind(volume);
    if (!isTransportedProfile(profileKind)) {
        return false;
    }

    vec3 extents = max(volume.half_extents_extinction.xyz, vec3(1.0e-4));
    vec3 localStart = rotateByQuaternion(
        segmentStart - volume.center_shape.xyz,
        volume.inverse_rotation
    ) / extents;
    vec3 localEnd = rotateByQuaternion(
        segmentEnd - volume.center_shape.xyz,
        volume.inverse_rotation
    ) / extents;
    vec2 interval;
    // Box and cone both bound with the unit box (the cone's radii fit it).
    bool intersects = volume.center_shape.w == float(FOG_VOLUME_SHAPE_BOX)
            || volume.center_shape.w == float(FOG_VOLUME_SHAPE_CONE)
        ? intersectUnitBoxSegment(localStart, localEnd, interval)
        : intersectUnitSphereSegment(localStart, localEnd, interval);
    if (!intersects) {
        return false;
    }

    float segmentCoverage = interval.y - interval.x;
    densityProfile = 0.0;
    stateProfile = 0.0;
    sourceWorldPos = mix(segmentStart, segmentEnd, 0.5 * (interval.x + interval.y));
    normalizedOffset = vec3(0.0);
    turbulence = vec2(0.5);
    for (int sampleIndex = 0; sampleIndex < 3; ++sampleIndex) {
        float samplePhase = (float(sampleIndex) + 0.5) / 3.0;
        vec3 candidateWorldPos = mix(
            segmentStart,
            segmentEnd,
            mix(interval.x, interval.y, samplePhase)
        );
        vec3 candidateNormalizedOffset;
        vec3 candidateLocalOffset;
        float candidateProfile;
        float candidateDensity;
        vec2 candidateTurbulence;
        if (evaluateLocalFogVolume(
                volume,
                candidateWorldPos,
                candidateNormalizedOffset,
                candidateLocalOffset,
                candidateProfile,
                candidateDensity,
                candidateTurbulence)
                && candidateDensity > densityProfile) {
            densityProfile = candidateDensity;
            stateProfile = candidateDensity;
            sourceWorldPos = candidateWorldPos;
            normalizedOffset = candidateNormalizedOffset;
            turbulence = candidateTurbulence;
        }
    }
    // Extensive quantities (fuel mass and optical extinction) occupy only
    // part of this depth slab, so their volume average carries coverage.
    // Intensive source state (temperature, radiance calibration, velocity)
    // must retain the strongest occupied sample. Scaling both would apply
    // coverage twice in `combustionMedium` (`sigma_t * L_e`) and can push a
    // perfectly hot sub-froxel flame below ignition merely because the
    // transport grid is coarse.
    densityProfile *= segmentCoverage;
    return densityProfile > 1.0e-7;
}

void applyCombustionSources(
    vec3 worldPos,
    vec3 segmentStart,
    vec3 segmentEnd,
    inout vec4 chemistry,
    inout vec4 dynamics,
    inout vec4 optical,
    float dt
) {
    FogClusterEntry cluster;
    if (!localFogCluster(worldPos, cluster)) {
        return;
    }
    uint primitiveCount = min(cluster.count, MAX_FOG_VOLUMES_PER_CLUSTER);
    for (uint i = 0u; i < primitiveCount; ++i) {
        uint volumeIndex = fogClusterIndices[cluster.offset + i];
        if (volumeIndex >= fogVolumeCount) {
            continue;
        }
        GpuFogVolume volume = fogVolumes[volumeIndex];
        vec3 sourceWorldPos;
        vec3 normalizedOffset;
        float profileKind;
        float densityProfile;
        float stateProfile;
        vec2 turbulence;
        if (!evaluateCombustionSource(
                volume,
                segmentStart,
                segmentEnd,
                sourceWorldPos,
                normalizedOffset,
                profileKind,
                densityProfile,
                stateProfile,
                turbulence)) {
            continue;
        }

        float radialSource = 1.0 - smoothstep(0.28, 0.92, length(normalizedOffset.xz));
        float normalizedHeight = normalizedOffset.y * 0.5 + 0.5;
        bool nuclearExplosion = isNuclearExplosionProfile(profileKind);
        float basalSource = isLocalProfile(profileKind, FOG_VOLUME_PROFILE_FLAME)
            ? 1.0 - smoothstep(
                0.0,
                FLAME_FUEL_BOUNDARY_HEIGHT_FRACTION,
                normalizedHeight
            )
            : (nuclearExplosion
                ? 1.0
                : 1.0 - smoothstep(-0.72, 0.10, normalizedOffset.y));
        float sourceMask = densityProfile * radialSource * basalSource;
        float sourceStateMask = stateProfile * radialSource * basalSource;
        float authoredSigmaT = max(volume.half_extents_extinction.w, 0.0);
        vec3 sourceAlbedo = clamp(volume.albedo_edge.rgb, vec3(0.0), vec3(1.0));

        if (isLocalProfile(profileKind, FOG_VOLUME_PROFILE_SMOKE)) {
            // Passive smoke supplies aerosol and a mild warm/upward exhaust,
            // but no fuel. `max` models a continuously emitting boundary
            // without making density depend on frame rate.
            float sourceSigmaT = authoredSigmaT * sourceMask;
            chemistry.z = max(chemistry.z, sourceSigmaT);
            optical.rgb = max(optical.rgb, sourceSigmaT * sourceAlbedo);
            chemistry.y = max(
                chemistry.y,
                mix(AMBIENT_TEMPERATURE_K, 430.0, sourceStateMask)
            );
            dynamics.y = max(
                dynamics.y,
                sourceStateMask * 0.42 * WORLD_UNITS_PER_METER
            );
            continue;
        }

        if (isLocalProfile(profileKind, FOG_VOLUME_PROFILE_FLAME)) {
            // A persistent flame has a compact replenished fuel inlet and a
            // larger luminous reaction zone. The transported solver, not the
            // authored bound, determines the tip, downstream plume, and soot
            // left after combustion.
            float reactionZone = 1.0 - smoothstep(
                FLAME_REACTION_ZONE_FADE_START_FRACTION,
                FLAME_REACTION_ZONE_HEIGHT_FRACTION,
                normalizedHeight
            );
            float reactionMask = densityProfile * radialSource * reactionZone;
            float reactionStateMask = stateProfile * radialSource * reactionZone;
            float sourceTemperature = clamp(
                max(volume.emission_temperature.w, COMBUSTION_FLAME_SOURCE_FLOOR_K),
                COMBUSTION_IGNITION_TEMPERATURE_K,
                COMBUSTION_MAX_TEMPERATURE_K
            );
            chemistry.x = max(chemistry.x, sourceMask * 0.82);
            chemistry.y = max(
                chemistry.y,
                mix(AMBIENT_TEMPERATURE_K, sourceTemperature, reactionStateMask)
            );
            float sourceSigmaT = authoredSigmaT * reactionMask * 0.16;
            chemistry.z = max(chemistry.z, sourceSigmaT);
            optical.rgb = max(optical.rgb, sourceSigmaT * sourceAlbedo);
            float sourceRadianceLuma = max(
                dot(volume.emission_temperature.rgb, LUMA_REC709),
                0.0
            );
            float sourceVisibleRatio = blackbodyVisibleRadianceRatio(
                sourceTemperature,
                1850.0
            );
            chemistry.w = max(
                chemistry.w,
                reactionStateMask * sourceRadianceLuma / max(sourceVisibleRatio, 1.0e-4)
            );
            dynamics.y = max(
                dynamics.y,
                reactionStateMask * mix(1.12, 1.48, turbulence.x)
                    * WORLD_UNITS_PER_METER
            );
            vec2 lateralTarget = (turbulence * 2.0 - 1.0)
                * FLAME_SOURCE_LATERAL_SPEED_MPS
                * WORLD_UNITS_PER_METER;
            float velocityResponse = 1.0 - exp(
                -FLAME_SOURCE_VELOCITY_RESPONSE_PER_SECOND
                    * reactionStateMask
                    * dt
            );
            dynamics.xz = mix(dynamics.xz, lateralTarget, velocityResponse);
            continue;
        }

        // Explosion: a finite radial momentum/heat/fuel impulse. The authored
        // primitive is the *swept bound* of the legacy particle effect, not a
        // continuously burning spherical volume. Re-injecting its whole
        // age-expanded density body every frame pins a bright ball to the
        // emitter and overwhelms advection. Warm history therefore receives
        // only a compact, early core impulse; after that the transported field
        // owns expansion, cooling, soot, and buoyancy.
        //
        // A cold history (camera cut/resize) is different: seed the canonical
        // density at its current age once so an in-progress blast does not
        // disappear merely because storage was reset.
        float age = clamp(volume.profile_params.y, 0.0, 1.0);
        float lifetime = max(volume.profile_params.z, 0.12);
        float ageSeconds = age * lifetime;
        float ignitionEnvelope = nuclearExplosion
            ? 1.0 - smoothstep(0.20, 1.0, ageSeconds)
            : 1.0 - smoothstep(0.12, 0.75, ageSeconds);
        float smokeEnvelope = nuclearExplosion
            ? smoothstep(0.35, 1.8, ageSeconds)
                * (1.0 - smoothstep(0.96, 1.0, age))
            : smoothstep(0.35, 1.25, ageSeconds)
                * (1.0 - smoothstep(0.88, 1.0, age));
        bool coldHistory = params.prev_camera_pos.w <= 0.5;
        float impulseEnvelope = 1.0 - smoothstep(
            EXPLOSION_IMPULSE_DURATION_SECONDS * 0.35,
            EXPLOSION_IMPULSE_DURATION_SECONDS,
            ageSeconds
        );
        float coreRadius = length(normalizedOffset);
        float compactCore = 1.0 - smoothstep(0.12, 0.36, coreRadius);
        // A one-froxel fireball needs a warm shell as well as its compact
        // centre. Without this, the authored explosion's visible shell is
        // seeded only as cooled smoke and the normal composite reduces it to
        // a tiny ember even though the volume debug view shows its opacity.
        float shellCenter = mix(0.42, 0.62, age);
        float hotShell = 1.0 - smoothstep(
            0.16,
            0.44,
            abs(coreRadius - shellCenter)
        );
        float ignitionCoverage = max(compactCore, hotShell * 0.55);
        float ignitionMask = densityProfile * (
            coldHistory
                ? ignitionEnvelope
                : ignitionCoverage * impulseEnvelope
        );
        float ignitionStateMask = stateProfile * (
            coldHistory
                ? ignitionEnvelope
                : ignitionCoverage * impulseEnvelope
        );
        // Seed the finite event's cooling shell, not only its compact hot
        // core. This is still a one-shot source term (the envelope closes
        // before the emitter expires), but it gives the transported aerosol
        // enough mass to survive the fireball and rise as visible smoke.
        float smokeSeedMask = coldHistory
            ? densityProfile * smokeEnvelope
            : densityProfile * smokeEnvelope * 0.38;
        float sourceTemperature = clamp(
            max(volume.emission_temperature.w, nuclearExplosion
                ? COMBUSTION_NUCLEAR_SOURCE_FLOOR_K
                : COMBUSTION_EXPLOSION_SOURCE_FLOOR_K),
            COMBUSTION_IGNITION_TEMPERATURE_K,
            COMBUSTION_MAX_TEMPERATURE_K
        );
        chemistry.x = max(
            chemistry.x,
            ignitionMask * (nuclearExplosion ? 1.0 : 0.72)
        );
        chemistry.y = max(
            chemistry.y,
            mix(AMBIENT_TEMPERATURE_K, sourceTemperature, ignitionStateMask)
        );
        chemistry.y = max(
            chemistry.y,
            mix(
                AMBIENT_TEMPERATURE_K,
                COMBUSTION_COOLED_AEROSOL_SOURCE_TEMPERATURE_K,
                smokeSeedMask
            )
        );
        dynamics.y = max(
            dynamics.y,
            smokeSeedMask
                * COMBUSTION_TURBULENCE_COARSE_RISE_SPEED_MPS
                * (nuclearExplosion ? NUCLEAR_EXPLOSION_BUOYANCY_SCALE : 1.0)
                * WORLD_UNITS_PER_METER
        );
        float sourceSigmaT = authoredSigmaT
            * (ignitionMask * 0.10
                + smokeSeedMask * COMBUSTION_EXPLOSION_SMOKE_EXTINCTION_SCALE
                    * (nuclearExplosion ? NUCLEAR_EXPLOSION_SMOKE_MASS_SCALE : 1.0));
        chemistry.z = max(chemistry.z, sourceSigmaT);
        optical.rgb = max(optical.rgb, sourceSigmaT * sourceAlbedo);
        float sourceRadianceLuma = max(
            dot(volume.emission_temperature.rgb, LUMA_REC709),
            0.0
        );
        float sourceVisibleRatio = blackbodyVisibleRadianceRatio(
            sourceTemperature,
            1850.0
        );
        chemistry.w = max(
            chemistry.w,
            ignitionStateMask * sourceRadianceLuma / max(sourceVisibleRatio, 1.0e-4)
        );
        vec3 radialDirection = sourceWorldPos - volume.center_shape.xyz;
        float radialLength = length(radialDirection);
        radialDirection = radialLength > 1.0e-4
            ? radialDirection / radialLength
            : vec3(0.0, 1.0, 0.0);
        float radius = max(
            volume.half_extents_extinction.x,
            max(volume.half_extents_extinction.y, volume.half_extents_extinction.z)
        );
        float expansionSpeed = min(
            radius / EXPLOSION_EXPANSION_TIME_SECONDS
                * (nuclearExplosion ? NUCLEAR_EXPLOSION_EXPANSION_SCALE : 1.0),
            COMBUSTION_MAX_SPEED_MPS * WORLD_UNITS_PER_METER
        );
        float impulseScale = !coldHistory
            ? dt * 7.0
            : 0.45;
        float radialImpulseMask = coldHistory
            ? stateProfile * ignitionEnvelope
            : stateProfile * (nuclearExplosion ? ignitionCoverage : compactCore)
                * impulseEnvelope;
        dynamics.xyz += radialDirection * expansionSpeed
            * radialImpulseMask * impulseScale;
        float expansionSpeedMps = expansionSpeed / WORLD_UNITS_PER_METER;
        float specificOverpressure = 0.5 * expansionSpeedMps * expansionSpeedMps;
        dynamics.w = max(
            dynamics.w,
            specificOverpressure * radialImpulseMask
        );
    }
}

// Curl of a periodic vector potential. The field is divergence-free by
// construction, so domain warping swirls without the "shower door" look of a
// uniform texture scroll. Inputs and amplitudes are physical metres.
vec3 curlField(vec3 p) {
    return vec3(
        -sin(p.y) - cos(p.z),
        -sin(p.z) - cos(p.x),
        -sin(p.x) - cos(p.y)
    ) * 0.5;
}

vec3 rotateY(vec3 p, float angle) {
    float c = cos(angle);
    float s = sin(angle);
    return vec3(c * p.x - s * p.z, p.y, s * p.x + c * p.z);
}

#endif
