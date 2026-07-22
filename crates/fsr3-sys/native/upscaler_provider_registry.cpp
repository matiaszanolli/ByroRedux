#include "ffx_provider.h"
#include "ffx_provider_fsr3upscale.h"

#include <cstdint>

namespace {

const ffxProvider* const kProviders[] = {
    &ffxProvider_FSR3Upscale::Instance,
};

} // namespace

const ffxProvider* GetffxProvider(ffxStructType_t descType, uint64_t overrideId, void*) {
    for (const ffxProvider* provider : kProviders) {
        if (provider->CanProvide(descType) && (!overrideId || provider->GetId() == overrideId)) {
            return provider;
        }
    }
    return nullptr;
}

const ffxProvider* GetAssociatedProvider(ffxContext* context) {
    if (!context || !*context) {
        return nullptr;
    }
    const auto* header = static_cast<const InternalContextHeader*>(*context);
    return header->provider;
}

uint64_t GetProviderCount(ffxStructType_t descType, void* device) {
    return GetProviderVersions(descType, device, UINT64_MAX, nullptr, nullptr);
}

uint64_t GetProviderVersions(
    ffxStructType_t descType,
    void*,
    uint64_t capacity,
    uint64_t* versionIds,
    const char** versionNames) {
    uint64_t count = 0;
    for (const ffxProvider* provider : kProviders) {
        if (count >= capacity) {
            break;
        }
        if (!provider->CanProvide(descType)) {
            continue;
        }
        if (versionIds) {
            versionIds[count] = provider->GetId();
        }
        if (versionNames) {
            versionNames[count] = provider->GetVersionName();
        }
        ++count;
    }
    return count;
}
