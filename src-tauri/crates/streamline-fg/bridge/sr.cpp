#include "sl_dlss.h"
// Diagnostic-only SR entry points: all resources must be GENERAL, and the
// caller retains them until its submitted command buffer has completed.
extern "C" int32_t probe_sr_options(const ProbeFGApi* api, uint32_t width,
    uint32_t height, uint32_t* inputSize) noexcept {
    if (!api || !api->feature || !inputSize || !width || !height) return -1000;
    try {
        sl::DLSSOptions options;
        options.mode = sl::DLSSMode::eMaxQuality;
        options.outputWidth = width; options.outputHeight = height;
        options.colorBuffersHDR = sl::eFalse;
        options.useAutoExposure = sl::eTrue;
        sl::DLSSOptimalSettings settings;
        auto r = fgFunction<PFun_slDLSSGetOptimalSettings>(api, sl::kFeatureDLSS,
            "slDLSSGetOptimalSettings")(options, settings);
        if (r != sl::Result::eOk) return int32_t(r);
        inputSize[0] = settings.optimalRenderWidth;
        inputSize[1] = settings.optimalRenderHeight;
        return int32_t(fgFunction<PFun_slDLSSSetOptions>(api, sl::kFeatureDLSS,
            "slDLSSSetOptions")(sl::ViewportHandle(0), options));
    } catch (int r) { return r ? r : -1001; } catch (...) { return -1001; }
}
extern "C" int32_t probe_sr_evaluate(const ProbeFGApi* api, void* evaluate,
    uint64_t command, uint32_t frameIndex, uint32_t reset,
    const ProbeFGResource* inputs) noexcept {
    if (!api || !api->token || !api->constants || !api->tags || !evaluate || !command || !inputs) return -1000;
    try {
        sl::FrameToken* frame{};
        auto r = reinterpret_cast<PFun_slGetNewFrameToken*>(api->token)(frame, &frameIndex);
        if (r != sl::Result::eOk) return int32_t(r);
        // The synthetic scene is stationary: constant depth, zero motion, zero jitter.
        // These inputs only prove GPU execution, not temporal reconstruction quality.
        sl::Constants constants;
        sl::float4x4 identity;
        for (uint32_t i=0; i<4; ++i) identity.setRow(i,
            {i==0 ? 1.0f : 0.0f, i==1 ? 1.0f : 0.0f, i==2 ? 1.0f : 0.0f, i==3 ? 1.0f : 0.0f});
        constants.cameraViewToClip = constants.clipToCameraView = constants.clipToLensClip =
            constants.clipToPrevClip = constants.prevClipToClip = identity;
        constants.jitterOffset = constants.cameraPinholeOffset = {0,0};
        constants.mvecScale = {1,1};
        constants.cameraPos = {0,0,0}; constants.cameraUp = {0,1,0};
        constants.cameraRight = {1,0,0}; constants.cameraFwd = {0,0,1};
        constants.cameraNear = 0.1f; constants.cameraFar = 1000.0f;
        constants.cameraFOV = 1.04719755f;
        constants.cameraAspectRatio = float(inputs[0].width)/float(inputs[0].height);
        constants.depthInverted = sl::eFalse; constants.cameraMotionIncluded = sl::eTrue;
        constants.motionVectors3D = constants.motionVectorsDilated = constants.motionVectorsJittered = sl::eFalse;
        constants.orthographicProjection = sl::eFalse;
        constants.reset = reset ? sl::eTrue : sl::eFalse;
        r = reinterpret_cast<PFun_slSetConstants*>(api->constants)(constants, *frame, sl::ViewportHandle(0));
        if (r != sl::Result::eOk) return int32_t(r);
        sl::Resource resources[4];
        sl::Extent extents[4];
        sl::ResourceTag tags[4];
        const sl::BufferType types[] = {sl::kBufferTypeScalingInputColor,
            sl::kBufferTypeScalingOutputColor, sl::kBufferTypeDepth, sl::kBufferTypeMotionVectors};
        for (uint32_t i=0;i<4;++i) {
            if (!inputs[i].image || !inputs[i].view || !inputs[i].width || !inputs[i].height) return -1000;
            resources[i] = sl::Resource(sl::ResourceType::eTex2d,
                reinterpret_cast<void*>(inputs[i].image), reinterpret_cast<void*>(inputs[i].memory),
                reinterpret_cast<void*>(inputs[i].view), VK_IMAGE_LAYOUT_GENERAL);
            resources[i].width=inputs[i].width; resources[i].height=inputs[i].height;
            resources[i].nativeFormat=inputs[i].format; resources[i].mipLevels=1;
            resources[i].arrayLayers=1; resources[i].usage=inputs[i].usage; resources[i].flags=0;
            extents[i] = {0,0,inputs[i].width,inputs[i].height};
            tags[i] = {&resources[i],types[i],sl::ResourceLifecycle::eValidUntilEvaluate,&extents[i]};
        }
        r = reinterpret_cast<PFun_slSetTagForFrame*>(api->tags)(*frame,sl::ViewportHandle(0),tags,4,reinterpret_cast<sl::CommandBuffer*>(command));
        if (r != sl::Result::eOk) return int32_t(r);
        sl::ViewportHandle viewport(0);
        const sl::BaseStructure* values[] = {&viewport};
        return int32_t(reinterpret_cast<PFun_slEvaluateFeature*>(evaluate)(sl::kFeatureDLSS,
            *frame,values,1,reinterpret_cast<sl::CommandBuffer*>(command)));
    } catch (...) { return -1001; }
}
extern "C" int32_t probe_sr_free(void* function) noexcept {
    if (!function) return -1000;
    try { return int32_t(reinterpret_cast<PFun_slFreeResources*>(function)(sl::kFeatureDLSS,sl::ViewportHandle(0))); }
    catch (...) { return -1001; }
}

// Game path uses viewport 1 and shares the presentation frame token with FG.
extern "C" int32_t target_sr_options(const ProbeFGApi* api, uint32_t width,
    uint32_t height, uint32_t mode, uint32_t preset, uint32_t* inputSize) noexcept {
    if (!api || !api->feature || !inputSize || !width || !height) return -1000;
    try {
        sl::DLSSOptions options;
        if (mode != 1 && mode != 2 && mode != 3 && mode != 6) return -1000;
        if (preset != 0 && preset != 10 && preset != 11 && preset != 12 && preset != 13) return -1000;
        options.dlaaPreset = options.qualityPreset = options.balancedPreset =
            options.performancePreset = options.ultraPerformancePreset = options.ultraQualityPreset =
            static_cast<sl::DLSSPreset>(preset);
        options.mode = static_cast<sl::DLSSMode>(mode);
        options.outputWidth = width; options.outputHeight = height;
        options.colorBuffersHDR = sl::eFalse;
        options.useAutoExposure = sl::eTrue;
        // The requested dimensions are explicit. Select a supported dynamic range;
        // never silently replace the slider value with a preset's optimal size.
        bool supported = false;
        const uint32_t modes[] = {mode, 6, 3, 2, 1};
        for (auto candidate : modes) {
            options.mode = static_cast<sl::DLSSMode>(candidate);
            sl::DLSSOptimalSettings settings;
            auto r = fgFunction<PFun_slDLSSGetOptimalSettings>(api, sl::kFeatureDLSS,
                "slDLSSGetOptimalSettings")(options, settings);
            if (r != sl::Result::eOk) continue;
            if ((inputSize[0] == settings.optimalRenderWidth && inputSize[1] == settings.optimalRenderHeight) ||
                (inputSize[0] >= settings.renderWidthMin && inputSize[0] <= settings.renderWidthMax &&
                 inputSize[1] >= settings.renderHeightMin && inputSize[1] <= settings.renderHeightMax)) {
                supported = true;
                break;
            }
        }
        if (!supported) return -1002;
        return int32_t(fgFunction<PFun_slDLSSSetOptions>(api, sl::kFeatureDLSS,
            "slDLSSSetOptions")(sl::ViewportHandle(1), options));
    } catch (int r) { return r ? r : -1001; } catch (...) { return -1001; }
}
extern "C" int32_t target_sr_evaluate(const ProbeFGApi* api, void* evaluate,
    uint64_t command, uint64_t token, uint32_t reset, float motionScaleX, float motionScaleY,
    const ProbeFGResource* inputs) noexcept {
    if (!api || !api->token || !api->constants || !api->tags || !evaluate || !command || !inputs) return -1000;
    try {
        auto* frame = reinterpret_cast<sl::FrameToken*>(token);
        if (!frame) return -1000;
        auto r = sl::Result::eOk;
        // Present-source path: estimated UV motion or zero motion, constant depth, zero jitter.
        sl::Constants constants;
        sl::float4x4 identity;
        for (uint32_t i=0; i<4; ++i) identity.setRow(i,
            {i==0 ? 1.0f : 0.0f, i==1 ? 1.0f : 0.0f, i==2 ? 1.0f : 0.0f, i==3 ? 1.0f : 0.0f});
        constants.cameraViewToClip = constants.clipToCameraView = constants.clipToLensClip =
            constants.clipToPrevClip = constants.prevClipToClip = identity;
        constants.jitterOffset = constants.cameraPinholeOffset = {0,0};
        constants.mvecScale = {motionScaleX,motionScaleY};
        constants.cameraPos = {0,0,0}; constants.cameraUp = {0,1,0};
        constants.cameraRight = {1,0,0}; constants.cameraFwd = {0,0,1};
        constants.cameraNear = 0.1f; constants.cameraFar = 1000.0f;
        constants.cameraFOV = 1.04719755f;
        constants.cameraAspectRatio = float(inputs[0].width)/float(inputs[0].height);
        constants.depthInverted = sl::eFalse; constants.cameraMotionIncluded = sl::eTrue;
        constants.motionVectors3D = constants.motionVectorsDilated = constants.motionVectorsJittered = sl::eFalse;
        constants.orthographicProjection = sl::eFalse;
        constants.reset = reset ? sl::eTrue : sl::eFalse;
        r = reinterpret_cast<PFun_slSetConstants*>(api->constants)(constants, *frame, sl::ViewportHandle(1));
        if (r != sl::Result::eOk) return int32_t(r);
        sl::Resource resources[4];
        sl::Extent extents[4];
        sl::ResourceTag tags[4];
        const sl::BufferType types[] = {sl::kBufferTypeScalingInputColor,
            sl::kBufferTypeScalingOutputColor, sl::kBufferTypeDepth, sl::kBufferTypeMotionVectors};
        for (uint32_t i=0;i<4;++i) {
            if (!inputs[i].image || !inputs[i].view || !inputs[i].width || !inputs[i].height) return -1000;
            resources[i] = sl::Resource(sl::ResourceType::eTex2d,
                reinterpret_cast<void*>(inputs[i].image), reinterpret_cast<void*>(inputs[i].memory),
                reinterpret_cast<void*>(inputs[i].view), VK_IMAGE_LAYOUT_GENERAL);
            resources[i].width=inputs[i].width; resources[i].height=inputs[i].height;
            resources[i].nativeFormat=inputs[i].format; resources[i].mipLevels=1;
            resources[i].arrayLayers=1; resources[i].usage=inputs[i].usage; resources[i].flags=0;
            extents[i] = {0,0,inputs[i].width,inputs[i].height};
            tags[i] = {&resources[i],types[i],sl::ResourceLifecycle::eValidUntilEvaluate,&extents[i]};
        }
        r = reinterpret_cast<PFun_slSetTagForFrame*>(api->tags)(*frame,sl::ViewportHandle(1),tags,4,reinterpret_cast<sl::CommandBuffer*>(command));
        if (r != sl::Result::eOk) return int32_t(r);
        sl::ViewportHandle viewport(1);
        const sl::BaseStructure* values[] = {&viewport};
        return int32_t(reinterpret_cast<PFun_slEvaluateFeature*>(evaluate)(sl::kFeatureDLSS,
            *frame,values,1,reinterpret_cast<sl::CommandBuffer*>(command)));
    } catch (...) { return -1001; }
}
extern "C" int32_t target_sr_free(void* function) noexcept {
    if (!function) return -1000;
    try { return int32_t(reinterpret_cast<PFun_slFreeResources*>(function)(sl::kFeatureDLSS,sl::ViewportHandle(1))); }
    catch (...) { return -1001; }
}
