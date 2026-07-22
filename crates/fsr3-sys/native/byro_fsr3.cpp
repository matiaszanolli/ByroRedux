#include "byro_fsr3.h"

#include <FidelityFX/host/ffx_fsr3upscaler.h>
#include <ffx_api/ffx_api.h>
#include <ffx_api/ffx_upscale.h>
#include <ffx_api/vk/ffx_api_vk.h>

#include <new>
#include <cstdlib>

#ifdef _WIN32
#include <malloc.h>
#endif

struct ByroFsr3Context {
    ffxContext context = nullptr;
};

namespace {

constexpr uint32_t kHdrFlag = FFX_UPSCALE_ENABLE_HIGH_DYNAMIC_RANGE;
constexpr uint32_t kDebugFlag = FFX_UPSCALE_ENABLE_DEBUG_CHECKING;

void* aligned_allocate(void*, uint64_t size) {
#ifdef _WIN32
    return _aligned_malloc(static_cast<size_t>(size), 64);
#else
    void* memory = nullptr;
    return posix_memalign(&memory, 64, static_cast<size_t>(size)) == 0 ? memory : nullptr;
#endif
}

void aligned_deallocate(void*, void* memory) {
#ifdef _WIN32
    _aligned_free(memory);
#else
    std::free(memory);
#endif
}

ffxAllocationCallbacks allocation_callbacks() {
    return {nullptr, aligned_allocate, aligned_deallocate};
}

uint32_t query_single_version(ByroFsr3Version* out_version) {
    if (!out_version) {
        return FFX_API_RETURN_ERROR_PARAMETER;
    }

    uint64_t count = 1;
    uint64_t provider_id = 0;
    const char* provider_name = nullptr;
    ffxQueryDescGetVersions query{};
    query.header.type = FFX_API_QUERY_DESC_TYPE_GET_VERSIONS;
    query.createDescType = FFX_API_CREATE_CONTEXT_DESC_TYPE_UPSCALE;
    query.outputCount = &count;
    query.versionIds = &provider_id;
    query.versionNames = &provider_name;
    const uint32_t result = ffxQuery(nullptr, &query.header);
    if (result != FFX_API_RETURN_OK) {
        return result;
    }
    if (count != 1 || !provider_name) {
        return FFX_API_RETURN_NO_PROVIDER;
    }

    out_version->major = FFX_FSR3UPSCALER_VERSION_MAJOR;
    out_version->minor = FFX_FSR3UPSCALER_VERSION_MINOR;
    out_version->patch = FFX_FSR3UPSCALER_VERSION_PATCH;
    out_version->provider_id = provider_id;
    return FFX_API_RETURN_OK;
}

} // namespace

extern "C" uint32_t byro_fsr3_query_version(ByroFsr3Version* out_version) {
    return query_single_version(out_version);
}

extern "C" uint32_t byro_fsr3_query_render_resolution(
    uint32_t display_width,
    uint32_t display_height,
    uint32_t quality_mode,
    uint32_t* out_render_width,
    uint32_t* out_render_height) {
    if (!display_width || !display_height || quality_mode > FFX_UPSCALE_QUALITY_MODE_ULTRA_PERFORMANCE ||
        !out_render_width || !out_render_height) {
        return FFX_API_RETURN_ERROR_PARAMETER;
    }
    ffxQueryDescUpscaleGetRenderResolutionFromQualityMode query{};
    query.header.type = FFX_API_QUERY_DESC_TYPE_UPSCALE_GETRENDERRESOLUTIONFROMQUALITYMODE;
    query.displayWidth = display_width;
    query.displayHeight = display_height;
    query.qualityMode = quality_mode;
    query.pOutRenderWidth = out_render_width;
    query.pOutRenderHeight = out_render_height;
    return ffxQuery(nullptr, &query.header);
}

extern "C" uint32_t byro_fsr3_query_jitter_phase_count(
    uint32_t render_width,
    uint32_t display_width,
    int32_t* out_phase_count) {
    if (!render_width || !display_width || !out_phase_count) {
        return FFX_API_RETURN_ERROR_PARAMETER;
    }
    ffxQueryDescUpscaleGetJitterPhaseCount query{};
    query.header.type = FFX_API_QUERY_DESC_TYPE_UPSCALE_GETJITTERPHASECOUNT;
    query.renderWidth = render_width;
    query.displayWidth = display_width;
    query.pOutPhaseCount = out_phase_count;
    return ffxQuery(nullptr, &query.header);
}

extern "C" uint32_t byro_fsr3_query_jitter_offset(
    uint32_t index,
    int32_t phase_count,
    float* out_x,
    float* out_y) {
    if (phase_count <= 0 || !out_x || !out_y) {
        return FFX_API_RETURN_ERROR_PARAMETER;
    }
    ffxQueryDescUpscaleGetJitterOffset query{};
    query.header.type = FFX_API_QUERY_DESC_TYPE_UPSCALE_GETJITTEROFFSET;
    query.index = index;
    query.phaseCount = phase_count;
    query.pOutX = out_x;
    query.pOutY = out_y;
    return ffxQuery(nullptr, &query.header);
}

extern "C" uint32_t byro_fsr3_context_create(
    ByroFsr3Context** out_context,
    const ByroFsr3CreateDesc* desc) {
    if (!out_context || !desc || *out_context || !desc->vk_device || !desc->vk_physical_device ||
        !desc->vk_get_device_proc_addr || !desc->max_render_width || !desc->max_render_height ||
        !desc->max_upscale_width || !desc->max_upscale_height) {
        return FFX_API_RETURN_ERROR_PARAMETER;
    }

    auto* wrapper = new (std::nothrow) ByroFsr3Context{};
    if (!wrapper) {
        return FFX_API_RETURN_ERROR_MEMORY;
    }

    ffxCreateBackendVKDesc backend{};
    backend.header.type = FFX_API_CREATE_CONTEXT_DESC_TYPE_BACKEND_VK;
    backend.vkDevice = reinterpret_cast<VkDevice>(desc->vk_device);
    backend.vkPhysicalDevice = reinterpret_cast<VkPhysicalDevice>(desc->vk_physical_device);
    backend.vkDeviceProcAddr = reinterpret_cast<PFN_vkGetDeviceProcAddr>(desc->vk_get_device_proc_addr);

    ffxCreateContextDescUpscale upscale{};
    upscale.header.type = FFX_API_CREATE_CONTEXT_DESC_TYPE_UPSCALE;
    upscale.header.pNext = &backend.header;
    upscale.flags = (desc->high_dynamic_range ? kHdrFlag : 0u) |
                    (desc->debug_checking ? kDebugFlag : 0u);
    upscale.maxRenderSize = {desc->max_render_width, desc->max_render_height};
    upscale.maxUpscaleSize = {desc->max_upscale_width, desc->max_upscale_height};
    upscale.fpMessage = nullptr;

    const ffxAllocationCallbacks callbacks = allocation_callbacks();
    const uint32_t result = ffxCreateContext(&wrapper->context, &upscale.header, &callbacks);
    if (result != FFX_API_RETURN_OK) {
        delete wrapper;
        return result;
    }

    *out_context = wrapper;
    return FFX_API_RETURN_OK;
}

extern "C" uint32_t byro_fsr3_context_destroy(ByroFsr3Context** context) {
    if (!context || !*context) {
        return FFX_API_RETURN_ERROR_PARAMETER;
    }
    ByroFsr3Context* wrapper = *context;
    const ffxAllocationCallbacks callbacks = allocation_callbacks();
    const uint32_t result = ffxDestroyContext(&wrapper->context, &callbacks);
    if (result == FFX_API_RETURN_OK) {
        delete wrapper;
        *context = nullptr;
    }
    return result;
}

extern "C" const char* byro_fsr3_error_string(uint32_t error_code) {
    switch (error_code) {
    case FFX_API_RETURN_OK: return "ok";
    case FFX_API_RETURN_ERROR: return "unspecified error";
    case FFX_API_RETURN_ERROR_UNKNOWN_DESCTYPE: return "unknown descriptor type";
    case FFX_API_RETURN_ERROR_RUNTIME_ERROR: return "Vulkan or FSR runtime error";
    case FFX_API_RETURN_NO_PROVIDER: return "no FSR provider";
    case FFX_API_RETURN_ERROR_MEMORY: return "memory allocation failed";
    case FFX_API_RETURN_ERROR_PARAMETER: return "invalid parameter";
    default: return "unknown FSR error";
    }
}
