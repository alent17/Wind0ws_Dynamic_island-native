# Native UI 2.0 Phase 6/7：Glass 与 Live Activities 工作包

日期：2026-10-03。本文是供后续实现包接手的只读架构方案，不代表功能已实现或验收。依据 `docs/Isle_Native_UI_2_Final_Plan.md`、`docs/native-ui-v2-execution.md`、进度与审查文档，以及当前 `native/crates` 源码静态核对。A3 正在处理播放器控件 Spring（`model/layout/visual_state/motion/render`），A4 正在处理设置位置与窄屏适配；本方案不要求打断或同时修改这两组工作。

## Result

### Scope

为 Phase 6 Album Glass 与 Phase 7 Timer / Volume Live Activities 定义最小完整接入、共享基础、实施顺序、文件 ownership 与企业级门禁。

静态核对的现状：

- `isle_core::activity::ActivityManager` 已有 ID 更新、去重、32 条容量、Priority、TTL、Dismiss、Complete、最多两个 slot 和 `next_expiry()`；这些目前只是 core 模型与单元测试。全仓没有 App 侧 manager 实例或调用。
- `isle_ui::state::UiState` 有 `activities`，`Model::set_activities()` 会校验并截成两个；但没有调用方。`LayoutSnapshot.activities` 目前只是 `Vec<ElementLayout>` 占位，`layout::compute()` 不产生活动布局，Renderer 也不绘制活动。
- Renderer 已有以 `Arc<Cover>` 标识的 `glass_cover`，封面变化时生成 96×96 模糊位图；生成发生在 `draw()` 的封面变化路径，缓存没有被合成。V2 expanded 背景仍是固定黑色线性渐变，当前不能称为封面玻璃或折射。
- `cover_spectrum_palette()` 已从真实 Cover 提取两种 RGB，随当前/上一张封面缓存；可作为 Dynamic Tint 的输入，不能直接当可读背景色。
- `Model` 的倒计时以 `timer_deadline`、`timer_duration` 和 App 单调时钟驱动。倒计时完成时 `Model::step()` 无条件 `switch(Page::Timer)`，V2 中会打断用户正在看的 Music。
- `AudioService` 只在 Isle 展开 Volume 页时 active；成功读取实际系统默认端点并在真实快照变化时发 `Event::Audio`，停止 active 后不轮询。`--test-fixture` 可填充假 `AudioSnapshot`，不能据此发活动。
- 窗口、swap-chain、布局当前使用 `HOST = 480 DIP`；最大主 Island 宽 396 DIP，外侧余量各 42 DIP。左右各放完整独立活动 Pill 会超出 host。当前 `region()` 只以主 Island outline 建一个 polygon Region，`Model::hit()` 也先要求指针落在主 outline 内；仅添加 `layout.activities` 会导致裁切和无法点击。

**共享实现**：保留并复用 `ActivityManager`、一个 App 单调时间域、现有主事件循环/一个 coarse timer、活动动画所需的 Spring/按需 frame timer、Cover 与双 palette、D2D device/context 生命周期、DIP 布局和 UIA revision 机制。Activities 是主 Surface 的正交状态；不复制 manager、不为每个 Activity 建线程或 timer、不在 Renderer 内创建业务状态。

**必须串行的部分**：先等待当前 A3/A4 交付并固定 API/窗口坐标边界；再完成 Phase 6 的合成和单独门禁；Phase 6 通过后再接 Phase 7 的 App 源适配和 Pill 绘制/Region/UIA。Glass 与 Activity Pill 都会碰 Renderer、视觉状态、Region 和像素证据，不能让两包同时改这些文件或混在同一轮 QA 中。Phase 7 内先 Timer（纯 App 内真实状态、较低外部副作用）后 Volume（真实 AudioService、安全隔离门禁）；可共享 manager 与呈现协议，但两个 source adapter 及其证据分别验收。

### Files changed

本方案唯一新建文件：`docs/native-ui-v2-glass-activities-plan.md`。没有修改业务源码、QA 脚本或其他文档；没有运行构建、测试、UI 或进程操作。

后续建议 ownership（须由 A0 在 A3/A4 交付后确认）：

| 工作 | 主要 ownership | 边界 |
| --- | --- | --- |
| App 活动源协调、`ActivityManager` 实例、single-clock expiry 调度、V2 Timer 完成路由 | A0，`crates/isle-app/src/main.rs` | 不增加音量 idle polling；仅在 App 事件边界读取 source 并同步模型。 |
| Activity slot 的 DIP 布局、按 ID 保留的 Spring、Glass opacity Spring、主模型命中快照 | A3，`crates/isle-ui/src/{layout,model,state,visual_state,motion}.rs` | 等当前播放器 Spring 包交付后再进入；不与当前 A3 变更并行。 |
| Blur bitmap 真正合成、tint/contrast、Activity Pill 绘制与 GPU 资源恢复 | A2 或 A3 的 Renderer owner，`crates/isle-app/src/render.rs` | `render.rs` 当前归 A3 工作范围；须先做文件交接，单一 owner。 |
| 外包络窗口定位、work-area 降级、组合 Region 与 DPI 变换 | A0 主窗口集成；协调 A4 窄屏/位置 API | 不改动 A4 正在编辑的 Settings/configuration 文件；待其交付后用新的边界 API 集成。 |
| 后续隔离活动回归、诊断与性能证据 | A6 | 新脚本如有需要另行分配；本方案不编辑脚本。 |
| 两个门禁后的独立风险复核 | A7 | Phase 6、Phase 7 分开审查，确认没有将旧限制标成通过。 |

### API changes

本次无代码 API 变更。实现包先由 A0 固定下列兼容契约；不要直接把占位字段当成可用接口。

**Glass 契约**

- Surface 背景只在 V2 的 Music/Album 可用时使用 Album Glass。输入必须是当前真实 `MediaSnapshot.cover` 的 512 封面；无封面、非 Music 或 V2 关闭时走安全 fallback，不沿用上一曲的颜色或 bitmap。
- 缓存按 `Arc<Cover>` 身份/媒体 generation 失效；96×96 BGRA cache 必须被 `DrawBitmap` 实际消费。动画帧只读缓存并变换 destination、clip、opacity/tint；不得每帧重新降采样或模糊。最多保留当前和切歌 crossfade 所需的上一张两份 cache。D2D device 重建时由仍在 Model 的 Cover 重建资源。
- 从已缓存 `cover_spectrum_palette()` 派生受限的 accent/dominant RGB；新增纯算法 `GlassStyle`（或等价私有值）包含主/辅 accent 与 dark-tint alpha。对最终 alpha-composite 后的白色普通文字计算 contrast，低于 4.5:1 时提高 dark tint；大号文字最低 3:1。颜色 clamp 与阈值算法可独立确定性测试。禁用或未加载专辑色时使用中性暗 tint。
- 为 Glass 添入共享视觉状态中的 `glass_opacity`（或等价 spring），目标由 V2 Surface/Album 显示状态确定；使用当前 MotionPolicy，反向保留 velocity，Reduced Motion 立即到目标。现有 A3 播放器控件 Spring API 稳定后再并入，不能在其当前编辑中抢改文件。

**Activity 契约**

- `App` 持有唯一 `ActivityManager`；它保留最多 32 条活动/有界 queue。`Model` 只收到两个可见 slot，queue 仍归 Manager 所有。按 `slots()` 顺序稳定映射 leading/trailing（Priority 同值时沿用 Manager 的稳定插入顺序）；低优先级活动排队，不无限扩宽。
- 将 `LayoutSnapshot.activities: Vec<ElementLayout>` 升级成可按身份关联的 `ActivityLayout`（至少 `id`、slot side、当前/target DIP rect、radius、opacity、interactive/exiting 状态）。文本/value/progress 从同 ID 的 `LiveActivity` 读取；动画、重排、完成和移除均按 ID，不按临时数组下标复用 velocity。A3 的 `VisualState` 复用现有 `VisualElement`/`AnimatedRect` 与 Spring，为两个 slot 提供可打断的 width/position、opacity、scale；退出时可短暂保留只绘制的 tombstone，source 已移除后立即关闭 hit/UIA 节点。
- `LayoutSnapshot.activities` 不能被当前主 shape clip。新增 DIP `ActivityEnvelope`，把主 animated outline 与两个 Pill bounds 做 union；`position_for`/viewport 保持主 Island 的全局锚点，按该外包络调整 HWND 和 swap-chain bounds，并在 DPI 边界统一转物理像素。Region 是主 polygon 加各活动圆角 Region 的 union；输入和 UIA bounds 使用同一 envelope/local transform。A0 必须先确认 host/window resize 生命周期与 A4 的 work-area 接口。
- `Model::hit()` 先命中当前可见且 interactive 的 ActivityLayout，再按原逻辑检查主 outline；每个 Pill 点击只导航到对应真实 Timer/Volume 页，不能直接修改端点或倒计时。Dismissing Activity 只移除展示，不暗中 Reset Timer 或改系统音量。若增加可见 dismiss affordance，它必须有各自 UIA Invoke 与 stale-revision 防护。
- Renderer 绘制主 Island 与两个 Pill 为各自独立的 rounded clip；布局/Pill 不可被主 outline mask 裁掉。可点击区域与 Region union 同几何来源。Activity 名称、当前值、role、screen bounds、Invoke 进入 MSAA/UIA snapshot；排序/移除递增 revision，使 stale provider 返回断开错误。

**真实 source 与时钟契约**

- Timer ID 固定为 `timer:countdown`。由 App 自己的 `Model.timer_active/timer_deadline/timer_left/timer_duration` 产生：只有用户设置并启动过的真实应用倒计时才创建；运行时用 `ceil(timer_left)` 与 `1 - left/duration` 更新相同 ID。暂停状态可以保留真实活动，但 value 明示暂停且不产生 1 Hz 动画；Reset 移除；进入 finished 时调用一次 `complete(id, now)`，沿用 Manager 3 秒完成 TTL。默认 `timer_minutes=20` 不等于存在 timer，`--page timer` 也不能创建 pill。
- V2 到期时将 `timer_finished` 和 transient completion 交给正交 Activity，不调用当前无条件的 `switch(Page::Timer)`；如果用户正在 Music 或其他页，主 Page 保持不变，点 Timer Pill 才导航。当前/旧 UI（非 V2）保留旧自动切换以免改变兼容路径。在 V2 compact 中 Activity 作为旁侧 Pill，不替换真实音乐 compact artwork；legacy compact timer 展示路径保持原样。
- Volume ID 固定为 `volume:default`，来源只取真实 `AudioService::take()` 后的成功 `AudioSnapshot`。忽略首次设备基线、`failed`/空端点、test fixture、离线 demo 和 UIA 值写入；仅当已有成功快照后真实 volume/mute/default device 发生变化，更新同一活动、显示百分比或静音、设置短 TTL（建议 2 秒）。本版本只承诺 Isle 展开 Volume 页期间 AudioService 实际观察到的变化；不宣称监听 Volume 页关闭后的全局热键/OSD。若以后要支持系统范围端点事件，应另立 callback/source 包，不得用常驻轮询伪装。
- Source reconciliation、Manager `expire(now)`、Timer 截止与 Activity TTL 全部使用 `model.now = start.elapsed()` 的 monotonic 秒。一个已有 `WM_TIMER` ID 1/coarse scheduler 重新设为“最近真实 deadline”：Timer 活动的显示秒边界、正在运行的 timer deadline、活动最早 expiry、已有 progress/clock tick 取最早者；重算时复用此 scheduler，不新增每活动 timer 或 worker。动画仍只由现有按需 frame timer 驱动。暂停 Timer/无 expiry 的活动/idle Music 不应保持定时器或 Present。

### Behavior

**Phase 6：Album Glass**

1. 等 A3 为 shared Album 给出稳定 current Rect、clip 与 opacity，再在 V2 expanded Music Surface 后方绘制当前专辑模糊纹理，裁切到当前 animated 主 outline。叠加自适应 dark tint、低强度受限 accent、微弱顶部 highlight 和 border；标题、歌手、进度、控件始终在上层。无真实 cover 时回退安全深色背景，绝不留下上一首 tint。
2. 复用已有 96×96 blur 算法和 cache，但真正上传并合成；记录 `blurBuildMs`。当前 CPU 构建在 Renderer 的 cover-change draw 分支中，一次性而非每帧；若目标机器门禁测得封面变化帧超过帧预算，再把 CPU pixel 预处理移到已有 Artwork worker，D2D bitmap 仍只能在 Renderer device/context 线程创建。不能把一次封面变化 CPU 小峰值隐瞒成“异步”。
3. Glass enter/leave 与 Album/Surface opacity 使用 Spring 和真实中间值，不按 `m.expanded` 瞬间切渐变。切歌时按 `artwork_fade` 对当前/前一张的 blur/tint crossfade；目标反向仍从当前值/速度 retarget。Acrylic 是 P2，不属于本包。
4. Reduced Motion 保持暗色静态 Glass 和正确的共享几何，opacity/tint 立即到位且不做明显颜色漂移；不启动独立动画时钟。

**Phase 7：Timer / Volume Live Activities**

1. Timer 活动作为正交 Surface 活动：实际启动/恢复时创建/更新；暂停时静止保留并明确显示暂停；Reset 移除；完成时显示 3 秒后消失。V2 timer 完成不能把展开 Music 切走。完成/expire/remove 需要一次退出 Spring，随后释放视觉 tombstone 与 host 扩展。
2. Volume 活动只对 AudioService 成功且不同于首次 baseline 的实际系统端点快照变化出现；同 ID 合并连续变化，最后值胜出，2 秒无更新自动消失。AudioService 仍严格只在真实 Volume 页面激活，离开后不轮询。打开 Volume Pill 只导航，dismiss 只藏通知，不回写端点。
3. 最多显示两个独立活动；highest priority 在 leading slot，second 在 trailing slot；超过两项保留有界 queue。正常主 Island 与两 Pill 在边缘方向映射后仍保持可读方向。空间不足时隐藏低 priority slot 并保留 queue，不缩成不可读文字、不溢出 work area。
4. 动画 enter/retarget/exit 使用共享按需 Spring；每帧的 Layout、D2D 绘制、Window Region 和 hit geometry 读取同一 animated snapshot。稳定以后 ID 1 timer、frame timer 均按真实状态关闭；可见活动没有源事件时也不能持续 Present。

### Tests

本次没有运行构建、测试、QA 或 UI；以下是后续包的必需门禁，按顺序留独立 SHA/结果：

| 门 | 最小证据与退出条件 |
| --- | --- |
| G0：handoff/API freeze | A3 播放器 Spring 与 A4 位置/窄屏包交付并通过各自门禁；A0 记录 activity rect、viewport/envelope、DIP→pixel 与 owner 边界。确认没有两个 owner 同写 `render.rs`/`main.rs`。 |
| G1：Glass pure + Renderer | clamp/style 对黑、白、灰、高饱和 cover、alpha/BGRA、无 cover 的确定性测试；确认 96 cache 实际进入 DrawBitmap、只按 cover identity 重建，切歌重建/设备丢失恢复；四边/DPI 与 expanded↔compact 0/20/50/80/100%、20/50/80% 反向。对比截图检查文字 contrast 和无 stale tint。记录 render CPU、blur build/upload、bitmap 数量，Glass settle 后 frames/Region 不增长；不把合成/渐变称折射。 |
| G2：Timer source-only | 真实 UI 启动、pause/resume、reset、完成、完成 TTL、app 单调时间、隐藏/恢复；无 Timer 时没有活动，默认 20 分钟和 `--page timer` 不造卡；V2 Music 保持当前 Page，legacy 自动导航行为不变。Manager 更新/去重/TTL/priority/queue/invalid input tests 保持通过。 |
| G3：Activity geometry/visual/UIA | 每个 Pill 进入/改变 slot/priority 抢占/退出、20/50/80% 反向、reduced motion、两槽/queue、dismiss/complete stale provider；Top/Right/Bottom/Left，100/125/150/175/200% DPI 与窄工作区。截图、mouse/hit、UIA/MSAA bounds 和 Region 必须一致；两侧独立 Pill 不裁切、不吃掉 envelope 外桌面点击，settle 后 idle Present/Region 不增长。 |
| G4：Volume source 安全门 | `--demo`/fixture 的前后 fresh diagnostics 必须没有真实服务字段（如 `audioPolls`/`audioWrites`/`audioEndpointAlive`），且零 Volume 活动；UIA Invoke 不写音量。live smoke 只在 AudioService 已 active 且有 baseline 后读取真实 endpoint；首次快照、失败/空设备不触发，真实变化触发/合并/过期，离开 Volume 后无后台 polling。未经单独授权不做真实音量写入。 |
| G5：集成审查 | 只在 G1 通过后进入 G2–G4；A6 每轮各记录命令、候选 SHA、场景、诊断、性能和限制；A7 分别复核 Glass 与 Activities，最终 BLOCKER=0/HIGH=0。保留屏幕阅读器实机、多屏/异 DPI、睡眠恢复为单独待验项，不以合成样本代替。 |

### Risks

- 现有 `ActivityManager.slots()` 是两个活动的排序结果，不能替代几何包络、绘制、hit、Region、UIA 或退出 Spring；Model 的 `set_activities()` 单独使用会扔掉 queue 语义。
- `HOST=480` 和 `LayoutSnapshot.surface` 的 396 DIP 最大宽度证明左右完整 Pill 不在当前主 shape/host 中。新增窗口包络会影响 window resize、composition target、DPI、attached edge 定位与空白区域点击穿透；在此门禁失败时 Phase 7 不可标成完成。
- 仅给 `LayoutSnapshot.activities` 填 rect 仍会被 Renderer 的主 outline layer clip；当前 `region()`/`hit()` 也不会接受 outline 外点击。必须一并实现独立 pill clip、union Region、hit 和 UIA bounds。
- Timer completion 当前强制切 Timer page；如仅加 ActivityManager 而未改 V2 路由，Music 会被抢占，违背正交状态设计。Legacy 必须保留旧行为，V2 的新边界要单测。
- Volume source 只在 Volume 页活跃；这意味着目前不观测其他窗口/全局热键的音量变化。不要声称全局系统 Volume OSD 能力。测试必须区分 demo fixture 与真实 audio 服务，避免重现 UIA 写系统音量的 HIGH 风险。
- 96×96 缓存当前同步 CPU 生成且未使用；启用它增加封面变化帧成本。必须测量并限制 cache 生命周期，D2D device 丢失与长切歌序列不能积累 bitmap。Gradient 或 album-tint 单色都不代表折射。
- Glass 与 Activity 共用 Renderer/visual/Region/contrast/perf 面；并发修改或同一 SHA 混测会使回归归因无效，所以两 Phase 需要分门通过。

### Needs from other agents

- **A3**：先提交当前播放器控件 Spring；明确后续可用的 shared Album current Rect、radius/opacity、VisualState retarget/reverse/reduced 契约，再接 `glass_opacity` 与按 ID 的两个 Activity visual slots。当前包不插入其文件。
- **A4**：先提交设置位置与窄屏适配并报告 DIPs、work area / DPI / position 边界；本包不触碰其 Settings/configuration owner。A0 之后用这些稳定能力算主 Island 加活动的外包络与可用尺寸。
- **A0**：在 G0 固定跨 owner API、manager 实例和 source reconciliation owner；决策 dynamic envelope 如何保持主 Island global anchor/不激活窗口、`SetWindowRgn` union、Reduced Motion/hidden resume 以及 V2 Timer completion page 保持。协调 shared files 的串行交接。
- **A2/A3 Renderer owner**：在 A3 当前变更 handoff 后独占实现 Glass bitmap/tint/contrast cache 与 Pill 绘制/clip/device-loss 路径；不要让 A2、A3 同时修改 `render.rs`。
- **A6**：G0–G5 后另行提供隔离的真实/合成 source 回归与 QA 证据；Volume 自动化 fail closed，不更改或触发真实端点写值。
- **A7**：分别审查 blur 实际使用/contrast/idle 与活动 source 真实性/外包络/Region/UIA/Timer 正交性。

### Suggested next step

先让当前 A3 播放器 Spring 与 A4 设置位置/窄屏包各自交付；A0 完成 G0 ownership 和 host envelope 决策。随后单独执行 Phase 6：先锁定 GlassStyle 的可读性测试，再把现有 blur cache 真正绘制出来，通过 G1 后冻结 Renderer API。然后串行完成 Phase 7：A0 接 Timer source 和 V2/Legacy 分流，A3 给 layout/视觉 slots，Renderer owner 接 Pill，最后在独立门禁中接 Volume 的真实 AudioService 快照。所有 Phase 6/7 都通过后才进入后续 Shelf 工作。
