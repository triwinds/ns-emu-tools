# NR、DLAA、FG 八组合验证（2026-10-01）

本轮补齐组合运行证据并完成真实游戏测试。八种开关组合均有实际执行样本，NR → SR 交接、历史重置和呈现退休检查通过；用户确认切换和运动画面正常，无明显闪烁、拖影或花屏。Ryujinx 原有问题按用户要求保留记录，不再作为后端修复任务。

## 新增证据与检查

`target_fg_frame` 增加同帧 NR/SR 是否执行、最终颜色来源、FG/SR 控制版本与实际 FG reset。验收器按全局 frame ID 对应 NR、SR 和 FG，检查 NR 运行时 SR 消费 `nr_output`、SR/FG 对待处理 reset 的应用，拒绝重复/错帧、失效来源、缺失完成和 reset 记录。

FG 组合覆盖要求 SDK 报告实际双帧呈现、非零输入完成 semaphore/value，且等待成功；请求开启却因后台、预热或运动缺失而未实际运行的消费者，不计入另一种“关闭”组合。该检查证明调用及完成行为，不能证明显示扫描或生成帧画质。

持续消费者切换检查还要求控制版本不变、SR fence 身份不变、FG timeline 身份不变并递增。测试脚本仅在对应设置改变时增加版本，避免 NR 开关/强度变化顺带重建 SR，掩盖历史传播问题。旧日志没有新证据字段时标记 `checked:false`，不追认组合覆盖。

## 实机方法与结果

固定组合沿用此前 RTX 5070 Ti Laptop、610.88、Streamline 2.12.0、NR 运行库 A、Rust bridge 和 VVL 1.4.363.0。源图 1920×1080，窗口 2560×1335，合成常量深度，未处理真实帧 NVOF；SR 使用 DLAA 100%。启用核心及同步验证和 NR 读回，不作性能比较。NR、DLAA、FG 全关后由用户进入运动场景，保持窗口前台，依次应用控制、预热两秒，再各采样约十二秒。

原始会话在忽略目录 `src-tauri/target/nr-combinations-validation-001`。测试层 SHA256 为 `ed4f79ccf53f990dc34b1968b0c9776dee15020eb21b5192f71de87cf1cdc9fc`。强化消费者连续性分析后，重新读取未修改的原始日志；仅更新分析程序，测试层和原始帧证据不变。固定输入、阶段时间、各控制版本、报告、消息全文及文件摘要见 [summary.json](summary.json)。

| 稳定采样阶段 | 应用帧 | NR 评估 | DLAA 评估 | FG 实际双帧报告 |
| --- | ---: | ---: | ---: | ---: |
| 全关 | 101 | 0 | 0 | 0 |
| DLAA | 98 | 0 | 98 | 0 |
| NR + DLAA | 94 | 94 | 94 | 0 |
| NR | 93 | 93 | 0 | 0 |
| NR + FG | 92 | 92 | 0 | 92 |
| FG | 101 | 0 | 0 | 101 |
| DLAA + FG | 95 | 0 | 95 | 95 |
| NR + DLAA + FG | 89 | 89 | 89 | 89 |
| NR 强度改为 0.5，DLAA/FG 持续开启 | 91 | 91 | 91 | 91 |
| NR 关闭，DLAA/FG 持续开启 | 96 | 0 | 96 | 96 |
| NR 恢复强度 1，DLAA/FG 持续开启 | 88 | 88 | 88 | 88 |

全会话还含进入场景、控制过渡、反馈等待和清理，不能把这些总量当作各阶段性能。共 2,555 次 NR、2,328 次 NR → SR 交接、1,075 个 FG 开启帧和 1,074 次 SDK 双帧报告；应用 Present 5,407 次，原生提交/退休各 6,474 次，其中 2,141 次从 SDK 工作线程完成。输入 timeline 递增 1,073 次，最终全部回收；目标正常退出码 0。

frame 3030（NR 开启）、3141（强度 0.5）、3253（NR 关闭）、3374（NR 恢复强度 1）均在相同 SR fence `292077567337565611`、SR revision 8 和持续 FG timeline 下完成。四帧 SR 和 FG reset 均生效；NR 关闭时 SR 改读 `native_source`，其余读 `nr_output`。最长连续 NR 评估为 1,991 帧，时间历史检查通过。十五次读回全部有限、无哨兵残留，CPU 读回约 77–97 ms。

## 独立记录的 SDK 首次布局问题

队列并发、`fake-swapchain-buffer` 和早期 `nv.ngx.dlssnr.resource` 错误本轮均未出现。严格报告仍有 224 条错误、30 条未审查警告；其中两条涉及 **SDK FG 的新建 clone 输出**，不归入用户决定暂不修复的 Ryujinx 原有问题：

```text
VUID-vkCmdDraw-None-09600
nv.sl.dlss_g.clone.dlfg-output_0 / dlfg-output_1
SDK command buffer expects TRANSFER_SRC_OPTIMAL
validation records UNDEFINED
```

两条均发生在首次启用 FG（NR + FG 阶段），之后未继续重复。编号 1180184443 与此前假交换链错误相同，但对象和当前布局不同，不能按编号统一忽略。固定 SDK 的 `Vulkan::createTexture2DResourceSharedImpl` 使用 `initialLayout=UNDEFINED`，并将输出 Resource state 设为 UNDEFINED；`cloneResource` 虽设置 `desc.state=initialState`，创建代码没有据此初始化真实布局。闭源 FG 插件的首次转换与 opaque NGX 使用尚未捕获，不能仅凭这一静态发现直接补任意 barrier，或断言 NR 无关。

其余新增消息编号主要是已销毁游戏 buffer 引起的失效命令缓冲连锁报告（viewport/scissor/blend/clear 等）；原始消息保留。NR 执行与八组合检查通过，不代表 SDK 这两条布局错误已经修复。严格原始报告不被改写为零错误，launcher 因严格门控返回 1；这不阻止继续开发已验证的 NR 组合路径。

## 检查和剩余覆盖

133 项 Rust 测试通过、1 项硬件专用测试忽略。`cargo fmt`、crate 宿主/显式 Windows all-features/all-targets、默认/SDK-only，以及主应用宿主/Windows `cargo check` 通过，无编译警告或错误。NGX 固定库为 `nvsdk_ngx_d.lib`：首次测试继承工具箱 `+crt-static` 导致链接冲突，改用该诊断/原生 NR 组合所需的 `RUSTFLAGS=-C target-feature=-crt-static` 后完整通过，未通过抑制链接警告绕过。

本轮没有注入安全 NR 失败、完成十次 FG 活动下的 NR 切换，或测试实际 FG 活动中的 resize/最小化恢复。合成深度、光流和短阶段观察也不能替代正式画质/长期/性能验收；`safe_failure_tested`、P1/P2 总体通过、显示扫描和正式画质/性能标记保持未完成。后续优先核对上述 SDK clone 首次使用，再覆盖安全失败回退和工具后端管理。
