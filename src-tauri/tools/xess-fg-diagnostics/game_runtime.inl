// Opt-in game renderer: zero motion/fixed depth, no optical flow or Streamline.
#include <array>
struct GameHandle {
    HANDLE value=nullptr;
    ~GameHandle(){if(value)CloseHandle(value);}
};
struct GameRuntime {
    struct FrameWork {
        ComPtr<ID3D12CommandAllocator> allocator;
        ComPtr<ID3D12GraphicsCommandList> commands;
        uint64_t completion=0;
    };
    Module ll,fg;
    Api api;
    ComPtr<ID3D12DeviceFactory> deviceFactory;
    ComPtr<IDXGIFactory6> factory;
    std::unique_ptr<Gpu> gpu;
    std::unique_ptr<Context> context;
    ComPtr<IDXGISwapChain4> swap;
    ComPtr<ID3D12Resource> color,motion,depth;
    ComPtr<ID3D12Fence> sharedFence;
    GameHandle colorHandle,fenceHandle;
    std::unique_ptr<Upload> upload;
    std::array<FrameWork,3> work;
    uint32_t width,height,frame=0;
    bool on=true, suspended=false, resetHistory=true;
    Clock::time_point previous=Clock::now();
    GameRuntime(const wchar_t* sdk,const wchar_t* agility,HWND window,const uint8_t* luid,uint32_t w,uint32_t h)
        :ll(std::wstring(sdk)+L"/bin/libxell.dll"),fg(std::wstring(sdk)+L"/bin/libxess_fg.dll"),api(fg,ll),width(w),height(h) {
        ComPtr<ID3D12SDKConfiguration1> config;
        hr(D3D12GetInterface(CLSID_D3D12SDKConfiguration,IID_PPV_ARGS(&config)),"game Agility configuration");
        char agilityUtf8[65536]{};
        if(!WideCharToMultiByte(CP_UTF8,0,agility,-1,agilityUtf8,sizeof(agilityUtf8),nullptr,nullptr))throw std::runtime_error("Agility path encoding");
        hr(config->CreateDeviceFactory(619,agilityUtf8,IID_PPV_ARGS(&deviceFactory)),"game Agility factory");
        hr(CreateDXGIFactory2(0,IID_PPV_ARGS(&factory)),"game DXGI factory");
        ComPtr<IDXGIAdapter1> selected;
        DXGI_ADAPTER_DESC1 selectedDesc{};
        for(uint32_t i=0;;++i){
            ComPtr<IDXGIAdapter1> adapter;
            if(factory->EnumAdapters1(i,&adapter)==DXGI_ERROR_NOT_FOUND)break;
            DXGI_ADAPTER_DESC1 desc{};hr(adapter->GetDesc1(&desc),"game adapter description");
            if(!(desc.Flags&DXGI_ADAPTER_FLAG_SOFTWARE)&&std::memcmp(&desc.AdapterLuid,luid,8)==0){selected=adapter;selectedDesc=desc;break;}
        }
        if(!selected)throw std::runtime_error("No D3D12 adapter matches game Vulkan LUID");
        gpu=std::make_unique<Gpu>(deviceFactory.Get(),selected.Get());
        context=std::make_unique<Context>(api);
        checked(api.xefgSwapChainD3D12CreateContext(gpu->device.Get(),&context->fg),"game FG context");
        checked(api.xefgSwapChainSetLoggingCallback(context->fg,XEFG_SWAPCHAIN_LOGGING_LEVEL_INFO,fg_log,nullptr),"game FG logging");
        checked(api.xellD3D12CreateContext(gpu->device.Get(),&context->ll),"game XeLL context");
        checked(api.xellSetLoggingCallback(context->ll,XELL_LOGGING_LEVEL_INFO,ll_log),"game XeLL logging");
        checked(api.xefgSwapChainSetLatencyReduction(context->fg,context->ll),"game FG XeLL attach");
        DXGI_SWAP_CHAIN_DESC1 desc{};desc.Width=w;desc.Height=h;desc.Format=DXGI_FORMAT_R8G8B8A8_UNORM;
        desc.SampleDesc.Count=1;desc.BufferUsage=DXGI_USAGE_RENDER_TARGET_OUTPUT;
        desc.BufferCount=3;desc.SwapEffect=DXGI_SWAP_EFFECT_FLIP_DISCARD;desc.Flags=DXGI_SWAP_CHAIN_FLAG_ALLOW_TEARING;
        xefg_swapchain_d3d12_init_params_t init{};init.maxInterpolatedFrames=1;init.uiMode=XEFG_SWAPCHAIN_UI_MODE_NONE;
        checked(api.xefgSwapChainD3D12InitFromSwapChainDesc(context->fg,window,&desc,nullptr,gpu->queue.Get(),factory.Get(),&init),"game FG proxy init");
        checked(api.xefgSwapChainD3D12GetSwapChainPtr(context->fg,IID_PPV_ARGS(&swap)),"game FG proxy pointer");
        xell_sleep_params_t sleep{};sleep.bLowLatencyMode=1;
        checked(api.xellSetSleepMode(context->ll,&sleep),"game XeLL required pacing");
        checked(api.xefgSwapChainSetEnabled(context->fg,1),"game FG enable");
        const uint32_t mw=std::max(64u,w/2),mh=std::max(64u,h/2);
        motion=texture(*gpu,mw,mh,DXGI_FORMAT_R16G16_FLOAT);depth=texture(*gpu,mw,mh,DXGI_FORMAT_R32_FLOAT);
        upload=std::make_unique<Upload>(*gpu,mw,mh);upload->render(0);
        // Replace the analytic demo's velocity with the explicitly requested zero field.
        std::memset(upload->data+upload->planeStride,0,static_cast<size_t>(upload->pitch)*mh);
        D3D12_HEAP_PROPERTIES heap{};heap.Type=D3D12_HEAP_TYPE_DEFAULT;
        D3D12_RESOURCE_DESC image{};image.Dimension=D3D12_RESOURCE_DIMENSION_TEXTURE2D;
        image.Width=w;image.Height=h;image.DepthOrArraySize=1;image.MipLevels=1;image.Format=desc.Format;image.SampleDesc.Count=1;
        image.Flags=D3D12_RESOURCE_FLAG_ALLOW_SIMULTANEOUS_ACCESS|D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS;
        hr(gpu->device->CreateCommittedResource(&heap,D3D12_HEAP_FLAG_SHARED,&image,D3D12_RESOURCE_STATE_COMMON,nullptr,IID_PPV_ARGS(&color)),"game shared color");
        hr(gpu->device->CreateFence(0,D3D12_FENCE_FLAG_SHARED,IID_PPV_ARGS(&sharedFence)),"game shared fence");
        hr(gpu->device->CreateSharedHandle(color.Get(),nullptr,GENERIC_ALL,nullptr,&colorHandle.value),"game color export");
        hr(gpu->device->CreateSharedHandle(sharedFence.Get(),nullptr,GENERIC_ALL,nullptr,&fenceHandle.value),"game fence export");
        verifyGuides();
        for(auto& slot:work){
            hr(gpu->device->CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT,IID_PPV_ARGS(&slot.allocator)),"game frame allocator");
            hr(gpu->device->CreateCommandList(0,D3D12_COMMAND_LIST_TYPE_DIRECT,slot.allocator.Get(),nullptr,IID_PPV_ARGS(&slot.commands)),"game frame commands");
            hr(slot.commands->Close(),"game initial frame close");
        }
        emit("game_runtime_ready","\"vendor\":"+std::to_string(selectedDesc.VendorId)+",\"width\":"+std::to_string(w)+",\"height\":"+std::to_string(h)+",\"zero_motion\":true,\"depth\":0.5,\"nvof\":false,\"streamline\":false");
    }
    void verifyGuides() {
        // Read the actual GPU textures once, outside measured gameplay. This
        // catches footprint alignment/plane-offset errors at arbitrary sizes.
        D3D12_HEAP_PROPERTIES heap{};heap.Type=D3D12_HEAP_TYPE_READBACK;
        D3D12_RESOURCE_DESC desc{};desc.Dimension=D3D12_RESOURCE_DIMENSION_BUFFER;
        desc.Width=upload->planeStride*2;desc.Height=1;desc.DepthOrArraySize=1;
        desc.MipLevels=1;desc.SampleDesc.Count=1;desc.Layout=D3D12_TEXTURE_LAYOUT_ROW_MAJOR;
        ComPtr<ID3D12Resource> readback;
        hr(gpu->device->CreateCommittedResource(&heap,D3D12_HEAP_FLAG_NONE,&desc,D3D12_RESOURCE_STATE_COPY_DEST,nullptr,IID_PPV_ARGS(&readback)),"guide readback buffer");
        gpu->begin();
        for(uint32_t plane=1;plane<=2;++plane){
            auto* resource=plane==1?motion.Get():depth.Get();
            const auto format=plane==1?DXGI_FORMAT_R16G16_FLOAT:DXGI_FORMAT_R32_FLOAT;
            barrier(gpu->commands.Get(),resource,D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE,D3D12_RESOURCE_STATE_COPY_DEST);
            upload->copy(gpu->commands.Get(),resource,format,plane);
            barrier(gpu->commands.Get(),resource,D3D12_RESOURCE_STATE_COPY_DEST,D3D12_RESOURCE_STATE_COPY_SOURCE);
            D3D12_TEXTURE_COPY_LOCATION src{},dst{};
            src.pResource=resource;src.Type=D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX;
            dst.pResource=readback.Get();dst.Type=D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT;
            dst.PlacedFootprint.Offset=upload->planeStride*(plane-1);
            dst.PlacedFootprint.Footprint={format,upload->width,upload->height,1,upload->pitch};
            gpu->commands->CopyTextureRegion(&dst,0,0,0,&src,nullptr);
            barrier(gpu->commands.Get(),resource,D3D12_RESOURCE_STATE_COPY_SOURCE,D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE);
        }
        gpu->submit();gpu->wait();
        uint8_t* bytes=nullptr;D3D12_RANGE range{0,static_cast<SIZE_T>(desc.Width)};
        hr(readback->Map(0,&range,reinterpret_cast<void**>(&bytes)),"guide readback map");
        uint32_t mismatch=0;
        for(uint32_t y=0;y<upload->height;++y)for(uint32_t x=0;x<upload->width;++x){
            const auto offset=static_cast<size_t>(y)*upload->pitch+x*4;
            uint32_t mv=0;float z=0;std::memcpy(&mv,bytes+offset,4);std::memcpy(&z,bytes+upload->planeStride+offset,4);
            if(mv!=0||z!=.5f)++mismatch;
        }
        D3D12_RANGE written{0,0};readback->Unmap(0,&written);
        emit("game_guides_readback","\"width\":"+std::to_string(upload->width)+",\"height\":"+std::to_string(upload->height)+",\"plane_stride\":"+std::to_string(upload->planeStride)+",\"mismatched_pixels\":"+std::to_string(mismatch));
        if(mismatch)throw std::runtime_error("Game zero-motion/fixed-depth GPU readback mismatch");
    }
    void present(uint64_t ready,uint64_t released,bool enabled,uint32_t* count,int32_t* result) {
        auto& slot=work[frame%work.size()];
        // Only reuse an allocator after its own submission completes. Do not
        // serialize every game frame behind all of XeSS's queued FG work.
        if(slot.completion&&gpu->fence->GetCompletedValue()<slot.completion){
            hr(gpu->fence->SetEventOnCompletion(slot.completion,gpu->event),"game frame retirement event");
            if(WaitForSingleObject(gpu->event,30000)!=WAIT_OBJECT_0)throw std::runtime_error("Game frame retirement timeout");
        }
        if(suspended){
            xell_sleep_params_t sleep{};sleep.bLowLatencyMode=1;
            checked(api.xellSetSleepMode(context->ll,&sleep),"game XeLL resume");
            suspended=false;resetHistory=true;previous=Clock::now();
        }
        if(on!=enabled){checked(api.xefgSwapChainSetEnabled(context->fg,enabled?1:0),"game FG switch");on=enabled;}
        checked(api.xellSleep(context->ll,frame),"game XeLL sleep");
        checked(api.xellAddMarkerData(context->ll,frame,XELL_SIMULATION_START),"game simulation start");
        checked(api.xellAddMarkerData(context->ll,frame,XELL_SIMULATION_END),"game simulation end");
        checked(api.xellAddMarkerData(context->ll,frame,XELL_RENDERSUBMIT_START),"game submit start");
        hr(slot.allocator->Reset(),"game frame allocator reset");
        hr(slot.commands->Reset(slot.allocator.Get(),nullptr),"game frame command reset");
        auto* commands=slot.commands.Get();
        hr(gpu->queue->Wait(sharedFence.Get(),ready),"game wait Vulkan input");
        ComPtr<ID3D12Resource> back;
        hr(swap->GetBuffer(swap->GetCurrentBackBufferIndex(),IID_PPV_ARGS(&back)),"game backbuffer");
        barrier(commands,color.Get(),D3D12_RESOURCE_STATE_COMMON,D3D12_RESOURCE_STATE_COPY_SOURCE);
        barrier(commands,back.Get(),D3D12_RESOURCE_STATE_PRESENT,D3D12_RESOURCE_STATE_COPY_DEST);
        commands->CopyResource(back.Get(),color.Get());
        barrier(commands,back.Get(),D3D12_RESOURCE_STATE_COPY_DEST,D3D12_RESOURCE_STATE_PRESENT);
        barrier(commands,color.Get(),D3D12_RESOURCE_STATE_COPY_SOURCE,D3D12_RESOURCE_STATE_COMMON);
        const uint32_t mw=upload->width,mh=upload->height;
        for(uint32_t plane=1;plane<=2;++plane){
            auto* resource=plane==1?motion.Get():depth.Get();
            // verifyGuides uploaded and checked these immutable zero-motion /
            // fixed-depth textures. Retag them without uploading them each frame.
            xefg_swapchain_d3d12_resource_data_t tag{};tag.type=plane==1?XEFG_SWAPCHAIN_RES_MOTION_VECTOR:XEFG_SWAPCHAIN_RES_DEPTH;
            tag.validity=XEFG_SWAPCHAIN_RV_ONLY_NOW;tag.resourceSize={mw,mh};tag.pResource=resource;tag.incomingState=D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE;
            checked(api.xefgSwapChainD3D12TagFrameResource(context->fg,commands,frame,&tag),"game guide tag");
        }
        const auto now=Clock::now();
        xefg_swapchain_frame_constant_data_t constants{};
        for(int k=0;k<4;++k){constants.viewMatrix[k*5]=1;constants.projectionMatrix[k*5]=1;}
        constants.motionVectorScaleX=1;constants.motionVectorScaleY=1;constants.resetHistory=resetHistory;
        constants.frameRenderTime=static_cast<float>(std::chrono::duration<double,std::milli>(now-previous).count());previous=now;
        checked(api.xefgSwapChainTagFrameConstants(context->fg,frame,&constants),"game constants");
        hr(commands->Close(),"game frame close");
        ID3D12CommandList* lists[]={commands};gpu->queue->ExecuteCommandLists(1,lists);
        hr(gpu->queue->Signal(sharedFence.Get(),released),"game return Vulkan resource");
        checked(api.xellAddMarkerData(context->ll,frame,XELL_RENDERSUBMIT_END),"game submit end");
        checked(api.xellAddMarkerData(context->ll,frame,XELL_PRESENT_START),"game present start");
        checked(api.xefgSwapChainSetPresentId(context->fg,frame),"game present id");
        hr(swap->Present(0,DXGI_PRESENT_ALLOW_TEARING),"game FG present");
        checked(api.xellAddMarkerData(context->ll,frame,XELL_PRESENT_END),"game present end");
        xefg_swapchain_present_status_t status{};
        const auto query=api.xefgSwapChainGetLastPresentStatus(context->fg,&status);
        *count=status.framesPresented;*result=query?query:status.frameGenResult;
        slot.completion=++gpu->value;
        hr(gpu->queue->Signal(gpu->fence.Get(),slot.completion),"game frame retirement signal");
        ++frame;resetHistory=false;
    }
    void suspend() {
        if(suspended)return;
        gpu->wait();
        checked(api.xefgSwapChainSetEnabled(context->fg,0),"game FG suspend");on=false;
        xell_sleep_params_t sleep{};
        checked(api.xellSetSleepMode(context->ll,&sleep),"game XeLL suspend");
        gpu->wait();suspended=true;resetHistory=true;
    }
    ~GameRuntime(){
        try {
            if(gpu)gpu->wait();
            swap.Reset();
            if(context)context->close();
        }catch(const std::exception& error){emit("fatal","\"stage\":\"game teardown\",\"message\":"+quote(error.what()));std::_Exit(32);}
    }
};
extern "C" int xess_fg_game_create(const wchar_t* sdk,const wchar_t* agility,void* window,const uint8_t* luid,uint32_t width,uint32_t height,Callback callback,void** out,intptr_t* color,intptr_t* fence){
    output=callback;
    try{auto runtime=std::make_unique<GameRuntime>(sdk,agility,static_cast<HWND>(window),luid,width,height);*color=reinterpret_cast<intptr_t>(runtime->colorHandle.value);*fence=reinterpret_cast<intptr_t>(runtime->fenceHandle.value);*out=runtime.release();return 0;}
    catch(const std::exception& e){emit("game_runtime_failure","\"message\":"+quote(e.what()));return 1;}
}
extern "C" int xess_fg_game_present(void* runtime,uint64_t ready,uint64_t released,uint32_t enabled,uint32_t* frames,int32_t* result){
    try{static_cast<GameRuntime*>(runtime)->present(ready,released,enabled!=0,frames,result);return 0;}
    catch(const std::exception& e){emit("game_runtime_failure","\"message\":"+quote(e.what()));return 1;}
}
extern "C" void xess_fg_game_destroy(void* runtime){delete static_cast<GameRuntime*>(runtime);}
extern "C" int xess_fg_game_wait(void* runtime){
    try{static_cast<GameRuntime*>(runtime)->gpu->wait();return 0;}
    catch(const std::exception& e){emit("game_runtime_failure","\"message\":"+quote(e.what()));return 1;}
}
extern "C" int xess_fg_game_suspend(void* runtime){
    try{static_cast<GameRuntime*>(runtime)->suspend();return 0;}
    catch(const std::exception& e){emit("game_runtime_failure","\"message\":"+quote(e.what()));return 1;}
}
struct OverlayWindow {
    HWND owner=nullptr,window=nullptr;
    HANDLE ready=nullptr,thread=nullptr;
    DWORD threadId=0;
    LONG width=0,height=0;
    bool visible=false;
};
static DWORD WINAPI overlay_thread(void* parameter){
    auto* state=static_cast<OverlayWindow*>(parameter);
    WNDCLASSW wc{};wc.lpfnWndProc=DefWindowProcW;wc.hInstance=GetModuleHandleW(nullptr);wc.lpszClassName=L"NsEmuFgOverlay";
    const bool registered=RegisterClassW(&wc)||GetLastError()==ERROR_CLASS_ALREADY_EXISTS;
    RECT rect{};
    if(registered&&GetClientRect(state->owner,&rect))
        state->window=CreateWindowExW(WS_EX_TRANSPARENT|WS_EX_NOACTIVATE|WS_EX_NOPARENTNOTIFY,wc.lpszClassName,L"",
            WS_CHILD|WS_CLIPSIBLINGS|WS_DISABLED,0,0,std::max(1L,rect.right),std::max(1L,rect.bottom),
            state->owner,nullptr,wc.hInstance,nullptr);
    SetEvent(state->ready);
    if(!state->window)return 1;
    // The emulator UI can synchronously message its children while joining
    // the render thread. A dedicated pump must keep responding throughout.
    MSG message{};
    while(GetMessageW(&message,nullptr,0,0)>0){TranslateMessage(&message);DispatchMessageW(&message);}
    if(IsWindow(state->window))DestroyWindow(state->window);
    return 0;
}
extern "C" void* xess_fg_overlay_window(void* owner,intptr_t* window){
    auto state=std::make_unique<OverlayWindow>();state->owner=static_cast<HWND>(owner);
    state->ready=CreateEventW(nullptr,TRUE,FALSE,nullptr);
    if(!state->ready)return nullptr;
    state->thread=CreateThread(nullptr,0,overlay_thread,state.get(),0,&state->threadId);
    if(!state->thread){CloseHandle(state->ready);return nullptr;}
    WaitForSingleObject(state->ready,INFINITE);CloseHandle(state->ready);state->ready=nullptr;
    if(!state->window){WaitForSingleObject(state->thread,INFINITE);CloseHandle(state->thread);return nullptr;}
    *window=reinterpret_cast<intptr_t>(state->window);
    return state.release();
}
extern "C" void xess_fg_overlay_visible(void* overlay,void* owner,uint32_t visible){
    auto* state=static_cast<OverlayWindow*>(overlay);
    const auto window=state->window;
    RECT rect{};
    if(visible&&GetClientRect(static_cast<HWND>(owner),&rect)){
        const LONG width=std::max(1L,rect.right),height=std::max(1L,rect.bottom);
        if(state->visible&&state->width==width&&state->height==height)return;
        if(SetWindowPos(window,HWND_TOP,0,0,width,height,SWP_NOACTIVATE|SWP_SHOWWINDOW|SWP_ASYNCWINDOWPOS)){
            state->visible=true;state->width=width;state->height=height;
        }
    }else if(state->visible&&ShowWindowAsync(window,SW_HIDE))state->visible=false;
}
extern "C" void xess_fg_overlay_destroy(void* overlay){
    auto* state=static_cast<OverlayWindow*>(overlay);
    PostThreadMessageW(state->threadId,WM_QUIT,0,0);
    WaitForSingleObject(state->thread,INFINITE);CloseHandle(state->thread);delete state;
}
extern "C" void* xess_fg_shadow_window(void* owner){
    // Use resident USER32 code even if the creating render thread owns a shadow
    // window past Vulkan teardown; no callback may point into an unloaded layer.
    WNDCLASSW wc{};wc.lpfnWndProc=DefWindowProcW;wc.hInstance=GetModuleHandleW(nullptr);wc.lpszClassName=L"NsEmuXeSSFGShadow";
    if(!RegisterClassW(&wc)&&GetLastError()!=ERROR_CLASS_ALREADY_EXISTS)return nullptr;
    RECT rect{};if(!GetClientRect(static_cast<HWND>(owner),&rect))return nullptr;
    return CreateWindowExW(0,wc.lpszClassName,L"",WS_POPUP,0,0,std::max(1L,rect.right),std::max(1L,rect.bottom),nullptr,nullptr,wc.hInstance,nullptr);
}
extern "C" void xess_fg_shadow_sync(void* shadow,void* owner){
    RECT rect{};if(GetClientRect(static_cast<HWND>(owner),&rect))SetWindowPos(static_cast<HWND>(shadow),nullptr,0,0,std::max(1L,rect.right),std::max(1L,rect.bottom),SWP_NOACTIVATE|SWP_NOZORDER|SWP_NOMOVE);
}
extern "C" void xess_fg_shadow_destroy(void* shadow){DestroyWindow(static_cast<HWND>(shadow));}
