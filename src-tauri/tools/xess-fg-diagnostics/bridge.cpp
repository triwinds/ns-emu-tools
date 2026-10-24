#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <windows.h>
#include <d3d12.h>
#include <d3d12sdklayers.h>
#include <dxgi1_6.h>
#include <wrl/client.h>
#include <xess_fg/xefg_swapchain_d3d12.h>
#include <xell/xell_d3d12.h>
#include <algorithm>
#include <chrono>
#include <cmath>
#include <cstdint>
#include <cstring>
#include <memory>
#include <sstream>
#include <stdexcept>
#include <string>
#include <thread>
#include <vector>
using Microsoft::WRL::ComPtr;
using Clock = std::chrono::steady_clock;
using Callback = void (*)(const char*);
static Callback output;
static std::string quote(const std::string& value) {
    std::string s = "\"";
    for (unsigned char c : value) {
        if (c == '"' || c == '\\') { s += '\\'; s += static_cast<char>(c); }
        else if (c == '\n') s += "\\n";
        else if (c == '\r') s += "\\r";
        else if (c >= 32) s += static_cast<char>(c);
    }
    return s + '"';
}
static void emit(const std::string& event, const std::string& fields) {
    const std::string line = "{\"event\":" + quote(event) + "," + fields + "}";
    output(line.c_str());
}
static void hr(HRESULT code, const char* stage) {
    if (FAILED(code)) throw std::runtime_error(std::string(stage) + ": HRESULT " + std::to_string(static_cast<uint32_t>(code)));
}
static void checked(int code, const char* stage) {
    if (code != 0) throw std::runtime_error(std::string(stage) + ": SDK " + std::to_string(code));
}
struct Module {
    HMODULE handle = nullptr;
    explicit Module(const std::wstring& path) {
        handle = LoadLibraryExW(path.c_str(), nullptr, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32);
        if (!handle) throw std::runtime_error("Cannot load pinned SDK module: " + std::to_string(GetLastError()));
    }
    ~Module() { if (handle) FreeLibrary(handle); }
    template<class T> T get(const char* name) {
        auto p = GetProcAddress(handle, name);
        if (!p) throw std::runtime_error(std::string("Missing export ") + name);
        return reinterpret_cast<T>(p);
    }
};
#define FG_FUNCTIONS(X) \
    X(xefgSwapChainGetVersion) X(xefgSwapChainD3D12CreateContext) \
    X(xefgSwapChainGetProperties) X(xefgSwapChainD3D12GetProperties) \
    X(xefgSwapChainSetLoggingCallback) X(xefgSwapChainSetLatencyReduction) \
    X(xefgSwapChainD3D12InitFromSwapChainDesc) X(xefgSwapChainD3D12GetSwapChainPtr) \
    X(xefgSwapChainSetEnabled) X(xefgSwapChainD3D12TagFrameResource) \
    X(xefgSwapChainTagFrameConstants) X(xefgSwapChainSetPresentId) \
    X(xefgSwapChainGetLastPresentStatus) X(xefgSwapChainDestroy)
#define LL_FUNCTIONS(X) X(xellD3D12CreateContext) X(xellGetVersion) X(xellSetLoggingCallback) \
    X(xellSetSleepMode) X(xellSleep) X(xellAddMarkerData) X(xellDestroyContext)
struct Api {
#define FIELD(name) decltype(&name) name = nullptr;
    FG_FUNCTIONS(FIELD)
    LL_FUNCTIONS(FIELD)
#undef FIELD
    Api(Module& fg, Module& ll) {
#define LOAD_FG(name) name = fg.get<decltype(name)>(#name);
        FG_FUNCTIONS(LOAD_FG)
#undef LOAD_FG
#define LOAD_LL(name) name = ll.get<decltype(name)>(#name);
        LL_FUNCTIONS(LOAD_LL)
#undef LOAD_LL
    }
};
static void fg_log(const char* message, xefg_swapchain_logging_level_t level, void*) {
    emit("fg_log", "\"level\":"+std::to_string(level)+",\"message\":"+quote(message));
}
static void ll_log(const char* message, xell_logging_level_t level) {
    emit("xell_log", "\"level\":"+std::to_string(level)+",\"message\":"+quote(message));
}
struct Context {
    Api& api;
    xefg_swapchain_handle_t fg = nullptr;
    xell_context_handle_t ll = nullptr;
    explicit Context(Api& a) : api(a) {}
    void close() {
        if (fg) { checked(api.xefgSwapChainDestroy(fg), "destroy FG"); fg = nullptr; }
        if (ll) { checked(api.xellDestroyContext(ll), "destroy XeLL"); ll = nullptr; }
    }
    ~Context() {
        if (fg) api.xefgSwapChainDestroy(fg);
        if (ll) api.xellDestroyContext(ll);
    }
};
struct Window {
    HWND handle = nullptr;
    static LRESULT CALLBACK proc(HWND w, UINT message, WPARAM a, LPARAM b) {
        return DefWindowProcW(w, message, a, b);
    }
    Window(uint32_t width, uint32_t height) {
        WNDCLASSW wc{};
        wc.lpfnWndProc = proc;
        wc.hInstance = GetModuleHandleW(nullptr);
        wc.lpszClassName = L"NsEmuXeSSFGDiagnostics";
        if (!RegisterClassW(&wc)) throw std::runtime_error("RegisterClass failed");
        RECT rect{0,0,static_cast<LONG>(width),static_cast<LONG>(height)};
        const DWORD style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;
        AdjustWindowRect(&rect,style,FALSE);
        handle = CreateWindowExW(0,wc.lpszClassName,L"XeSS FG independent D3D12 test",style,
            CW_USEDEFAULT,CW_USEDEFAULT,rect.right-rect.left,rect.bottom-rect.top,
            nullptr,nullptr,wc.hInstance,nullptr);
        if (!handle) throw std::runtime_error("CreateWindow failed");
        ShowWindow(handle,SW_SHOWNORMAL);
    }
    ~Window() {
        if (IsWindow(handle)) DestroyWindow(handle);
        UnregisterClassW(L"NsEmuXeSSFGDiagnostics",GetModuleHandleW(nullptr));
    }
};
struct Gpu {
    ComPtr<ID3D12Device> device;
    ComPtr<ID3D12CommandQueue> queue;
    ComPtr<ID3D12CommandAllocator> allocator;
    ComPtr<ID3D12GraphicsCommandList> commands;
    ComPtr<ID3D12Fence> fence;
    HANDLE event = nullptr;
    uint64_t value = 0;
    Gpu(ID3D12DeviceFactory* factory, IDXGIAdapter1* adapter) {
        hr(factory->CreateDevice(adapter,D3D_FEATURE_LEVEL_12_0,IID_PPV_ARGS(&device)),"D3D12 device");
        D3D12_COMMAND_QUEUE_DESC q{};
        q.Type = D3D12_COMMAND_LIST_TYPE_DIRECT;
        hr(device->CreateCommandQueue(&q,IID_PPV_ARGS(&queue)),"direct queue");
        hr(device->CreateCommandAllocator(q.Type,IID_PPV_ARGS(&allocator)),"allocator");
        hr(device->CreateCommandList(0,q.Type,allocator.Get(),nullptr,IID_PPV_ARGS(&commands)),"command list");
        hr(commands->Close(),"initial command close");
        hr(device->CreateFence(0,D3D12_FENCE_FLAG_NONE,IID_PPV_ARGS(&fence)),"fence");
        event = CreateEventW(nullptr,FALSE,FALSE,nullptr);
        if (!event) throw std::runtime_error("Fence event creation failed");
    }
    void wait() {
        hr(queue->Signal(fence.Get(),++value),"fence signal");
        if (fence->GetCompletedValue()<value) {
            hr(fence->SetEventOnCompletion(value,event),"fence event");
            if (WaitForSingleObject(event,30000)!=WAIT_OBJECT_0) {
                emit("fatal", "\"stage\":\"GPU completion timeout; process terminates without resource teardown\"");
                std::_Exit(30);
            }
        }
        hr(device->GetDeviceRemovedReason(),"device status");
    }
    void begin() {
        hr(allocator->Reset(),"allocator reset");
        hr(commands->Reset(allocator.Get(),nullptr),"command reset");
    }
    void submit() {
        hr(commands->Close(),"command close");
        ID3D12CommandList* lists[] = {commands.Get()};
        queue->ExecuteCommandLists(1,lists);
    }
    ~Gpu() { if(event) CloseHandle(event); }
};
static void barrier(ID3D12GraphicsCommandList* cmd, ID3D12Resource* resource,
        D3D12_RESOURCE_STATES before, D3D12_RESOURCE_STATES after) {
    D3D12_RESOURCE_BARRIER b{};
    b.Type = D3D12_RESOURCE_BARRIER_TYPE_TRANSITION;
    b.Transition = {resource,D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES,before,after};
    cmd->ResourceBarrier(1,&b);
}
static ComPtr<ID3D12Resource> texture(Gpu& gpu, uint32_t w, uint32_t h, DXGI_FORMAT format) {
    D3D12_HEAP_PROPERTIES heap{};
    heap.Type = D3D12_HEAP_TYPE_DEFAULT;
    D3D12_RESOURCE_DESC desc{};
    desc.Dimension=D3D12_RESOURCE_DIMENSION_TEXTURE2D;
    desc.Width=w;desc.Height=h;desc.DepthOrArraySize=1;desc.MipLevels=1;
    desc.Format=format;desc.SampleDesc.Count=1;
    ComPtr<ID3D12Resource> result;
    hr(gpu.device->CreateCommittedResource(&heap,D3D12_HEAP_FLAG_NONE,&desc,
        D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE,nullptr,IID_PPV_ARGS(&result)),"guide texture");
    return result;
}
struct Upload {
    ComPtr<ID3D12Resource> resource;
    uint8_t* data = nullptr;
    uint32_t pitch, width, height;
    uint64_t planeStride;
    Upload(Gpu& gpu,uint32_t w,uint32_t h) : pitch((w*4+255)&~255u),width(w),height(h),
        planeStride((static_cast<uint64_t>(pitch)*h+511)&~511ull) {
        D3D12_HEAP_PROPERTIES heap{};heap.Type=D3D12_HEAP_TYPE_UPLOAD;
        D3D12_RESOURCE_DESC desc{};desc.Dimension=D3D12_RESOURCE_DIMENSION_BUFFER;
        desc.Width=planeStride*3;desc.Height=1;desc.DepthOrArraySize=1;
        desc.MipLevels=1;desc.SampleDesc.Count=1;desc.Layout=D3D12_TEXTURE_LAYOUT_ROW_MAJOR;
        hr(gpu.device->CreateCommittedResource(&heap,D3D12_HEAP_FLAG_NONE,&desc,
            D3D12_RESOURCE_STATE_GENERIC_READ,nullptr,IID_PPV_ARGS(&resource)),"upload buffer");
        D3D12_RANGE range{0,0};
        hr(resource->Map(0,&range,reinterpret_cast<void**>(&data)),"upload map");
    }
    void render(uint32_t frame) {
        for(uint32_t y=0;y<height;++y) for(uint32_t x=0;x<width;++x) {
            const auto at=static_cast<size_t>(y)*pitch+x*4;
            const auto u=(x+width-frame%width)%width;
            const auto c=((u/12+y/12)&1)?190:35;
            data[at]=static_cast<uint8_t>(c);
            data[at+1]=static_cast<uint8_t>(u%80<3?240:c);
            data[at+2]=static_cast<uint8_t>(y%53<2?230:c);
            data[at+3]=255;
            const uint16_t motion[]={0xbc00,0}; // -1,0 in IEEE FP16 pixels.
            std::memcpy(data+planeStride+at,motion,4);
            const float depth=.5f;
            std::memcpy(data+planeStride*2+at,&depth,4);
        }
    }
    void copy(ID3D12GraphicsCommandList* cmd,ID3D12Resource* destination,DXGI_FORMAT format,uint32_t plane) {
        D3D12_TEXTURE_COPY_LOCATION src{},dst{};
        src.pResource=resource.Get();src.Type=D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT;
        src.PlacedFootprint.Offset=planeStride*plane;
        src.PlacedFootprint.Footprint={format,width,height,1,pitch};
        dst.pResource=destination;dst.Type=D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX;
        cmd->CopyTextureRegion(&dst,0,0,0,&src,nullptr);
    }
    ~Upload() { if(data) resource->Unmap(0,nullptr); }
};
#ifndef XESS_FG_GAME
extern "C" int xess_vk_init(const uint8_t*,intptr_t,intptr_t,uint32_t,uint32_t);
extern "C" int xess_vk_render(uint32_t);
extern "C" uint32_t xess_vk_finish();
extern "C" int xess_fg_readback(uint32_t,uint32_t,uint32_t,uint32_t,const uint8_t*);
struct Readback {
    ComPtr<ID3D12Resource> buffer;
    uint32_t pitch,width,height;
    Readback(Gpu& gpu,uint32_t w,uint32_t h):pitch((w*4+255)&~255u),width(w),height(h) {
        D3D12_HEAP_PROPERTIES heap{};heap.Type=D3D12_HEAP_TYPE_READBACK;
        D3D12_RESOURCE_DESC desc{};desc.Dimension=D3D12_RESOURCE_DIMENSION_BUFFER;
        desc.Width=static_cast<uint64_t>(pitch)*height;desc.Height=1;desc.DepthOrArraySize=1;
        desc.MipLevels=1;desc.SampleDesc.Count=1;desc.Layout=D3D12_TEXTURE_LAYOUT_ROW_MAJOR;
        hr(gpu.device->CreateCommittedResource(&heap,D3D12_HEAP_FLAG_NONE,&desc,D3D12_RESOURCE_STATE_COPY_DEST,nullptr,IID_PPV_ARGS(&buffer)),"readback buffer");
    }
    void record(ID3D12GraphicsCommandList* commands,ID3D12Resource* source) {
        D3D12_TEXTURE_COPY_LOCATION src{},dst{};
        src.pResource=source;src.Type=D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX;
        dst.pResource=buffer.Get();dst.Type=D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT;
        dst.PlacedFootprint.Footprint={DXGI_FORMAT_R8G8B8A8_UNORM,width,height,1,pitch};
        commands->CopyTextureRegion(&dst,0,0,0,&src,nullptr);
    }
    void verify(uint32_t frame) {
        uint8_t* data=nullptr;
        D3D12_RANGE range{0,static_cast<SIZE_T>(pitch)*height};
        hr(buffer->Map(0,&range,reinterpret_cast<void**>(&data)),"readback map");
        uint32_t mismatches=0;
        for(uint32_t y=0;y<height;++y)for(uint32_t x=0;x<width;++x) {
            const auto u=(x+width-frame%width)%width;
            const auto c=((u/12+y/12)&1)?190:35;
            const uint8_t expected[]={static_cast<uint8_t>(c),static_cast<uint8_t>(u%80<3?240:c),static_cast<uint8_t>(y%53<2?230:c),255};
            if(std::memcmp(data+static_cast<size_t>(pitch)*y+x*4,expected,4)!=0)++mismatches;
        }
        const auto saved=xess_fg_readback(frame,width,height,pitch,data);
        const D3D12_RANGE written{0,0};buffer->Unmap(0,&written);
        emit("vulkan_color_readback","\"frame\":"+std::to_string(frame)+",\"pixels\":"+std::to_string(width*height)+",\"mismatched_pixels\":"+std::to_string(mismatches));
        if(mismatches||saved)throw std::runtime_error("Vulkan/D3D12 color round-trip mismatch or artifact save failure");
    }
};
struct SharedColor {
    struct Handle {
        HANDLE value=nullptr;
        ~Handle(){if(value)CloseHandle(value);}
    } textureHandle,fenceHandle;
    ComPtr<ID3D12Resource> color;
    ComPtr<ID3D12Fence> fence;
    bool ready=false;
    SharedColor(Gpu& gpu,uint32_t width,uint32_t height) {
        D3D12_HEAP_PROPERTIES heap{};heap.Type=D3D12_HEAP_TYPE_DEFAULT;
        D3D12_RESOURCE_DESC desc{};desc.Dimension=D3D12_RESOURCE_DIMENSION_TEXTURE2D;
        desc.Width=width;desc.Height=height;desc.DepthOrArraySize=1;desc.MipLevels=1;
        desc.Format=DXGI_FORMAT_R8G8B8A8_UNORM;desc.SampleDesc.Count=1;
        desc.Flags=D3D12_RESOURCE_FLAG_ALLOW_SIMULTANEOUS_ACCESS|D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS;
        hr(gpu.device->CreateCommittedResource(&heap,D3D12_HEAP_FLAG_SHARED,&desc,D3D12_RESOURCE_STATE_COMMON,nullptr,IID_PPV_ARGS(&color)),"shared color");
        hr(gpu.device->CreateFence(0,D3D12_FENCE_FLAG_SHARED,IID_PPV_ARGS(&fence)),"shared timeline fence");
        hr(gpu.device->CreateSharedHandle(color.Get(),nullptr,GENERIC_ALL,nullptr,&textureHandle.value),"export color handle");
        hr(gpu.device->CreateSharedHandle(fence.Get(),nullptr,GENERIC_ALL,nullptr,&fenceHandle.value),"export fence handle");
        const auto luid=gpu.device->GetAdapterLuid();
        checked(xess_vk_init(reinterpret_cast<const uint8_t*>(&luid),reinterpret_cast<intptr_t>(textureHandle.value),reinterpret_cast<intptr_t>(fenceHandle.value),width,height),"Vulkan shared producer init");
        ready=true;
    }
    uint32_t finish(){if(!ready)return 0;ready=false;return xess_vk_finish();}
    ~SharedColor(){finish();}
};
static uint32_t debug_messages(ID3D12Device* device) {
    ComPtr<ID3D12InfoQueue> info;
    if (FAILED(device->QueryInterface(IID_PPV_ARGS(&info)))) return 0;
    uint32_t bad=0;
    for(uint64_t i=0;i<info->GetNumStoredMessagesAllowedByRetrievalFilter();++i) {
        SIZE_T size=0;hr(info->GetMessage(i,nullptr,&size),"debug message size");
        std::vector<uint8_t> bytes(size);
        auto* m=reinterpret_cast<D3D12_MESSAGE*>(bytes.data());
        hr(info->GetMessage(i,m,&size),"debug message");
        if(m->Severity<=D3D12_MESSAGE_SEVERITY_WARNING) {
            ++bad;
            emit("d3d12_message","\"severity\":"+std::to_string(m->Severity)+",\"id\":"+std::to_string(m->ID)+",\"message\":"+quote(m->pDescription));
        }
    }
    return bad;
}
extern "C" int xess_fg_run(const wchar_t* sdk,uint32_t adapterIndex,uint32_t frames,uint32_t width,
        uint32_t height,uint32_t validation,uint32_t capabilities,uint32_t interop,uint32_t sourceFps,Callback callback) {
    output=callback;
    try {
        ComPtr<ID3D12SDKConfiguration1> configuration;
        hr(D3D12GetInterface(CLSID_D3D12SDKConfiguration,IID_PPV_ARGS(&configuration)),"Agility configuration");
        ComPtr<ID3D12DeviceFactory> deviceFactory;
        wchar_t binaryPath[32768]{};
        if(!GetModuleFileNameW(nullptr,binaryPath,32768)) throw std::runtime_error("Cannot resolve diagnostic executable");
        std::wstring agilityPath(binaryPath);
        agilityPath=agilityPath.substr(0,agilityPath.find_last_of(L"\\/")+1)+L"D3D12\\";
        char agilityUtf8[65536]{};
        if(!WideCharToMultiByte(CP_UTF8,0,agilityPath.c_str(),-1,agilityUtf8,sizeof(agilityUtf8),nullptr,nullptr)) throw std::runtime_error("Agility path encoding failed");
        hr(configuration->CreateDeviceFactory(619,agilityUtf8,IID_PPV_ARGS(&deviceFactory)),"Agility device factory");
        Module ll(std::wstring(sdk)+L"/bin/libxell.dll");
        Module fg(std::wstring(sdk)+L"/bin/libxess_fg.dll");
        Api api(fg,ll);
        xefg_swapchain_version_t version{};xell_version_t llVersion{};
        checked(api.xefgSwapChainGetVersion(&version),"FG version");
        checked(api.xellGetVersion(&llVersion),"XeLL version");
        emit("versions","\"fg\":"+quote(std::to_string(version.major)+"."+std::to_string(version.minor)+"."+std::to_string(version.patch))+",\"xell\":"+quote(std::to_string(llVersion.major)+"."+std::to_string(llVersion.minor)+"."+std::to_string(llVersion.patch)));
        if(validation) {
            ComPtr<ID3D12Debug> debug;
            hr(deviceFactory->GetConfigurationInterface(CLSID_D3D12Debug,IID_PPV_ARGS(&debug)),"D3D12 debug layer unavailable");
            debug->EnableDebugLayer();
        }
        ComPtr<IDXGIFactory6> factory;
        hr(CreateDXGIFactory2(0,IID_PPV_ARGS(&factory)),"DXGI factory");
        ComPtr<IDXGIAdapter1> selected;
        for(uint32_t i=0;;++i) {
            ComPtr<IDXGIAdapter1> adapter;
            if(factory->EnumAdapters1(i,&adapter)==DXGI_ERROR_NOT_FOUND) break;
            DXGI_ADAPTER_DESC1 desc{};hr(adapter->GetDesc1(&desc),"adapter description");
            if(desc.Flags&DXGI_ADAPTER_FLAG_SOFTWARE) continue;
            char name[256]{};
            WideCharToMultiByte(CP_UTF8,0,desc.Description,-1,name,sizeof(name),nullptr,nullptr);
            ComPtr<ID3D12Device> device;
            auto result=deviceFactory->CreateDevice(adapter.Get(),D3D_FEATURE_LEVEL_12_0,IID_PPV_ARGS(&device));
            int supported=-10000;
            xefg_swapchain_properties_t props{};
            D3D12_FEATURE_DATA_SHADER_MODEL model{D3D_SHADER_MODEL_6_4};
            if(SUCCEEDED(result)) {
                device->CheckFeatureSupport(D3D12_FEATURE_SHADER_MODEL,&model,sizeof(model));
                Context probe(api);
                supported=api.xefgSwapChainD3D12CreateContext(device.Get(),&probe.fg);
                if(supported==0) checked(api.xefgSwapChainGetProperties(probe.fg,&props),"capability properties");
                probe.close();
            }
            emit("adapter","\"index\":"+std::to_string(i)+",\"name\":"+quote(name)+",\"vendor\":"+std::to_string(desc.VendorId)+",\"device\":"+std::to_string(desc.DeviceId)+",\"shader_model\":"+std::to_string(model.HighestShaderModel)+",\"fg_support_result\":"+std::to_string(supported)+",\"max_interpolations\":"+std::to_string(props.maxSupportedInterpolations));
            if(supported==0 && ((adapterIndex==UINT32_MAX&&desc.VendorId!=0x10de&&!selected)||(adapterIndex==i))) selected=adapter;
        }
        if(capabilities) return 0;
        if(!selected) throw std::runtime_error("No supported requested non-NVIDIA adapter; choose explicit --adapter only when intended");
        Gpu gpu(deviceFactory.Get(),selected.Get());
        Context context(api);
        checked(api.xefgSwapChainD3D12CreateContext(gpu.device.Get(),&context.fg),"FG create");
        checked(api.xefgSwapChainSetLoggingCallback(context.fg,XEFG_SWAPCHAIN_LOGGING_LEVEL_INFO,fg_log,nullptr),"FG log");
        checked(api.xellD3D12CreateContext(gpu.device.Get(),&context.ll),"XeLL create");
        checked(api.xellSetLoggingCallback(context.ll,XELL_LOGGING_LEVEL_INFO,ll_log),"XeLL log");
        checked(api.xefgSwapChainSetLatencyReduction(context.fg,context.ll),"FG/XeLL attach");
        Window window(width,height);
        DXGI_SWAP_CHAIN_DESC1 desc{};
        desc.Width=width;desc.Height=height;desc.Format=DXGI_FORMAT_R8G8B8A8_UNORM;
        desc.SampleDesc.Count=1;desc.BufferUsage=DXGI_USAGE_RENDER_TARGET_OUTPUT;
        desc.BufferCount=3;desc.SwapEffect=DXGI_SWAP_EFFECT_FLIP_DISCARD;
        desc.Flags=DXGI_SWAP_CHAIN_FLAG_ALLOW_TEARING;
        xefg_swapchain_d3d12_init_params_t init{};
        init.initFlags=XEFG_SWAPCHAIN_INIT_FLAG_HIGH_RES_MV;
        init.maxInterpolatedFrames=1;init.uiMode=XEFG_SWAPCHAIN_UI_MODE_NONE;
        checked(api.xefgSwapChainD3D12InitFromSwapChainDesc(context.fg,window.handle,&desc,nullptr,gpu.queue.Get(),factory.Get(),&init),"FG swapchain init");
        ComPtr<IDXGISwapChain4> swap;
        checked(api.xefgSwapChainD3D12GetSwapChainPtr(context.fg,IID_PPV_ARGS(&swap)),"FG proxy");
        xell_sleep_params_t sleep{};sleep.bLowLatencyMode=1;
        checked(api.xellSetSleepMode(context.ll,&sleep),"XeLL enable");
        checked(api.xefgSwapChainSetEnabled(context.fg,1),"FG enable");
        auto motion=texture(gpu,width,height,DXGI_FORMAT_R16G16_FLOAT);
        auto depth=texture(gpu,width,height,DXGI_FORMAT_R32_FLOAT);
        Upload upload(gpu,width,height);
        std::unique_ptr<SharedColor> shared;
        if(interop){shared=std::make_unique<SharedColor>(gpu,width,height);upload.render(0);}
        std::unique_ptr<Readback> readback;
        if(interop)readback=std::make_unique<Readback>(gpu,width,height);
        struct CompletionGuard {
            Gpu& owner;
            ~CompletionGuard() noexcept {
                try { owner.wait(); }
                catch(const std::exception& error) {
                    emit("fatal","\"stage\":\"completion during teardown\",\"message\":"+quote(error.what()));
                    std::_Exit(31);
                }
            }
        } completion{gpu};
        uint64_t queued=0;uint32_t generated=0,failures=0,measured=0;
        const auto started=Clock::now();
        auto measuredStart=started;
        for(uint32_t frame=0;frame<frames;++frame) {
            MSG message{};
            while(PeekMessageW(&message,nullptr,0,0,PM_REMOVE)) { TranslateMessage(&message); DispatchMessageW(&message); }
            if(!IsWindow(window.handle)) throw std::runtime_error("Diagnostic window closed before bounded run completed");
            std::this_thread::sleep_until(started+std::chrono::duration_cast<Clock::duration>(std::chrono::duration<double>(static_cast<double>(frame)/sourceFps)));
            const auto frameStart=Clock::now();
            checked(api.xellSleep(context.ll,frame),"XeLL sleep");
            checked(api.xellAddMarkerData(context.ll,frame,XELL_SIMULATION_START),"simulation start");
            if(shared) checked(xess_vk_render(frame),"Vulkan color production");
            else upload.render(frame);
            checked(api.xellAddMarkerData(context.ll,frame,XELL_SIMULATION_END),"simulation end");
            checked(api.xellAddMarkerData(context.ll,frame,XELL_RENDERSUBMIT_START),"submit start");
            gpu.begin();
            ComPtr<ID3D12Resource> back;
            hr(swap->GetBuffer(swap->GetCurrentBackBufferIndex(),IID_PPV_ARGS(&back)),"proxy backbuffer");
            barrier(gpu.commands.Get(),back.Get(),D3D12_RESOURCE_STATE_PRESENT,D3D12_RESOURCE_STATE_COPY_DEST);
            if(shared) {
                hr(gpu.queue->Wait(shared->fence.Get(),static_cast<uint64_t>(frame)*2+1),"D3D12 wait Vulkan color");
                barrier(gpu.commands.Get(),shared->color.Get(),D3D12_RESOURCE_STATE_COMMON,D3D12_RESOURCE_STATE_COPY_SOURCE);
                gpu.commands->CopyResource(back.Get(),shared->color.Get());
                barrier(gpu.commands.Get(),shared->color.Get(),D3D12_RESOURCE_STATE_COPY_SOURCE,D3D12_RESOURCE_STATE_COMMON);
            } else upload.copy(gpu.commands.Get(),back.Get(),desc.Format,0);
            const bool capture=readback&&(frame==0||frame==16);
            if(capture) {
                barrier(gpu.commands.Get(),back.Get(),D3D12_RESOURCE_STATE_COPY_DEST,D3D12_RESOURCE_STATE_COPY_SOURCE);
                readback->record(gpu.commands.Get(),back.Get());
                barrier(gpu.commands.Get(),back.Get(),D3D12_RESOURCE_STATE_COPY_SOURCE,D3D12_RESOURCE_STATE_PRESENT);
            } else barrier(gpu.commands.Get(),back.Get(),D3D12_RESOURCE_STATE_COPY_DEST,D3D12_RESOURCE_STATE_PRESENT);
            for(uint32_t plane=1;plane<=2;++plane) {
                auto* guide=plane==1?motion.Get():depth.Get();
                const auto format=plane==1?DXGI_FORMAT_R16G16_FLOAT:DXGI_FORMAT_R32_FLOAT;
                barrier(gpu.commands.Get(),guide,D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE,D3D12_RESOURCE_STATE_COPY_DEST);
                upload.copy(gpu.commands.Get(),guide,format,plane);
                barrier(gpu.commands.Get(),guide,D3D12_RESOURCE_STATE_COPY_DEST,D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE);
                xefg_swapchain_d3d12_resource_data_t tag{};
                tag.type=plane==1?XEFG_SWAPCHAIN_RES_MOTION_VECTOR:XEFG_SWAPCHAIN_RES_DEPTH;
                tag.validity=XEFG_SWAPCHAIN_RV_ONLY_NOW;tag.resourceSize={width,height};
                tag.pResource=guide;tag.incomingState=D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE;
                checked(api.xefgSwapChainD3D12TagFrameResource(context.fg,gpu.commands.Get(),frame,&tag),"FG resource tag");
            }
            xefg_swapchain_frame_constant_data_t constants{};
            for(int k=0;k<4;++k) { constants.viewMatrix[k*5]=1;constants.projectionMatrix[k*5]=1; }
            constants.motionVectorScaleX=1;constants.motionVectorScaleY=1;
            constants.resetHistory=frame==0;constants.frameRenderTime=1000.f/sourceFps;
            checked(api.xefgSwapChainTagFrameConstants(context.fg,frame,&constants),"FG constants");
            gpu.submit();
            if(frame==0&&shared)emit("bridge_stage","\"stage\":\"D3D12 color copy submitted\"");
            if(shared) hr(gpu.queue->Signal(shared->fence.Get(),static_cast<uint64_t>(frame)*2+2),"D3D12 release shared color");
            checked(api.xellAddMarkerData(context.ll,frame,XELL_RENDERSUBMIT_END),"submit end");
            checked(api.xellAddMarkerData(context.ll,frame,XELL_PRESENT_START),"present start");
            checked(api.xefgSwapChainSetPresentId(context.fg,frame),"FG present id");
            const auto present=swap->Present(0,DXGI_PRESENT_ALLOW_TEARING);
            if(frame==0&&shared)emit("bridge_stage","\"stage\":\"FG present returned\"");
            hr(present,"proxy present");
            checked(api.xellAddMarkerData(context.ll,frame,XELL_PRESENT_END),"present end");
            xefg_swapchain_present_status_t status{};
            const auto statusResult=api.xefgSwapChainGetLastPresentStatus(context.fg,&status);
            gpu.wait();
            if(capture)readback->verify(frame);
            if(frame==32) measuredStart=frameStart;
            if(frame>=32) {
                ++measured;queued+=status.framesPresented;
                if(statusResult==0&&status.frameGenResult==0&&status.isFrameGenEnabled&&status.framesPresented==2&&present==S_OK) ++generated;
                else ++failures;
            }
            emit("present","\"frame\":"+std::to_string(frame)+",\"status_result\":"+std::to_string(statusResult)+",\"fg_result\":"+std::to_string(status.frameGenResult)+",\"enabled\":"+std::to_string(status.isFrameGenEnabled)+",\"frames_queued\":"+std::to_string(status.framesPresented)+",\"hresult\":"+std::to_string(static_cast<uint32_t>(present)));
        }
        const auto measuredEnd=Clock::now();
        gpu.wait();
        const auto vulkanBad=shared?shared->finish():0;
        checked(api.xefgSwapChainSetEnabled(context.fg,0),"FG disable");
        swap.Reset();
        motion.Reset();depth.Reset();
        context.close();
        const auto seconds=std::chrono::duration<double>(measuredEnd-measuredStart).count();
        const auto bad=validation?debug_messages(gpu.device.Get()):0;
        emit("summary","\"measured_frames\":"+std::to_string(measured)+",\"generated_success_frames\":"+std::to_string(generated)+",\"failures\":"+std::to_string(failures)+",\"frames_queued\":"+std::to_string(queued)+",\"seconds\":"+std::to_string(seconds)+",\"source_fps\":"+std::to_string(measured/seconds)+",\"sdk_queued_fps\":"+std::to_string(queued/seconds)+",\"d3d12_debug_enabled\":"+(validation?"true":"false")+",\"d3d12_warnings_errors\":"+(validation?std::to_string(bad):"null"));
        return measured>0&&generated==measured&&bad==0&&vulkanBad==0?0:2;
    } catch(const std::exception& error) {
        emit("failure","\"message\":"+quote(error.what()));
        return 1;
    }
}
#else
#include "game_runtime.inl"
#endif
