#include "source/core/sl.interposer/vulkan/layerRoute.h"
#include <atomic>
#include <cassert>
#include <cstring>
#include <iostream>
#include <thread>

using namespace sl::layer_route;
std::atomic<unsigned> calls{0};
void VKAPI_CALL sentinel() {}
PFN_vkVoidFunction VKAPI_CALL nextInstance(VkInstance instance, const char* name)
{
    ++calls;
    // A synchronous reentrant lookup would deadlock if the route lock were held.
    assert(instanceProc(instance, "vkGetInstanceProcAddr") ==
        reinterpret_cast<PFN_vkVoidFunction>(instanceProc));
    return (std::strcmp(name, "known") == 0 || std::strcmp(name, "vkGetPhysicalDeviceProperties2") == 0) ? sentinel : nullptr;
}
PFN_vkVoidFunction VKAPI_CALL nextDevice(VkDevice device, const char* name)
{
    ++calls;
    assert(deviceProc(device, "vkGetDeviceProcAddr") ==
        reinterpret_cast<PFN_vkVoidFunction>(deviceProc));
    return std::strcmp(name, "known") == 0 ? sentinel : nullptr;
}

int main()
{
    auto instance = reinterpret_cast<VkInstance>(1);
    auto physical = reinterpret_cast<VkPhysicalDevice>(2);
    auto device = reinterpret_cast<VkDevice>(3);
    SlLayerRouteV1 route{sizeof(SlLayerRouteV1), 1, instance, physical, device, nextInstance, nextDevice};
    assert(bind(nullptr) == sl::Result::eErrorInvalidParameter);
    assert(claim(instance, physical, device) == sl::Result::eErrorInvalidState);
    for (unsigned invalid = 0; invalid < 9; ++invalid)
    {
        auto bad = route;
        switch (invalid)
        {
        case 0: bad.size = 0; break;
        case 1: bad.version = 2; break;
        case 2: bad.instance = nullptr; break;
        case 3: bad.physicalDevice = nullptr; break;
        case 4: bad.device = nullptr; break;
        case 5: bad.getInstanceProcAddr = nullptr; break;
        case 6: bad.getDeviceProcAddr = nullptr; break;
        case 7: bad.getInstanceProcAddr = instanceProc; break;
        case 8: bad.getDeviceProcAddr = deviceProc; break;
        }
        assert(bind(&bad) == sl::Result::eErrorInvalidParameter);
    }
    for (unsigned cycle = 0; cycle < 2; ++cycle)
    {
        assert(bind(&route) == sl::Result::eOk);
        assert(bind(&route) == sl::Result::eErrorInvalidState);
        assert(instanceProc(instance, "known") == nullptr); // Not claimed yet.
        assert(claim(instance, nullptr, device) == sl::Result::eErrorInvalidState);
        assert(claim(instance, physical, device) == sl::Result::eOk);
        assert(claim(instance, physical, device) == sl::Result::eErrorInvalidState);
        assert(instanceProc(instance, "vkGetDeviceProcAddr") ==
            reinterpret_cast<PFN_vkVoidFunction>(deviceProc));
        assert(deviceProc(device, "vkGetDeviceProcAddr") ==
            reinterpret_cast<PFN_vkVoidFunction>(deviceProc));
        assert(instanceProc(nullptr, "known") == nullptr);
        assert(instanceProc(reinterpret_cast<VkInstance>(99), "known") == nullptr);
        assert(deviceProc(reinterpret_cast<VkDevice>(99), "known") == nullptr);
        assert(deviceProc(device, nullptr) == nullptr);
        assert(instanceProc(instance, nullptr) == nullptr);
        assert(physicalProc(nullptr, "vkGetPhysicalDeviceProperties2") == nullptr);
        assert(physicalProc(reinterpret_cast<VkPhysicalDevice>(99), "vkGetPhysicalDeviceProperties2") == nullptr);
        assert(physicalProc(physical, nullptr) == nullptr);
        assert(physicalProc(physical, "vkDestroyInstance") == nullptr);
        assert(physicalProc(physical, "vkGetPhysicalDeviceProperties2") == sentinel);
        assert(physicalProc(physical, "vkGetPhysicalDeviceToolProperties") == nullptr);
        assert(instanceProc(instance, "known") == sentinel);
        assert(deviceProc(device, "known") == sentinel);
        assert(instanceProc(instance, "missing") == nullptr);
        assert(deviceProc(device, "missing") == nullptr);
        std::thread worker([&] {
            for (unsigned i = 0; i < 100; ++i)
            {
                assert(instanceProc(instance, "known") == sentinel);
                assert(deviceProc(device, "known") == sentinel);
            }
        });
        worker.join(); // Teardown requires the host to quiesce its work.
        clear();
        const auto before = calls.load();
        assert(instanceProc(instance, "known") == nullptr);
        assert(deviceProc(device, "known") == nullptr);
        assert(physicalProc(physical, "vkGetPhysicalDeviceProperties2") == nullptr);
        assert(calls.load() == before);
    }
    assert(calls.load() == 412);
    std::cout << "route contract passed: invalid inputs, binding, self-query, unknown objects, "
                 "physical ownership, missing commands, reentrant lookup, worker lookup, teardown and second lifecycle\n";
}
