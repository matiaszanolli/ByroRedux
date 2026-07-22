#include <FidelityFX/host/backends/vk/ffx_vk.h>

// The shared v1.1.4 Vulkan backend exposes one optional frame-generation
// callback in FfxInterface even when only the upscaler is built. Linking the
// real implementation would pull in the frame-interpolation swapchain. Keep
// the ABI slot valid but make the unsupported operation fail explicitly.
extern "C" FfxErrorCode ffxSetFrameGenerationConfigToSwapchainVK(
    FfxFrameGenerationConfig const*) {
    return FFX_ERROR_INVALID_ARGUMENT;
}
