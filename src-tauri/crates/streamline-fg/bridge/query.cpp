// Only this translation unit sees Streamline's C++ types. The Rust boundary
// contains fixed-width scalars, copied strings, and opaque function addresses.
#include <cstddef>
#include <cstring>
#include <cstdlib>

static bool referenceParameters() noexcept {
    static const bool enabled = [] {
        char value[2]{}; size_t size{};
        return getenv_s(&size, value, sizeof(value), "NS_STREAMLINE_TARGET_REFERENCE_PARAMS") == 0 && std::strcmp(value, "1") == 0;
    }();
    return enabled;
}
#include <vulkan/vulkan_core.h>
#include "sl_helpers_vk.h"

static bool estimateMotion() noexcept {
    static const bool enabled = [] {
        char value[2]{}; size_t size{};
        return getenv_s(&size,value,sizeof(value),"NS_STREAMLINE_TARGET_MOTION")==0 && std::strcmp(value,"1")==0;
    }();
    return enabled;
}

struct ProbeRequirements {
    uint32_t flags, cpuThreads, viewports, graphicsQueues, computeQueues, opticalFlowQueues;
    uint32_t counts[5]; // instance extensions, device extensions, features12, features13, tags
    uint32_t tags[64];
    char names[4][64][128];
    uint32_t versions[4][3]; // OS detected/required, driver detected/required
};
static_assert(sizeof(sl::VulkanInfo) == 96);
static_assert(sizeof(ProbeRequirements) == 33116);
static_assert(offsetof(ProbeRequirements, names) == 300);

static bool copyNames(char (&dest)[64][128], uint32_t count, const char** source) {
    if (count > 64 || (count && !source)) return false;
    for (uint32_t i = 0; i < count; ++i) {
        if (!source[i]) return false;
        const auto len = strnlen_s(source[i], 128);
        if (len >= 128) return false;
        std::memcpy(dest[i], source[i], len + 1);
    }
    return true;
}
extern "C" int32_t probe_sl_init(void* function, const wchar_t* directory) noexcept {
    if (!function || !directory) return -1000;
    try {
        // Stable storage: retained for the entire SDK session.
        static const sl::Feature features[] = {sl::kFeatureDLSS_G, sl::kFeatureReflex, sl::kFeaturePCL};
        static const wchar_t* paths[1];
        paths[0] = directory;
        sl::Preferences prefs;
        prefs.pathsToPlugins = paths;
        prefs.numPathsToPlugins = 1;
        prefs.pathToLogsAndData = directory;
        prefs.logLevel = sl::LogLevel::eVerbose;
        prefs.featuresToLoad = features;
        prefs.numFeaturesToLoad = 3;
        prefs.flags = sl::PreferenceFlags::eDisableCLStateTracking
                    | sl::PreferenceFlags::eUseManualHooking
                    | sl::PreferenceFlags::eUseFrameBasedResourceTagging;
        prefs.renderAPI = sl::RenderAPI::eVulkan;
        prefs.engine = sl::EngineType::eCustom;
        prefs.engineVersion = "NSEmu-P0-Probe-0.1";
        prefs.projectId = "fd3bfdaf-72d1-48b9-a4ae-4dcb031c535b";
        return static_cast<int32_t>(reinterpret_cast<PFun_slInit*>(function)(prefs, sl::kSDKVersion));
    } catch (...) { return -1001; }
}
extern "C" int32_t probe_sl_requirements(void* function, uint32_t feature, ProbeRequirements* out) noexcept {
    if (!function || !out) return -1000;
    *out = {};
    try {
        sl::FeatureRequirements req;
        const auto result = reinterpret_cast<PFun_slGetFeatureRequirements*>(function)(feature, req);
        if (result != sl::Result::eOk) return static_cast<int32_t>(result);
        if (req.numRequiredTags > 64 || (req.numRequiredTags && !req.requiredTags)) return -1002;
        if (!copyNames(out->names[0], req.vkNumInstanceExtensions, req.vkInstanceExtensions)
            || !copyNames(out->names[1], req.vkNumDeviceExtensions, req.vkDeviceExtensions)
            || !copyNames(out->names[2], req.vkNumFeatures12, req.vkFeatures12)
            || !copyNames(out->names[3], req.vkNumFeatures13, req.vkFeatures13)) return -1002;
        out->flags = static_cast<uint32_t>(req.flags);
        out->cpuThreads = req.maxNumCPUThreads;
        out->viewports = req.maxNumViewports;
        out->graphicsQueues = req.vkNumGraphicsQueuesRequired;
        out->computeQueues = req.vkNumComputeQueuesRequired;
        out->opticalFlowQueues = req.vkNumOpticalFlowQueuesRequired;
        out->counts[0] = req.vkNumInstanceExtensions;
        out->counts[1] = req.vkNumDeviceExtensions;
        out->counts[2] = req.vkNumFeatures12;
        out->counts[3] = req.vkNumFeatures13;
        out->counts[4] = req.numRequiredTags;
        for (uint32_t i = 0; i < req.numRequiredTags; ++i) out->tags[i] = req.requiredTags[i];
        const sl::Version versions[] = {req.osVersionDetected, req.osVersionRequired,
            req.driverVersionDetected, req.driverVersionRequired};
        for (uint32_t i = 0; i < 4; ++i) {
            out->versions[i][0] = versions[i].major;
            out->versions[i][1] = versions[i].minor;
            out->versions[i][2] = versions[i].build;
        }
        return 0;
    } catch (...) { return -1001; }
}
extern "C" int32_t probe_sl_shutdown(void* function) noexcept {
    if (!function) return -1000;
    try { return static_cast<int32_t>(reinterpret_cast<PFun_slShutdown*>(function)()); }
    catch (...) { return -1001; }
}
extern "C" void probe_sl_abi(uint64_t* out) noexcept {
    sl::Preferences p;
    sl::FeatureRequirements r;
    out[0] = sl::kSDKVersion;
    out[1] = sizeof(p);
    out[2] = p.structVersion;
    out[3] = sizeof(r);
    out[4] = r.structVersion;
    out[5] = sizeof(ProbeRequirements);
    sl::VulkanInfo vk;
    out[6] = sizeof(vk);
    out[7] = vk.structVersion;
}
extern "C" int32_t probe_sl_set_vulkan(void* function, uint64_t instance,
    uint64_t physical, uint64_t device, uint32_t family,
    uint32_t graphicsStart, uint32_t computeStart) noexcept {
    if (!function || !instance || !physical || !device) return -1000;
    try {
        sl::VulkanInfo info;
        info.instance = reinterpret_cast<VkInstance>(instance);
        info.physicalDevice = reinterpret_cast<VkPhysicalDevice>(physical);
        info.device = reinterpret_cast<VkDevice>(device);
        info.graphicsQueueFamily = family;
        info.graphicsQueueIndex = graphicsStart;
        info.computeQueueFamily = family;
        info.computeQueueIndex = computeStart;
        // Fixed manual-hooking guide explicitly supports optical-flow interop
        // without a native OF queue. No native OF feature is enabled here.
        info.useNativeOpticalFlowMode = false;
        return static_cast<int32_t>(reinterpret_cast<PFun_slSetVulkanInfo*>(function)(info));
    } catch (...) { return -1001; }
}
extern "C" int32_t probe_sl_supported(void* function, uint32_t feature, uint64_t physical) noexcept {
    if (!function || !physical) return -1000;
    try {
        sl::AdapterInfo adapter;
        adapter.vkPhysicalDevice = reinterpret_cast<void*>(physical);
        return static_cast<int32_t>(reinterpret_cast<PFun_slIsFeatureSupported*>(function)(feature, adapter));
    } catch (...) { return -1001; }
}// Private, versioned experimental ABI, independent of the official SDK structs.
struct ProbeLayerRouteV1 {
    uint32_t size;
    uint32_t version;
    VkInstance instance;
    VkPhysicalDevice physical;
    VkDevice device;
    PFN_vkGetInstanceProcAddr gipa;
    PFN_vkGetDeviceProcAddr gdpa;
};
static_assert(sizeof(ProbeLayerRouteV1) == 48, "private route ABI");
static_assert(offsetof(ProbeLayerRouteV1, gipa) == 32, "private callback ABI");
extern "C" int32_t probe_sl_register_route(void* function, uint64_t instance,
    uint64_t physical, uint64_t device, PFN_vkGetInstanceProcAddr gipa,
    PFN_vkGetDeviceProcAddr gdpa) noexcept {
    if (!function || !instance || !physical || !device || !gipa || !gdpa) return -1000;
    try {
        ProbeLayerRouteV1 route{sizeof(ProbeLayerRouteV1), 1,
            reinterpret_cast<VkInstance>(instance), reinterpret_cast<VkPhysicalDevice>(physical),
            reinterpret_cast<VkDevice>(device), gipa, gdpa};
        using Register = sl::Result(*)(const ProbeLayerRouteV1*);
        return static_cast<int32_t>(reinterpret_cast<Register>(function)(&route));
    } catch (...) { return -1001; }
}


#include "sl_dlss_g.h"
#include "sl_reflex.h"
#include "sl_pcl.h"
struct ProbeFGApi { void *feature, *token, *constants, *tags; };
struct ProbeFGState { uint32_t status, minimum, presented, maximum; uint64_t fence, value; };
struct ProbeFGResource { uint64_t image, memory, view; uint32_t width, height, format, usage; };
static_assert(sizeof(ProbeFGState) == 32);
static_assert(sizeof(ProbeFGResource) == 40);
template<class T> static T* fgFunction(const ProbeFGApi* api, sl::Feature feature, const char* name) {
    void* ptr{};
    auto result = reinterpret_cast<PFun_slGetFeatureFunction*>(api->feature)(feature, name, ptr);
    if (result != sl::Result::eOk || !ptr) throw int(result);
    return reinterpret_cast<T*>(ptr);
}
extern "C" int32_t probe_fg_state(const ProbeFGApi* api, ProbeFGState* output) noexcept {
    try {
        sl::DLSSGState state;
        auto result = fgFunction<PFun_slDLSSGGetState>(api, sl::kFeatureDLSS_G, "slDLSSGGetState")(sl::ViewportHandle(0), state, nullptr);
        *output = {uint32_t(state.status), state.minWidthOrHeight, state.numFramesActuallyPresented,
            state.numFramesToGenerateMax, uint64_t(state.inputsProcessingCompletionFence), state.lastPresentInputsProcessingCompletionFenceValue};
        return int32_t(result);
    } catch (...) { return -1001; }
}
extern "C" int32_t probe_fg_options(const ProbeFGApi* api, uint32_t enabled, uint32_t width, uint32_t height, uint32_t count, uint32_t frameLimitUs) noexcept {
    try {
        sl::ReflexOptions reflex;
        reflex.mode = sl::ReflexMode::eLowLatency;
        reflex.frameLimitUs = frameLimitUs;
        auto r = fgFunction<PFun_slReflexSetOptions>(api, sl::kFeatureReflex, "slReflexSetOptions")(reflex);
        if (r != sl::Result::eOk) return int32_t(r);
        sl::DLSSGOptions options;
        options.mode = enabled ? sl::DLSSGMode::eOn : sl::DLSSGMode::eOff;
        options.numFramesToGenerate = 1;
        options.numBackBuffers=count;options.colorWidth=options.mvecDepthWidth=width;options.colorHeight=options.mvecDepthHeight=height;
        options.colorBufferFormat=VK_FORMAT_B8G8R8A8_UNORM;options.depthBufferFormat=VK_FORMAT_R32_SFLOAT;options.mvecBufferFormat=referenceParameters() && !estimateMotion() ? VK_FORMAT_R16G16_SFLOAT : VK_FORMAT_R32G32_SFLOAT;
        return int32_t(fgFunction<PFun_slDLSSGSetOptions>(api, sl::kFeatureDLSS_G, "slDLSSGSetOptions")(sl::ViewportHandle(0), options));
    } catch (...) { return -1001; }
}
extern "C" int32_t probe_fg_begin(const ProbeFGApi* api, uint32_t frame, uint64_t* token) noexcept {
    try {
        sl::FrameToken* ptr{};
        auto r = reinterpret_cast<PFun_slGetNewFrameToken*>(api->token)(ptr, &frame);
        if (r != sl::Result::eOk) return int32_t(r);
        *token = uint64_t(ptr);
        return int32_t(fgFunction<PFun_slReflexSleep>(api, sl::kFeatureReflex, "slReflexSleep")(*ptr));
    } catch (...) { return -1001; }
}
extern "C" int32_t probe_fg_marker(const ProbeFGApi* api, uint64_t token, uint32_t marker) noexcept {
    try { return int32_t(fgFunction<PFun_slPCLSetMarker>(api, sl::kFeaturePCL, "slPCLSetMarker")(sl::PCLMarker(marker), *reinterpret_cast<sl::FrameToken*>(token))); }
    catch (...) { return -1001; }
}
extern "C" int32_t probe_fg_inputs(const ProbeFGApi* api, uint64_t token, uint32_t reset,
    const ProbeFGResource* inputs) noexcept {
    try {
        auto& frame = *reinterpret_cast<sl::FrameToken*>(token);
        sl::Constants constants;
        sl::float4x4 identity;
        for (uint32_t i=0; i<4; ++i) identity.setRow(i, {i==0 ? 1.0f : 0.0f, i==1 ? 1.0f : 0.0f, i==2 ? 1.0f : 0.0f, i==3 ? 1.0f : 0.0f});
        constants.cameraViewToClip = constants.clipToCameraView = constants.clipToLensClip = constants.clipToPrevClip = constants.prevClipToClip = identity;
        constants.jitterOffset = constants.cameraPinholeOffset = {0,0};
        constants.mvecScale = {1,1};
        constants.cameraPos = {0,0,0}; constants.cameraUp = {0,1,0};
        constants.cameraRight = {1,0,0}; constants.cameraFwd = {0,0,1};
        constants.cameraNear = 0.1f;
        constants.cameraFar = referenceParameters() ? 10000.0f : 1000.0f;
        constants.cameraFOV = referenceParameters() ? 1.0f : 1.04719755f;
        if (referenceParameters()) {
            constants.motionVectorsInvalidValue = 0.0f;
            constants.orthographicProjection = sl::eFalse;
            constants.motionVectorsDilated = sl::eFalse;
            constants.motionVectorsJittered = sl::eFalse;
            constants.minRelativeLinearDepthObjectSeparation = 40.0f;
        }
        constants.cameraAspectRatio = float(inputs[0].width)/float(inputs[0].height);
        constants.depthInverted = sl::eFalse; constants.cameraMotionIncluded = sl::eTrue;
        constants.motionVectors3D = sl::eFalse; constants.reset = reset ? sl::eTrue : sl::eFalse;
        auto r = reinterpret_cast<PFun_slSetConstants*>(api->constants)(constants, frame, sl::ViewportHandle(0));
        if (r != sl::Result::eOk) return int32_t(r);
        sl::Resource resources[2];
        for (uint32_t i=0;i<2;++i) {
            resources[i] = sl::Resource(sl::ResourceType::eTex2d, reinterpret_cast<void*>(inputs[i].image),
                reinterpret_cast<void*>(inputs[i].memory), reinterpret_cast<void*>(inputs[i].view), VK_IMAGE_LAYOUT_GENERAL);
            resources[i].width=inputs[i].width; resources[i].height=inputs[i].height;
            resources[i].nativeFormat=inputs[i].format; resources[i].mipLevels=1; resources[i].arrayLayers=1;
            resources[i].flags=0; resources[i].usage=inputs[i].usage;
        }
        sl::Extent extent{0,0,inputs[0].width,inputs[0].height};
        sl::ResourceTag tags[] = {
            {inputs[0].image ? &resources[0] : nullptr, sl::kBufferTypeDepth, sl::ResourceLifecycle::eValidUntilPresent, &extent},
            {inputs[1].image ? &resources[1] : nullptr, sl::kBufferTypeMotionVectors, sl::ResourceLifecycle::eValidUntilPresent, &extent}};
        return int32_t(reinterpret_cast<PFun_slSetTagForFrame*>(api->tags)(frame, sl::ViewportHandle(0), tags, 2, nullptr));
    } catch (...) { return -1001; }
}
