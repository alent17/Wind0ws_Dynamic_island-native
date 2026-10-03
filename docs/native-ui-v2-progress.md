# Isle 原生 UI 2.0 进度表

依据：用户提供的 `Isle_Native_UI_2_Final_Plan.md` 与 `Isle_Multi_Agent_Development_Plan.md`。基线：`41626bc3ecf2342ea72e72aef152fc65feafe0b5`。开始日期：2026-10-02；组织方案更新：2026-10-03。架构、ownership、Sprint 与企业级门禁见 [执行与验收](native-ui-v2-execution.md)。

执行方式：主 agent 集成及验收；子 agent 使用 `gpt-6-luna` / `xhigh`（用户指定 6.0 luna、极高）。保留原生 Rust / Win32 / Direct2D 路线，保留原配置保护和旧独立播放器。

勾选规则：`[x]` 表示已完成并有验证证据；`[ ]` 表示尚未完成或尚未验收。模型、接口、单元测试完成不等于真实 Windows 视觉验收完成。

本次上传范围：保存当前原生 UI 2.0 基础、设置 Runtime / 位置 / 控件绘制增量及测试脚本到 `codex/native-ui-v2`。上传不代表全部方案或企业级发布门禁完成；完整控件状态、键盘 / UIA / IME、实机视觉与混合 DPI 保持待验，Glass 实际渲染和后续活动 / Widget 尚未完成。默认旧版路径保留，V2 通过 `--ui-v2` 启用。

| 阶段 | 内容 | 状态 |
| --- | --- | --- |
| Phase 0 | 保护层、基线、功能开关、性能指标 | 进行中 |
| Phase 1 | Runtime Settings、合并保存、失败及退出处理 | 基线及位置增量软件回归通过 |
| Phase 2 | 原生 D2D Settings 2.0、实时修改、窗口避让 | 位置 / 窄屏增量软件通过；完整控件绘制开发中，视觉待验 |
| Phase 3 | 状态、布局、动画分层 | 控件 Spring 的模型与窗口软件回归通过；动态 UIA / 视觉待验 |
| Phase 4 | Shared Album、多级封面、Crossfade | 开发中 |
| Phase 5 | Island 内完整播放器、能力驱动、交互稳定 | 开发中 |
| Phase 6 | Album Glass、动态色、缓存 | 纯颜色算法通过，实际渲染待接入 |
| Phase 7 | Timer / Volume Live Activities | 待执行 |
| Phase 8 | Built-in Widget Shelf | 待执行 |
| Phase 9 | Widget Editor 及键盘替代操作 | 待执行 |
| Phase 10 | 旧独立播放器兼容和后续统一边界 | 待执行 |

## Phase 0：保护层

- [x] 读取最终方案，核对仓库基线，建立本进度表。
- [x] 创建 `codex/native-ui-v2` 工作分支。
- [x] 运行现有单元测试、配置保护测试及 UI 自动化基线（截图另行验收）。
- [x] 增加 `--ui-v2`，保留默认兼容路径（workspace 编译 / 测试通过；原设置窗口保留）。
- [x] 增加独立 render CPU、Region CPU、封面上传、Blur 构建性能指标（V2 motion 实测有值，render 时间排除 Present；短样本不替代性能验收）。

## Phase 1：Settings Runtime Engine

- [x] Runtime / Persisted / Pending 与 Typed SettingsPatch（core / App 后台线程及固定 F699 隔离 UI 回归通过）。
- [x] runtime / saving / persisted revisions 与最新修改合并（core 测试通过）。
- [x] 立即应用到真实 App，不等待落盘（V2 滑块快速修改实时状态验证通过）。
- [ ] Toggle / Combo、Slider、Text 保存节流。
- [x] V2 播放器允许全部 / 手动勾选 / 优先排序实时保存与重启恢复（离线 fixture、冲突恢复回归通过，不发送真实媒体控制）。
- [x] 保存失败状态、重试、恢复已保存设置（V2 外部冲突、重试保护及 Runtime 恢复回归通过；文字显示视觉验收另记）。
- [x] 有限等待退出 Flush，记录失败（后台线程及 V2 debounce 退出 / 重启一致通过；冲突退出错误返回有测试，App 失败记录路径已代码核查）。
- [x] 未知字段保真、锁、备份、外部冲突、损坏文件保护回归（core / 后台线程、V2 和 configuration_faults 五类配置通过，固定 F699 候选）。

## Phase 2：Settings 2.0 UI

- 完整 D2D 控件包已落入当前源码，见 [设置控件方案](native-ui-v2-settings-controls-plan.md)。包含 Common Controls v6 manifest、共享 D2D painter、保留原生 HWND / EDIT、完整下拉框外框绘制与故障回退；完整状态、键盘 / IME / UIA 和视觉门禁尚未验收，不勾选整项完成。
- [x] 完整控件前置：嵌入 Common Controls v6 / PMv2 / asInvoker manifest，保留 ICON / VERSIONINFO；E6DD 资源与 inspector activation、目标进程 WinSxS v6 模块 / 主窗及设置 PMv2、legacy 控件保存重启与正常退出、V2 保存和 30 次设置生命周期软件回归通过。此项不代表 D2D 控件迁移完成。
- [x] 移除假 Preview 和 Apply 按钮（源码核查与 V2 104 / 105 / 212 按钮不存在检查通过）。
- [ ] D2D Shell、六页导航、中性暗色 Tokens 和 Cards。
- [ ] Toggle / Slider / Segmented / Dropdown 完整交互状态。
- [ ] 原生 EDIT 保留中文 IME、剪贴板及选择。
- [ ] 真实 Isle 保持展开检查，离页及关闭恢复。
- [x] 设置窗口位置增量软件回归：默认不写回、有空间避让、手动移动 / 缩放重开与重启、离屏 / 溢出恢复、手动覆盖 owner、未知嵌套字段保真、父关窗补取最后位置（D060，7 个隔离场景，均正常退出）。
- [x] 设置导航与横滚软件回归：模拟 DPI 的六页可达、2×3 紧凑布局、字号随 DPI、标签 / 控件同步横滚、实际 Tab 到右侧控件及 End 改值（D060；实际 1366 px / 200% 矮工作区硬件验收不包含在内）。
- [ ] DPI、最小尺寸、移动、位置保存、离屏恢复及窗口避让的实机视觉 / 混合 DPI 最终验收。
- [ ] 实时控件接入，功能只显示已支持能力。
- [ ] Tab / Shift+Tab / Enter / Space / Arrow、UIA / MSAA 验收。

## Phase 3：Layout / Motion 分层

- [ ] UiState 正交 Surface / Interaction / Activities。
- [ ] LayoutSnapshot 与统一 Edge Layout Mapper。
- [ ] VisualState、AnimatedRect 与 Motion Profiles。
- [ ] retarget 缩小职责，Renderer 消费当前动画值。
- [x] 可中断、可反向 Spring 与统一 Reduced Motion（V2 模型覆盖 20 / 50 / 80% 反向、四边及静止停止；窗口验收另记）。
- [x] Debug Motion Time Scale：1 / 0.5 / 0.2（CLI 与 MotionPolicy 接入，模型测试通过）。

## Phase 4：Shared Album

- [ ] Compact / Hover / Expanded 共用单 Album Visual。
- [ ] 连续 Rect、稳定裁切、0–100% 与中途反向。
- [ ] 切歌原 Rect Crossfade。
- [ ] Thumbnail128 / Display512 异步升级。
- [ ] 有界 CPU / GPU / Blur Cache、设备丢失恢复。

## Phase 5：Liquid Full Player

- [ ] Island 内 Morph、封面、标题、歌手、时间进度、控制。
- [ ] Capability-driven 控制，真实 seek，队列入口按 Provider 能力显示。
- [ ] Content Spring，无延迟链，长文本及缺失媒体处理。
- [ ] 稳定 Interaction Envelope、120–180 ms Leave Grace。
- [ ] Press / Drag / Inspection 时禁止自动收起。
- [x] 按需帧调度，静止不持续 Present（暂停 fixture 两段 idle 的帧数 / Region 更新均不增长，帧计时器为 0）。

## Phase 6：Glass

- 实现接口与门禁见 [Glass / Activities 工作包](native-ui-v2-glass-activities-plan.md)；纯算法已准备，实际 Renderer 接入待执行。
- [x] 纯颜色与 Contrast Clamp 算法：对预乘 BGRA、封面透明度、最坏缓存像素和受限 accent 求保守暗遮罩；9 项独立测试通过。512×512 封面 / 96×96 白色缓存的 500 次优化构建采样 mean 883.849 μs、P95 1129.1 μs、最低文字对比度 4.897622。模块尚未注册，不计入 workspace 测试，也不代表实际渲染或过渡帧对比度通过。
- [ ] Album Glass 与 Dynamic Tint。
- [ ] Contrast Clamp、保证文字可读。
- [ ] 封面变化时生成 Blur Cache，动画只更新变换及透明度。
- [ ] 可选系统 Acrylic 能力检测与 fallback（P2）。

## Phase 7：Live Activities

- [x] Manager：ID 更新、去重、TTL、Priority、Dismiss、Complete（纯模型测试通过）。
- [ ] Timer / Volume 真实数据源。
- [ ] 左右各一个 Slot 与有界队列。
- [ ] Activity Spring 及退出动画。

## Phase 8：Widget Shelf

- [x] Music / Volume / Timer / Clock / Weather / System Stats Registry（注册模型，真实 Shelf 接入待做）。
- [x] Solo / Dual / Compact Layout（规则布局模型测试通过，尚未渲染）。
- [x] ID / Order / Span / Enabled 配置，未知 ID 和嵌套字段保留（core 配置往返测试通过）。
- [ ] Shelf 真实渲染及数据接入。

## Phase 9：Widget Editor

- [ ] Add / Remove / Reorder / Resize / Span。
- [ ] 规则化拖动与键盘排序替代。
- [ ] 设置保存及重启验收。

## Phase 10：Detached Player

- [ ] 明确 Legacy 兼容路径、可选入口，避免同时重写。
- [ ] 新 Full Player 稳定后评估共享 D2D Renderer（后续）。

## 综合验收

- [x] cargo fmt / check / clippy / workspace tests（2026-10-03 位置 / 控件 Spring / 短窗口导航修复源码：112 passed，1 项显式联网测试 ignored；UI 回归按各候选另记）。
- [x] Settings V2 创建 / 换页 / Resize / Paint / 关闭 30 次，预热后 GDI 稳定且进程正常退出（生命周期崩溃修复回归）。
- [x] 设置快速修改、不丢最后值、失败恢复、重启一致、外部冲突（固定 F699 候选隔离回归通过；后续位置工作包另验）。
- [x] Expand / Collapse 各 30 次，20 / 50 / 80% 反向（窗口 30 次循环及短时反向 smoke 通过；精确阶段反向与速度保持由模型测试验证，视觉另验）。
- [ ] 无媒体、暂停、缺失封面、长文本、未知时长、缺失设备、天气失败。
- [ ] 四边及 100 / 125 / 150 / 175 / 200% DPI。
- [ ] 多屏、异 DPI、主屏变化、睡眠恢复（真实硬件验收）。
- [ ] Keyboard / UIA / Screen Reader。
- [ ] Idle、60 Hz Motion、Region、Bitmap 和 Blur 指标。
- [x] 独立 Final Reviewer：BLOCKER = 0、HIGH = 0（D060 位置 / 控件 Spring 增量 0 / 0 / 0 / 0；修复前 F699 的 MEDIUM 2 已在此增量收口，整候选仍有 Glass 未接入的 LOW 1 和全方案未完成项；后续源码须再审）。

## 企业级组织进度

- [x] 合并两份方案并保存来源副本。
- [x] 架构摘要、已有能力、风险、ownership、依赖图和 Sprint 计划。
- [x] 子 agent 使用 `gpt-6-luna` / `xhigh` 分包，当前并行稳定设置、动画渲染、配置与媒体。
- [x] Sprint 1 / 2 软件集成门禁（固定 F699 候选测试、release 和适用软件回归通过；硬件与视觉门禁保留未验收）。
- [x] A6 独立 QA 和性能记录（固定 F699 候选，10 项回归通过；报告明确软件采样和平台限制）。
- [x] A7 独立最终审查及高风险修复复核（固定 F699 候选，不代表后续 WIP 或整个方案完成）。

## 验证记录

- **94E3 窗口回归未通过**：`ui_v2.py` 第一次 fresh diagnostic 就超时，尚未打开 Settings，未生成 `snapshot.json`。owned PID 29452 / 主窗 HWND 8130136 收到 WM_CLOSE 后未立即退出；当次启动 20:23:43 后，临时错误日志在 20:23:44 记录 `0x80070057` 参数错误。QA 核实同 PID 错误对话框的相同 HRESULT，并点击其“确定”正常关闭；进程随后消失，未强杀，但退出码未捕获，不能记为 exit 0。根因尚未判定，不能宣称控件 smoke、生命周期、故障回退或产品运行通过；完整调查、修复及同 SHA 重跑保持待执行。本次仅上传可编译 / 单测通过的开发功能分支，默认旧路径和历史候选保留，不作发布验收。
- 2026-10-03 当前上传候选已冻结：`target/ui2-settings-painter/release/isle-native.exe`，SHA `94E33D2E6C56E64A9C54CE86D76B4899B862239266A7934DDCD312739AC01930`，独立 release 1m16s。最终 fmt / workspace check / clippy warnings-as-errors / workspace tests 通过：core 41、app 36、ui 38，共 115 passed，1 项显式联网测试 ignored；另 9 项 Glass 纯算法测试单独记录，不计入工作区。资源 / inspector activation 检查通过，保留 v6 / PMv2 / asInvoker / ICON / VERSIONINFO；此检查不证明产品实际绘制、UIA 或视觉状态。同 SHA 窗口 smoke 另行记录。
- 证据路径维护：`artifacts/manifest-controls/results.json` 当前为 94E3，E6DD 的该 inspector 独立旧副本未找到；下方 E6DD 静态资源结论是当时的历史报告，不得使用当前文件冒充 E6 同 SHA 证据。E6DD 整个 UI V2 回归目录已保存到 `artifacts/ui-v2-e6dd-history/`，后续候选可以复用 canonical 而不覆盖此历史。
- Glass 纯算法独立核验：`rustc --edition=2021 -O --test` 的 9 项全部通过；root 另用仅纯函数的优化 microbenchmark 采样，证据 `artifacts/glass-style/results.json`，source SHA `10BB614A4333BC6D44D283E0CCD5333BF92AA97C0696B0421DC3443FE6807BD7`。实际 draw/cache/Spring、设备恢复、视觉与动画帧可读性仍未验收，不据此关闭 Phase 6。
- 2026-10-03 继续设置完整控件工作包：当前工作区 `cargo check --workspace` 通过，但尚未冻结新的控件候选。子 agent 分别完善 D2D / 原生回退、准备键盘及 Settings UIA 回归、核对 Glass 纯颜色算法。Settings UIA 的 E6DD 初始 inventory 仅为树枚举证据，全部 Pane 的原因需先核实标准客户端代理，不能据此宣告产品缺陷或无障碍通过。新控件必须重新完成编译、测试、独立 release、同 SHA 动态回归与审查；不继承 E6DD 结论。
- E6DD 倒计时严格复测与 A7 独立核对最终通过：canonical 结果 PID28860、同 E6DD SHA，stable UTF-16 `WM_SETTEXT` 返回 1，`WM_GETTEXT` 回读 EDIT buffer 25；fresh start=1499.9999936、pause=1499.9829654、reset=1500 秒，分别满足 1495..1500 与精确 1500 断言。正常退出 0 / 47 ms / 无强杀，配置有效且 demo 无真实音频 / 媒体服务。旧 PID33568 误通过已另存 `partial-false-positive-30-minute.json`；前两次 helper failure 保留，未改产品源码。此项正式关闭 E6DD 补充软件门禁。
- E6DD 倒计时补测的 PID33568 结果经 A0 独立复核拒绝按“25 分钟”PASS：artifact 实际 started≈1800 / reset=1800 秒，仍为 30 分钟；helper 使用 `>=1400` 过宽断言，caption 显示 25 不能证明业务时长已改。已要求 A6 保留 false-positive 记录并改为 `1495..1500` / reset=1500，使用跨进程 `WM_SETTEXT` 与真实 EDIT 读取重跑，不伪造父 EN_CHANGE。Microsoft 明确跨进程控件须发送 WM_SETTEXT 而非 SetWindowText；目前仍为 helper 边界问题，未确认产品缺陷。此项恢复 pending，不影响此前严格 manifest / legacy Settings / V2 生命周期证据。
- 已撤销的历史结论（PID33568，不能作为 PASS）：当次 helper 曾将倒计时补测报告为 25 分钟通过，但后续独立复核发现业务仍为 1800 秒。原失败已另存 `partial-false-positive-30-minute.json`；正式结果为上方 PID28860 的严格 1500 秒复测。
- E6DD 独立复审已核实 manifest / legacy / V2 生命周期软件门禁，增量计数 0 / 0 / 0 / 0。A6 额外补原生倒计时 EDIT / Space 启停回归：前两次 helper 假设失败均保留，第二次立即请求诊断可能早于 TimerWndProc 转发内部 COMMAND，尚未证实产品缺陷；改为轮询新的目标状态后复测。此前 `native_settings.py` 的 Timer 仅为工具栏开关，不代替独立倒计时交互证据，该额外场景仍 pending。
- E6DD 同 SHA 的 `ui_v2.py` 已 PASS：实时保存 / inspection / conflict / revert / exit flush / restart 与 30 次 Settings 创建 / 换页 / Resize / Paint / 关闭，预热后 GDI 54，关闭样本 52–54、steadyRange 2。各段关闭后脚本断言 returncode 0，未留下 owned 进程；D060 结果已保存 `artifacts/ui-v2-d060-history`。完整 D2D 控件正在新源码实施，尚未形成新候选或其测试结论；E6DD 仍只代表 manifest 前置包。
- E6DD manifest 前置资源 / legacy 兼容已 PASS，SHA `E6DD5BACDC6F7CACD8FA5F24110BC63C72087F0EBB3EE5E20AAAADAB2C90A4E7`。release 独立构建耗时 2m01s。实际测试进程加载 WinSxS Common Controls 6.0.26100.9278 模块，主窗 / Settings / 重启主窗 PMv2=true；两次 legacy 回归通过，最终 owned PID 7392 / 29384 正常退出码 0、无强杀。inspector 仅用于可信本地构建并在自身进程调用解析到的 ComCtl32 DllGetVersion，不执行目标 EXE；与目标 runtime 证据分别记录。A4 开始完整控件业务实现，E6DD 冻结不覆盖；A6 继续同 E6DD 的 V2 生命周期兼容回归，A7 前置包独立复审待最终软件证据。
- D060 同 SHA 的 strict legacy/demo UIA 复测已 PASS：精确按 PID + `IsleNativePrototype` class 选择主窗，PID 29892 / HWND 1446544，配置有效且无真实媒体 / 音频服务字段；UIA / MSAA 断言通过，正常退出 0，用时 48 ms、无强杀。此前失败原因及未确认部分按历史保留；此证据不代替 V2 专用 provider Bounds 或 Settings UIA。A7 同 SHA 增量复审 0 / 0 / 0 / 0，完整方案与硬件 / 视觉验收仍未完成。
- 设置完整 D2D 控件的 Common Controls v6 manifest 前置已落入 `build.rs` / `isle.manifest`，保留 ICON、VERSIONINFO、字体复制，声明等价 PMv2 与 asInvoker。当前 fmt / check / clippy / 112 tests + 1 ignored 通过，正在独立 `target/ui2-controls` 构建；资源 / activation inspector 和兼容 UI 门禁完成前不接入控件迁移。
- D060 最后的 legacy/demo UIA 兼容回归遇到测试宿主问题，暂不记通过：先因 PowerShell 7 与 Framework Accessibility interop 版本不匹配，在启动 App 前编译失败；Windows PowerShell 5.1 下交互通过，但 finally 关闭后 5 秒未退出。随后 A6 精确枚举观察到 `.MainWindowHandle` 选择同 PID 的 UAC Input Indicator，而实际 `IsleNativePrototype` 主窗仍响应。原关闭句柄未记录，不能断言原次一定错发；已确认窗口选择不可靠，脚本须按 PID + class 缓存严格主窗并检查 PostMessage 返回值，保留原失败再同 SHA 重跑。另外修正 PS5 UTF8 BOM fixture，并要求配置解析有效。下一包 manifest 暂停，无产品源码改动。
- D060 `ui_v2.py` 与 `ui_v2_motion.py` 已 PASS：30 次设置窗换页 / Resize / Paint / 关闭后的 GDI steadyRange 2；motion 21 场景含四边 × 五档模拟 DPI、seek、反向 smoke 与 30 次展开收起。额外 idle 2 秒 frames 保持 4013、RegionUpdates 保持 2066。短样本 renderCpuP95 约 1.646 ms、regionCpuP95 约 0.647 ms、封面上传约 0.2869 ms、Blur 构建约 2.3993 ms；不据此宣称硬件刷新率或视觉验收。自有进程正常退出 0，无强杀；两份 artifact 的 SHA 均为 D060。同版本 legacy/demo UIA 兼容检查随后执行，V2 provider 专用 Bounds 动态证据仍待补。
- D060 `ui_v2_placement.py` 最终 7 个隔离场景 PASS，所有进程正常 exitcode 0、无强杀。真实查询 DPI 96、工作区 1920×1032；模拟 192 DPI 请求 1366×690 被 min-track 限制到实际 1466×1032，因此不宣称真实 1366 / 200% 验收。模拟 384 DPI（400%）实际客户高 244 DIP，2×3 导航、六页切换、字体 56 px 通过；恢复模拟 192 后字体 28 px、横滚标签与控件同步位移 -166 px，真实 Tab 到 ID213 并 End 选择 index3。前四次脚本前提失败已分别保留并修正；未更改 OS DPI / 工作区。证据：`artifacts/ui-v2-placement/results.json`，SHA 与 D060 一致。生命周期 / motion / 同版本兼容 UIA 继续验收。
- D060 placement 后续已运行默认不写回、有空间避让、离屏与溢出恢复、手动移动 / 缩放重开与重启、嵌套字段保真、父关窗 dirty flush；最终动态紧凑导航场景暂未收口。实际 Windows 的 min-track 会把模拟 440 px 高请求限制到当前真实工作区对应的最小高度，不能用请求尺寸冒充实际 200% 矮屏证据。A6 将保留失败和 requested / actual 尺寸，另以较高模拟 DPI 覆盖 `<270 DIP` 布局分支，真实 200% 矮工作区仍待硬件验收。
- D060 的 placement 首测在默认避让断言失败：owner HWND 宽 480 px、位于副屏中央，左右各 720 px，均放不下 836 px 的设置窗口与 20 px 间距。静态复核确认这是无条件 non-overlap 的测试前提错误；既定方案允许空间不足时在工作区居中。A6 保留失败记录，并补充“无空间可见且不写回”及“owner 靠左、有空间时必须避让”两个场景；测试进程正常退出，不操作已有用户实例。候选动态验收仍进行中。
- 新冻结候选 SHA-256 `D060EDFA55D189766F41AB4DF48EF665F6E84BE65AF6759182B99D96C7F0974E` 已完成独立目录 `target/ui2-next` release（37.42s），包含短窗口导航与隐藏控件帧请求两项复审修复。A6 已收到同 SHA 隔离窗口回归任务；A7 按此候选单独复核，尚未替换已验收的默认开发构建。
- 2026-10-03 短窗口导航修复与播放器隐藏控件帧请求修复已通过 fmt / check / clippy / workspace tests：core 41、app 33、ui 38，共 112 passed，1 项显式联网测试 ignored。设置绘制和 HWND 使用同一个导航布局；426 DIP 以上保留原六行，270 / 320 DIP 压缩六行，220 DIP 使用两列三行；Resize / DPI 后重新定位并保持字体尺度。纯几何验证包含六项可见、不重叠、正常布局不变；新的 release 与窗口回归尚待收口，D86 仅为修复前候选历史。
- 新候选 D86F4204C2DE0A1594D1315BDE50BD97BE56BFD7346A548C9DD798E00BF1BF92：位置 / 窄工作区和 Full Player 控件 Spring 两包已合拢，fmt / workspace check / clippy warnings-as-errors / 108 tests + 1 opt-in network ignored 通过，独立 `target/ui2-next` release 用时 1m47。新候选窗口 QA 与 A7 审查进行中，尚未替换用户预览。新增 app dev-dependency 仅复用 core 已使用且锁定的 serde_json，以验证后台实际保存后的未知字段；未增加运行时依赖或更新库版本。
- 2026-10-03 用户明确取消关机。已写入 `cancel.signal` 防止旧 helper 再触发，status 标为 cancelled_by_user；Windows `shutdown /a` 在允许访问系统的执行环境返回 1116（当前没有待执行关机）。不再安排关机，继续开发和验收。
- 2026-10-03 08:14 用户恢复后要求继续。系统事件 6006 记录 02:47:36 停止，6005 记录 08:03:51 启动；原预览 PID 12792 和计时 helper PID 32580 均已不存在。旧计时仍保留 armed 历史记录，未观察到 helper 的 shutdown-requested 记录，不能宣称按其 03:44 截止执行。该一次性安排已过期，不重新设关机；源码及 QA artifacts 完整，继续下一候选集成。status.json 已标为 expired_after_system_restart。
- 用户 2026-10-03 追加安排：两小时后关机，或整个任务提前完成后关机。本机后台正常关机计时已设置至 03:44:51（不强制结束程序）；仅在完整任务完成时才提前触发，不将本轮回归通过当作整个方案完成。调度状态记录在隔离的 `artifacts/power-control/status.json`。
- 基线 `cargo test --manifest-path Cargo.toml --workspace`：69 passed，1 个显式联网测试 ignored。
- 基线 release 构建成功。
- `configuration_faults.py`：损坏、类型错误、超限、未知字段和未知时区启动保护通过，原文件保留。
- `native_settings.py --no-screenshots`：置顶、工具、动画、时区、四边定位、形状、颜色、重启和副屏边界通过。
- 系统 Python 缺 Pillow；改用 Codex bundled Python。当前执行环境抓屏返回 `screen grab failed`，视觉截图验收待补。脚本新增 `--no-screenshots`，保留正常截图默认行为。
- `interaction.py --no-screenshots` 独立重跑：8 项四边布局 / 跨进程透明点击路由检查通过。
- 当前 `cargo test -p isle-core`：34 项全部通过（含新的设置状态机、活动模型、Widget 保真测试）。
- 2026-10-03 全工作区单元测试：core 36、app 21、ui 34，共 91 passed，1 个显式联网测试 ignored；含实际后台线程 latest-wins flush、保存中 revert、外部冲突安全重试，以及 V2 反向与 Seek 命中回归。
- 2026-10-03 `cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`git diff --check` 通过。测试源不新增依赖。
- 2026-10-03 首个 release 构建成功并在 DISPLAY2 启动。恢复 QA 后，F8 创建设置窗口出现诊断超时；用户随后报告应用程序内存读取错误。独立审查确认 Settings V2 将局部 `Box<Theme>` 的借用指针存入 HWND，但返回时释放对象，造成悬垂指针及关闭时重复释放，评级 BLOCKER。首个候选不可验收。
- 生命周期修复：Settings 显式拥有 Theme；从创建 HWND 起使用 RAII 清理失败路径，成功后转移 Box 所有权，销毁窗口时清空借用指针并仅释放一次；Theme 清理 GDI 资源。修复后 fmt / clippy / 91 项单元测试及独立目录 release 构建通过。
- 修复候选 release 构建通过，SHA-256 `F699F378415C94D63055285CAE38369F322FA2E0AF3890A8142ACA763C549942`。A6 `ui_v2.py` 通过：F8、快速修改最后值、未知字段保留、检查锁、外部冲突 / retry / revert、退出 flush 与重启；5 次预热后又完成 30 次设置窗口打开 / 换页 / 实际 resize / paint / 关闭，每次诊断更新且最终窗口释放。GDI 基线 51，闭后 49–51，范围 2，无持续增长；进程退出码 0，未强杀。证据：`artifacts/ui-v2/results.json`。已用同一候选替换旧预览，在 DISPLAY2 重启用户查看实例（PID 12792，窗口响应正常）。其余兼容回归仍待完成。
- 审核修复：同曲目 128 结果覆盖已有 512、渲染器未消费动画封面矩形、Seek 命中与动画矩形不一致、暂停 crossfade 未继续请求帧、Hover Cancel 提前计时后丢失 Leave Grace。
- Settings 窗口位置目前未持久化，也未使用内存中的上次位置；因此 Phase 2 位置保存保持未勾选。Glass 仅半透明黑色渐变，缓存 Blur 尚未用于玻璃合成，不标记折射实现完成。
- A4 已完成并经 A0 审阅位置持久化接口设计，见 [位置方案](native-ui-v2-placement-plan.md)。源码工作包现已启动；接口设计本身不代表位置功能验收完成。
- A6 `ui_v2_players.py` 通过允许全部切换、离线播放器勾选和排序、关闭重开与重启、外部冲突及恢复保护。首跑脚本将当前 Media 导航视为可点击，遇到正常禁用状态；修正 helper 后同一候选重跑通过，未改业务源码。证据：`artifacts/ui-v2-players/results.json`。
- A6 `ui_v2_motion.py`：20 组模拟 DPI / 四边和 1 组压力场景通过，涵盖共享封面、合成 seek、30 次展开收起与短时反向。两段 idle 的 frames 均为 4010、Region 更新均为 2060，帧 timer 为 0。短样本 render P95 0.9145 ms、Region P95 0.6541 ms、封面上传 0.1692 ms、Blur 构建 1.857 ms；不据此宣称真实刷新率或硬件 DPI / 视觉通过。证据：`artifacts/ui-v2-motion/results.json`。
- 独立静态审查新增两项 MEDIUM：200% DPI 窄物理工作区下，Settings 控件仍按固定 DIP 横坐标布局，可能超出窗口；Full Player 的 `controls_opacity` 尚未被 Renderer 消费，按钮过渡验收未完成。均保留为待实现 / 待验收项，不勾选对应整项功能。
- A0 / A7 在运行无障碍脚本前发现测试启动隔离缺陷：脚本默认启动会连接真实媒体 / 音量。已显式改为 `--demo`，在 UIA 写值前与整组后读取新鲜诊断，要求真实服务字段缺席（App 仅在真实服务存在时输出这些字段）。未执行原有不安全脚本，不把字段缺席记为零次采样；修正后的通过证据见下一项。
- 隔离 UIA 实测通过且 A7 HIGH 已关闭：8 descendants、Invoke、RangeValue 67 / 拒绝 101、MSAA 名称 / 角色 / Focus / Bounds / stale provider / scaled click / collapse。初末真实服务字段均缺席，counter 记为 null；正常退出 code 0，用时 29 ms，未强杀。证据：`artifacts/accessibility-results.json`。初次 benchmark 禁用测试宿主导致 Invoke 被系统拒绝，未写值；移除该测试模式并保留 demo / 诊断守卫后复验通过。
- `configuration_faults.py` 五类保护与 `native_settings.py --no-screenshots` 的旧版置顶、工具、动画、时区、四边、形状颜色、草稿取消 / 重启通过。`display_lifecycle.py` 32 组模拟场景通过。`lifecycle.py` 首次第 8 窗关闭超时（无 PID / class，根因未确认），单例 Clock 同序列正常；严格主窗类选择后完整 30 个进程复验全部正常退出。历史失败保留，不推定为 IME，也不以放宽等待通过。
- 下一候选源码工作包已启动：A4 位置持久化 / 窄 DPI 工作区适配，A3 Full Player 控件 Spring / 当前动画 Rect。固定 F699 用户预览及验收二进制保持不变；新源码 API / 单元 / release / QA / Review 完成前不替换预览或标记新功能完成。
