# A7 独立最终审查

当前上传限制（A0，2026-10-03）：94E3 的有限源码复读不等于动态验收。后续 `ui_v2.py` 在第一次诊断请求超时，owned PID 29452 未生成 snapshot；启动同时临时错误日志记录 `0x80070057`。窗口回归未通过，根因待调查，不继承 E6DD / D060 的 PASS。当前提交仅是编译、Clippy 与 115 项单测通过的开发功能分支；不报告产品运行通过或企业级正式完成。详见 QA 与进度表。

证据路径维护（A0，2026-10-03）：下文各节按候选保存历史，不能把旧节的审查计数用于当前控件 WIP。D060 `ui_v2.py` 已改指向实际保留的 `artifacts/ui-v2-d060-history/results.json`，F699 UIA 指向核对 SHA 的 `f699-accessibility-results.json`。F699 当次 `ui-v2/results.json` 在后续候选复用 canonical 时已覆盖，当前未发现独立原副本；本节数值保留为当次审查历史，现 canonical 为 E6DD，不能据现路径重新证明 F699 同 SHA。E6DD 节使用其实际 canonical artifact。此次只维护链接与证据保留限制，不修改 A7 已作出的审查结论。

日期：2026-10-03。审查依据为 `docs/Isle_Native_UI_2_Final_Plan.md`、`docs/Isle_Multi_Agent_Development_Plan.md`、`docs/native-ui-v2-execution.md`、`docs/native-ui-v2-progress.md`、`README.md`，以及当前未提交工作树和新增源码。审查范围限定 Sprint 1/2 与 P0；本审查未运行 UI 脚本，也未改业务代码。

## 结论与数量

当前只能作为开发候选构建继续 QA，不能判为企业级正式验收完成。A0 报告 `cargo fmt`、`cargo clippy -- -D warnings`、workspace 91 tests（另 1 个联网测试 ignored）和 release 构建通过；候选二进制 SHA-256 为 `F699F378415C94D63055285CAE38369F322FA2E0AF3890A8142ACA763C549942`。A6 的 `ui_v2.py`、`ui_v2_players.py`、隔离后的 UIA 回归与设置窗生命周期/GDI 压力检查已 PASS。设置窗显式预热后 30 次打开、4 页导航、真实 SetWindowPos/WM_SIZE、Invalidate/WM_PAINT、关闭均通过；最终 `settingsWindowAlive=false`，GDI 计数稳定在 49–51（跨度 2，门限 ±4）。UIA 测试同 SHA，确认前后 fresh 报告都不含音频/媒体服务字段，并通过 Invoke、RangeValue 合法/越界值、MSAA 名称/焦点/边界、stale provider、点击与折叠检查。证据位于 `artifacts/ui-v2/results.json`、`artifacts/ui-v2-players/results.json` 与 `artifacts/f699-accessibility-results.json`；对应二进制 SHA 均匹配候选。

| 严重度 | 当前开放 | 记录 |
|---|---:|---|
| BLOCKER | 0 | 曾发现设置 Theme 悬垂指针/重复释放，A0 修复，A6 生命周期与 GDI 30 轮压力测试通过。 |
| HIGH | 0 | UIA 真实音量副作用已由同候选隔离回归关闭；旧 unsafe 脚本未执行。 |
| MEDIUM | 2 | 播放器控件未消费其 Spring 透明度；极高 DPI 与窄工作区会裁切设置页右侧控件。 |
| LOW | 1 | Glass blur 缓存目前未用于合成。 |

## 发现

### 已修复 BLOCKER — 设置窗 Theme 生命周期

修复前，`Settings::new_v2` 将局部 `Box<Theme>` 的内部地址写入 `GWLP_USERDATA`，却在构造返回时释放 Box；之后窗口过程会通过悬垂指针访问 Theme，关闭时还会再次释放该地址。A0 已修复：`Settings` 持有 Theme 分配，构造中途出错也由 RAII 清理，userdata 在窗口生命周期内只借用该指针；Drop 先清 userdata 并销毁 HWND，再仅一次回收 Box，Theme 的 GDI 资源由其 Drop 释放。修复位置见 `crates/isle-app/src/settings/window.rs:725-730`、`789-793`、`823-827`、`1503-1520`。

触发方式：打开 V2 设置后再收消息或关闭窗口。静态复核确认原 UAF/double-free 所有权路径已消失。A6 用候选 SHA 对应的二进制完成显式预热后 30 次窗口打开、4 页导航、真实 resize/paint 与关闭；诊断状态最后清零，GDI 在 49–51 范围内稳定、owned 进程正常退出。动态证据 `artifacts/ui-v2/results.json` 关闭此 BLOCKER。

### 已修复 HIGH — UIA 自动化真实音量副作用

原 `scripts/accessibility.ps1` 启动命令缺少诊断隔离参数；`main.rs:2142-2159` 会自动追加 `--live-media`，随后初始化 `AudioService` 与 `MediaService`（`main.rs:2408-2417`）。该脚本的 `RangeValuePattern.SetValue(67)` 会在音频服务启用时把当前系统输出音量调到 67（`main.rs:1948-1951`）。A6 确认旧脚本版本尚未运行，因此没有已知的现场副作用。

当前脚本通过 `--demo` 与独立日志隔离；应用只有显式 live flags 才创建媒体或音频模型/服务。`Get-IsolatedDiagnostics` 解析新鲜 `prototype` / `renderer` 报告，并拒绝任何包含 `audioPolls` 或 `mediaPolls` 字段的报告；首个 UIA Invoke/SetValue 前和测试后都会检查（`scripts/accessibility.ps1:58-88,117-127`）。A6 使用候选 SHA `F699F378…C549942` 实测 PASS：`isolatedDemo=true`、`initialServiceFieldsAbsent=true`、`finalServiceFieldsAbsent=true`，poll 字段为 null；8 个 UIA descendant、Invoke、RangeValue 设为 67 并拒绝 101、MSAA names/focus/bounds、stale provider、scaled click 与 collapse 均通过。最终证据 `artifacts/f699-accessibility-results.json` 还记录 owned PID 24412、`ownedExitCode=0`、`gracefulClosePosted=true`、shutdown 29ms、`forcedKill=false`。这关闭了真实音量副作用 HIGH；旧 unsafe 版本未运行。

A6 报告第一次历史尝试在打开第 8 个设置窗口时诊断超时，但当时没有保留 PID / window class，根因无法归因。单例 Clock 同序列正常；helper 后来加入仅查找 `IsleNativePrototype` 类窗口，匹配候选完整 30 次进程压力测试通过。该历史记录为 inconclusive，不归因给 IME，也不保留为开放缺陷。

### MEDIUM — Full Player 控件 Spring 尚未进入实际绘制

`VisualState` 会更新 `controls_opacity` Spring，且把它计入 active motion（`crates/isle-ui/src/visual_state.rs:53-78`）；Renderer 没有读取该值，音乐控制仍按全不透明绘制（`crates/isle-app/src/render.rs:1412-1425`）。控件矩形来自目标布局，动画快照只把 Seek 矩形同步到动画进度条（`crates/isle-ui/src/model.rs:546-594`）。触发方式：V2 Compact / Expanded Music 展开或收起。按钮跟随动态窗口裁剪显露，但不会像 Shared Album、标题、歌手和进度一样使用控件 Spring。建议在 Full Player 阶段接入控件透明度与矩形过渡，并覆盖中途反向。

此项是 P0 Full Player 动效完成度缺口，不影响已实现的 Shared Album 矩形反向模型；Progress 表将 Full Player / Content Spring 标为开发中，不应据此宣称最终 P0 完成。

### MEDIUM — 200% DPI 窄工作区会横向裁切设置控件

初始窗宽按 820 DIP 乘 DPI 生成，但 `monitor_position` 会把宽度压到物理工作区宽减 32 px（`crates/isle-app/src/settings/window.rs:759-771`、`688-690`）。外观页贴边方向组合框固定在 x=617 DIP、宽 145 DIP（同文件 908 行），控件仍按完整 DPI 比例定位（340-355 行）；而最小窗宽仍固定为 720 DIP（521-529 行）。触发方式：1366 px 宽显示器设为 200% DPI。初始窗只能得到约 667 DIP 的外宽，右侧控件超出 client 区域；最小轨迹宽度也会超过可用工作区。建议缩放内容布局或为窄工作区提供横向适配，并以目标物理分辨率实测确认。执行文档已将高 DPI 工作区列作风险，本条由当前几何关系确认。

### LOW — Glass blur 缓存仍在绘制线程构建但当前未使用

V2 封面变化且专辑图可见时，Renderer 在绘制函数中调用 `blurred_cover` 并上传 bitmap（`crates/isle-app/src/render.rs:829-854`）。全仓 `glass_cover` 目前只声明、创建和清空，没有参与合成。每次新封面会同步做一次 512→96 缩小与两次盒式模糊；它不会逐帧执行，但会在封面变化帧产生不必要的 CPU 与 bitmap 上传开销。Glass 本轮明确 pending；建议在实际合成接入前停建该缓存，或测得负载可接受后再保留。

## 已审查通过的代码路径及边界

- 设置：`RuntimeSettings` 保留 Runtime / Persisted / Pending revision，后台写入完成后再启动最新 pending；保存中 revert 会排队落盘，外部文件变更会阻止覆盖。退出 `flush_timeout` 有 2 秒上限并返回/记录失败。未知字段通过原 Document clone 保真。A6 `ui_v2.py` 已覆盖实时保存最后值、未知字段、冲突、retry/revert、退出 flush 和重启并 PASS。
- Players：V2 选择器走 `Dialog::new_live`，禁用保存按钮，变更后通过主窗口进入同一个 debounced settings service。A6 `ui_v2_players.py` PASS，覆盖 allow-all 开关、离线播放器勾选、上下排序即时生效与落盘、关窗重开和重启恢复，以及外部冲突后 revert 保留外部文件；结果 `artifacts/ui-v2-players/results.json`，SHA 匹配候选。旧式 `native_players.py` 同候选回归也 PASS：manual/all/none、排序保存与重启、12 次 dialog 生命周期；私有内存 89.49→89.60 MiB 后稳定，handle 892。实际环境没有 GSMTC media session（managerAlive=false、polls=0），故真实 session 发现与播放控制仍未覆盖；两项脚本均未发送真实媒体控制，截图验收仍未做。
- Media / artwork：Seek 只接受有效 capability 和有效 timeline，移动请求在共享槽中合并为最后一个；封面分别提供 128 与 512 tier，同曲目输出按像素面积阻止降级；CPU cache 每 tier 有容量上限。实际播放端控制、高清封面升级和现场资源表现仍以 A6 的报告为准。
- Layout / motion / hit：V2 Renderer 使用 `shared_album_rect()` 绘制同一个 bitmap 的动画 Rect；共享封面宽高、位置、圆角由 Spring 驱动，model reverse tests 覆盖中途 retarget；Seek 的绘制与命中矩形共用动画快照。Region 和绘制轮廓均来自当前 outline，DPI 输入按窗口 scale 转回 DIP。A6 的实际四边、模拟 DPI、region 点击和帧 idle 结果待入 QA 记录。
- UIA：主 Isle 有 MSAA / IAccessibleEx provider，并为 Seek 提供 RangeValue 接口；同候选隔离 UIA / MSAA 自动化已 PASS。Settings、物理屏幕阅读器语义及完整键盘验收未实测；provider 存在不能代替屏幕阅读器验收。

## 尚未完成的方案范围与门禁

活动管理与 Widget Shelf 只有 model / registry，尚无真实数据源和 Renderer 绘制；Glass blur bitmap 尚未接入合成，当前所谓 Glass 仅为半透明渐变，不能称为折射效果。设置窗位置没有跨重启持久化。截图目视验收、真实 100–200% DPI / 多屏设备、睡眠恢复、真实 Region 鼠标路由和屏幕阅读器证据仍待完成。`docs/native-ui-v2-progress.md` 对这些项目保持未勾选是正确的。

完成定义仍要求适用 Sprint QA 全部通过、BLOCKER = 0、HIGH = 0。即使 workspace 测试与 release 构建通过，在其余适用 UI 回归、视觉截图和硬件限制未记录前，本开发候选也不能标为整个 UI 2.0 方案正式完成。

## Result

### Scope
独立只读审查当前未提交改造与新增 native 源码，范围为 Sprint 1/2 与 P0；只创建本审查文件。

### Files changed
`docs/native-ui-v2-review.md`。

### API changes
无。发现的 Theme 所有权修复由 A0 实施并完成静态复核。

### Behavior
记录设置保存安全、播放器选择、Seek、封面升级、共享封面绘制、反向 Spring、Region / DPI / UIA 的审查结论及未验收项。Theme 生命周期动态压力证据已由 A6 提供并核对。

### Tests
本 Reviewer 未运行构建或 UI 脚本，以免干扰 A6。接收并核对 A0 报告：fmt / clippy warnings-as-errors / workspace 91 tests（1 项联网测试 ignored）/ release 通过；A6 `ui_v2.py`、`ui_v2_players.py`、隔离 UIA PASS，重复设置窗口的 30 轮生命周期/GDI 检查 PASS（见 `artifacts/ui-v2/results.json`、`artifacts/ui-v2-players/results.json`、`artifacts/f699-accessibility-results.json`）；旧式播放器选择回归也 PASS（`artifacts/native-players-results.json`）。UIA 同 SHA 服务隔离、自然关闭与 exit code 0 均由结果 JSON 证明。

### Risks
UIA 同候选隔离安全测试通过，旧 unsafe 版本未执行。播放器控件 Spring 未消费；200% DPI 窄工作区可能横向裁切；无实机截图、屏幕阅读器、多屏与睡眠恢复证据；Glass、真实活动、Shelf、跨重启窗口位置未完成。

### Needs from other agents
A6 已闭合 UIA 安全与进程退出证据。继续记录 Sprint 1/2 适用 motion / DPI / region / idle 结果，并按真实媒体会话可用性补齐播放控制验证。

### Suggested next step
A6 已完成设置窗口生命周期/GDI 回归，生命周期 BLOCKER 关闭；UIA 真实音量副作用 HIGH 也已由同候选安全实测关闭。随后按执行文档补齐剩余 Sprint 门禁；保持活动、Shelf、Glass、窗口位置、截图及硬件验收的 pending 状态。

---

## 增量审查：D86F4204 候选（A3 播放器控件 / A4 设置位置与窄屏）

本节单独记录候选 SHA-256 `D86F4204C2DE0A1594D1315BDE50BD97BE56BFD7346A548C9DD798E00BF1BF92`，不沿用 F699 的二进制或 QA 证据；F699 的历史数量 `0 / 0 / 2 / 1` 保持不变。A0 报告 D86 的 fmt、check、clippy（warnings-as-errors）、workspace 108 passed + 1 network ignored 和 release 通过。该构建随后确认早于下述两个源码修复；此 SHA 未获 UI QA 结果，作为 pre-fix 候选不能通过审查门禁。

| 严重度 | D86 A3/A4 增量开放数 | 结论 |
|---|---:|---|
| BLOCKER | 0 | 本次静态审查未发现阻塞启动或安全性的确定问题。 |
| HIGH | 0 | 本次静态审查未发现确定 HIGH。 |
| MEDIUM | 1 | 矮工作区 / DPI 切换时导航按钮固定几何，部分设置页不可见、不可鼠标到达。 |
| LOW | 1 | 所有播放器 capability 同时撤回时仍为空控件 opacity 安排 Spring 帧。 |

D86 的 A3/A4 增量审查为 `0 / 0 / 1 / 1`。本候选全范围仍保留 F699 记录的 Glass blur 未使用 LOW，因此在 A3/A4 发现之外的候选总已知开放项为 `0 / 0 / 1 / 2`；D86 本身仍因 MEDIUM 与 LOW 未修复而不通过。

### A3：控件动画接线与 D86 LOW

源码中的 Full Player 目标由媒体 capabilities 生成；三枚按钮各有固定 `AnimatedRect`，`layout_snapshot()` 在展开态把当前动画矩形同时用于视觉布局、hit region 和 controls/UIA 列表。Renderer 用该矩形画图标，并消费 `controls_opacity`；时间和进度组消费 progress opacity。收起时 compact 控件仍可作为 outgoing draw 几何保留，但 `controls()` 与 hit regions 立即清空；缩回中重新展开的测试覆盖 20/50/80% 位置与速度。D86 的控件透明度和反向动画已比 F699 中记录的缺口接通。

**D86 LOW — 全部播放器 capability 一次撤回时多调度一段不可见动画。** `Model::retarget()` 将三个缺失 slot 立即放回 anchor，`layout_snapshot()` 没有按钮可绘制；但 D86 仍让 `controls_opacity` 从当前值 Spring 到 0，`VisualState::active()` 将其计入 `moving()/continuous()`。触发方式：Full Player 展开期间系统媒体 capability 同时变为全 false / 服务断开。结果是按钮不再显示或交互，renderer 仍被请求绘制到 opacity settle。A0 已在工作树修复：当全部 target 为 `None` 时立即 settle opacity，并添加全 capability revoke 后 idle 的模型用例；此修复不包含在 D86 SHA 中，须以重新构建的 SHA 复审。

### A4：位置、存储和窄屏适配静态结论

`SettingsWindowPlacement` 将负坐标保存为物理像素、将外窗尺寸保存为 DIP，限制 DPI/尺寸并使用 checked 换算。加载时按旧 DIP 与保存 DPI 找当前有交叠的工作区，再按目标显示器当前 DPI 换算并 clamp；移除的显示器没有交叠时走 owner 屏默认避让。Document setter 在更新已知字段时 clone 现有 placement 对象，因此未知嵌套字段会保留。窗口只在人工 move/size 的 ENTER/EXIT 周期上报一次最终值；父进程退出会补取正在进行的改动，再共用 settings revision/save worker debounce，主循环最后以两秒上限 flush。当前静态代码未发现未知字段丢失、负坐标溢出、显式移动修改污染配置，或 Theme/ HWND Box 所有权回归。

窄窗口横向范围以 client 宽与 content 右界计算；Direct2D 内容使用同一 scroll offset，原生子控件同步移动并按 content viewport 裁切，左导航和底栏保持固定。工作区限幅与 `WM_DPICHANGED` 会刷新 DPI、Direct2D、字体和控件位置。下面的导航缺陷仍使 D86 的窄屏适配未通过完整静态门禁。

**D86 MEDIUM — 高 DPI / 矮工作区会把 Settings 页面导航裁出窗口。** `add_nav()` 固定按 DIP 将六个按钮放在 y=`118 + 50×index`、高 42（`crates/isle-app/src/settings/window.rs:200-210`），而 D86 的 `WM_DPICHANGED` 只更新 nav 字体，没有重定位这些未注册在 `theme.controls` 的 nav HWND。小工作区默认位置与 `WM_GETMINMAXINFO` 会把外窗高度缩到工作区上限，导航容器不会滚动或折叠。触发例：1366×768、200% DPI 的可用客户高小于 368 DIP 时，Advanced 按钮从 y=368 DIP 开始而无法显示；500×350 px 工作区也会裁掉末尾导航。部分页面无法用鼠标选择；跨 DPI 后导航实体与 D2D 高亮还会错位。建议由 client height 统一算六个 nav Rect，resize/DPI 时同时更新 HWND 与 Shell 高亮，并给矮窗口提供紧凑导航布局及页可达性测试。A0 已接回此项并开始修改；修复 / 编译 / Windows UI 验收完成前维持 MEDIUM 开放。

### D86 候选门禁

**结论：D86 拒绝作为新候选通过审查。** 原 A3 LOW 修复和 A4 导航适配在 D86 release 后才进入源码，因此当前工作树源码与 SHA 不同。D86 也没有本节对应的 A6 motion / placement / 窄屏 UI 结果。A0 提供新冻结 SHA 后，需重新核实两项修复落入二进制，再按该 SHA 的 UI / DPI / 页导航结果单独更新计数；本节不借用 F699 或 D86 之前的 QA 记录。

### D86 Result

#### Scope
只读静态审查 Full Player 控件 Spring / 绘制 / hit / UIA，以及设置窗位置保存、恢复、未知字段保真、DPI / 窄工作区和退出 flush；不运行构建或 UI 自动化。

#### Files changed
只追加本节至 `docs/native-ui-v2-review.md`；业务源码未修改。

#### API changes
本审查无 API 变更。A0 对播放器 opacity 和设置页导航的修复另属工作树增量，未纳入 D86 二进制。

#### Behavior
记录 D86 的控件绘制与交互几何接线、全部 capability 丢失时的帧调度，以及矮工作区 / DPI 改变时的 Settings 导航可达性。

#### Tests
Reviewer 未运行测试、build 或 UI。A0 报告 D86 fmt/check/clippy、workspace 108 tests + 1 network ignored 和 release 通过；新源修复后需重建和同 SHA QA，不能以 D86 软件 gates 替代。

#### Risks
Settings 的真实 DPI / 屏幕高度布局需要同候选窗口验证；F699 硬件截图、实机混合 DPI、多屏拓扑仍未验收。活动 / Shelf / Glass 实际合成等全方案范围继续 pending。

#### Needs from other agents
A0 冻结导航适配并提供新 SHA；A6 对同一 SHA 实测页导航、横向滚动、dpi/window resize、播放器 motion 与退出状态。

#### Suggested next step
对下一冻结候选重新进行独立源码复核，并只按该候选的 SHA 对应门禁更新本节后续计数。

---

## 增量审查：D060EDFA 候选（A3/A4 修复后）

本节只针对 SHA-256 `D060EDFA55D189766F41AB4DF48EF665F6E84BE65AF6759182B99D96C7F0974E`，不继承 F699 或 D86 的运行时结果。已只读核对 `target/ui2-next/release/isle-native.exe` 实际哈希与 A0 提供值相同。A0 报告本候选 `fmt`、workspace `check`、`clippy -- -D warnings` 与 112 项测试通过，另 1 个 opt-in network test ignored；release 已成功。A6 对同一 SHA 的 Settings placement、V2 `ui_v2.py` 与 motion 软件回归现已 PASS；legacy/demo UIA 兼容回归的严格目标窗脚本已完成同 SHA 复跑并 PASS。截图、真实硬件 DPI 和 V2 专用 UIA 证据仍未完成，本节不构成整个方案最终验收。

| 严重度 | D060 A3/A4 增量开放数 | 结论 |
|---|---:|---|
| BLOCKER | 0 | 本轮静态源码复核未发现确定 BLOCKER。 |
| HIGH | 0 | 本轮静态源码复核未发现确定 HIGH。 |
| MEDIUM | 0 | D86 的短窗口导航缺陷已修复；D060 同 SHA placement 软件场景通过，真实硬件 DPI / 截图仍 pending。 |
| LOW | 0 | D86 全 capability 撤回时的不可见控件 Spring 已修复；D060 同 SHA motion / idle 软件场景通过。 |

D060 在本次 A3/A4 增量源码范围的静态开放数为 `0 / 0 / 0 / 0`，**不表示整个候选没有 LOW**。全候选已知开放项为 `0 / 0 / 0 / 1`：F699 中记录且当前代码仍存在的未使用 Glass blur 构建。活动与 Shelf 的真实集成、硬件截图等属于尚未完成的方案范围和验收风险，另在本节后文列明，不把它们改记为源码缺陷。

### A3 静态复核

Full Player 控件能力从媒体 snapshot 布局生成，三槽 rect Spring 在 `layout_snapshot()` 中供 draw 与交互几何共用。Expanded 状态的 hit regions / controls 与当前动画 Rect 一致；Compact 退出时 outgoing visual 几何可继续绘制，而 interactive `controls()` 和 hit regions 已清空。Renderer 消费控制组 opacity，进度与时间使用 progress opacity。全部 capabilities 撤回时三个 rect slot 归位，`controls_opacity` 立即归零，不再靠不可见 Spring 维持 `moving()/continuous()`；新测试 `ui_v2_revoking_all_player_capabilities_does_not_animate_invisible_controls` 覆盖非 Reduced 情况。当前源码静态闭环 D86 LOW。A6 同 SHA motion 软件回归现已通过：四边×96/120/144/168/192 合成 DPI 场景和 30 次快速展开/收起/反向后均进入 `continuous=false`、`timerRunning=false`；纯 Spring 测试验证精确 20/50/80% 反向点。结果 JSON 未记录截图验收，HWND 反向由时间采样覆盖，故不把它表述为像素级视觉验收。

### A4 静态复核

`navigation_layout(client_height)` 成为导航唯一几何来源。常规高度保持六行旧坐标；中等高度以不低于 28 DIP 的六行布局压缩；短高度隐藏侧栏标题并改为 2×3 按钮。Direct2D active highlight 与原生 owner-draw HWND 都消费同一组 Rect。`WM_SIZE` 与 `WM_DPICHANGED` 最终通过 `position_controls()` 重排导航并更新字体；六个导航按钮都保持启用，点击当前页由 `set_page()` 早退，当前 index 的 active highlight 与 owner-draw 状态同步。模型边界用例覆盖 426 DIP 正常布局、270/320 DIP 六行压缩及 220 DIP 的 2×3 紧凑布局，同时断言按钮非零、位于 sidebar 与 client 高度内且互不重叠。静态检查没有发现 active highlight / native bounds 的算法分叉。

位置存储与恢复仍按 DIP 外窗大小、物理坐标和目标显示器当前 DPI 工作；未知 placement 嵌套字段更新时保留。manual move/size 只在结束时产生最终 placement patch，退出阶段补取仍在进行中的有效变更后与普通 settings 快照共用有限 flush。横向滚动将 Direct2D 内容和原生子控件一起平移，子控件区域裁到内容 viewport；DPI 切换刷新 Shell DPI、字体和控件坐标。上述路径静态未见确定数据丢失或错误坐标换算。

### 本候选独立动态门禁状态

A6 已对本 SHA 的 Settings placement 场景通过 7 项软件回归：默认避让与无可用侧隙时居中回退、owner 侧有空间时避让、离屏 / 坐标溢出恢复不覆盖原值、手动移动尺寸保存并重启还原、保留未知及无关配置字段、父窗口退出时 flush 未完成的手动修改，以及合成 DPI 窄布局导航 / 横向滚动 / 键盘遍历。所有 owned 进程均正常退出且未强杀。结果位于 `artifacts/ui-v2-placement/results.json`，SHA 匹配本候选。第 7 项申请 1366×690 物理视口时，当前真实工作区与最小跟踪尺寸将窗口限制到 1466×1032；脚本保留了这个实际尺寸，并用合成 400% DPI 得到 244 DIP 客户区验证 2×3 导航，随后恢复合成 200% 字体与滚动验证。它证明软件布局路径，不代表真实 OS DPI / 工作区变化。

A6 同 SHA 的 motion 结果 `artifacts/ui-v2-motion/results.json` 有 21 个场景：四边与 96/120/144/168/192 合成 DPI 组合，以及 30 次快速展开 / 收起 / 反向和 idle 检查。反向后的快照为 `continuous=false`、`timerRunning=false`。精确 20/50/80% 位置由纯 Spring 测试覆盖；实际 HWND 场景采用时间采样。此 artifact 明确 `screenshotsVerified=false`、`realHardwareDpiVerified=false`。

A6 同 SHA 的 `ui_v2.py` 也已 PASS，owned 进程正常退出、exit code 0 且未强杀；结果记录于 `artifacts/ui-v2-d060-history/results.json`。这些软件门禁关闭本候选 A3/A4 MEDIUM 与 LOW 的动态项；D060 增量计数为 `0 / 0 / 0 / 0`。V2 专用 Settings / Spring provider UIA 尚无同 SHA 证据；legacy `accessibility.ps1` 兼容回归不能替代它。屏幕阅读器、目视截图、实际 OS DPI / 多屏等也仍 pending。

D060 上一次 `accessibility.ps1` 完成了 UIA/MSAA 交互断言，但在 finally 发 `WM_CLOSE` 后 5 秒进程仍存活，脚本没有生成 success artifact；这次运行不作为 PASS。A6 随后的只读 HWND 探针确认，同 PID 下 `Get-Process.MainWindowHandle` 曾解析为 `UAC Input Indicator` helper，真实 `IsleNativePrototype` 主窗 HWND 不同。旧运行未记录 finally 实际使用的 HWND，因此不能断言 timeout 是错发消息，也不能将其归因于产品关闭路径。A6 另以精确主窗类 HWND 验证了诊断消息、`WM_CLOSE` 与正常进程退出。更新后的隔离脚本现按 owned PID 枚举唯一 `IsleNativePrototype`，每次 UIA / 诊断 / 关闭前均校验 `IsWindow`、PID 与窗口类，并检查 `PostMessage` 返回值；同时将设置 fixture 写为 UTF-8 无 BOM，并在任何可写 UIA 操作前要求新鲜诊断 `configurationValid=true` 且完全没有音频 / 媒体服务字段。A6 以该脚本重跑同 SHA 后 PASS：`artifacts/accessibility-results.json` 记录 PID 29892、main HWND 1446544、配置前后有效且音频 / 媒体服务字段缺席、8 个 descendant、Invoke、RangeValue 设为 67 / 拒绝 101、MSAA 名称角色焦点边界值、stale provider、缩放点击与折叠释放通过；main HWND 已验证，WM_CLOSE 后 48 ms 正常退出，exit code 0，未强杀。该结果关闭 D060 legacy/demo UIA 兼容门禁，但不代表 V2 专用 Settings / Spring provider UIA。

同 SHA 的旧 `accessibility.ps1` 未启用 `--ui-v2`，只能证明兼容 / 隔离安全回归，不能作为 V2 spring control provider bounds 或 Settings UIA 的实测证据。V2 当前控件矩形在源码中与绘制 / hit 几何共用；运行时 provider / 屏幕阅读器语义仍需专用测试，暂列未验收。

Live Activities 和 Shelf 仍只有 model / registry，Glass bitmap 仍未接入真实折射合成；无媒体会话、屏幕阅读器、实机多屏 / 异 DPI、截图目视和睡眠恢复也没有全方案证据。即使 A6 此候选的软件回归通过，也不能标记整个 UI 2.0 方案正式完成。

### D060 Result

#### Scope
独立只读复核 A3 Full Player controls opacity / Spring / draw / hit / UIA 几何与 A4 设置窗口导航 / placement / narrow-DPI 适配；本节按 SHA D060EDFA 单独计数。

#### Files changed
只在 `docs/native-ui-v2-review.md` 追加本节；未改业务源码。

#### API changes
无。`navigation_layout` 是应用内设置 UI 几何 helper，不增加外部 API；A3 opacity 修复保持既有 public model API。

#### Behavior
A3 全 capability 缺失时即刻清除不可见控件 motion；A4 导航随 client 高度切换正常 / 压缩 / 紧凑布局，并由同一几何驱动 Direct2D 高亮与原生按钮。配置保真、负坐标及目标 DPI 换算、人工 move/size 与退出 flush 静态复核通过。

#### Tests
本 Reviewer 未运行 build 或 UI。A0 报告 fmt/check/clippy warnings-as-errors、workspace 112 passed + 1 opt-in network ignored、release 成功；只读核实候选二进制 SHA 与报告一致。A6 同 SHA 的 placement 7 项、motion 21 项与 `ui_v2.py` 结果均 PASS，分别记录于 `artifacts/ui-v2-placement/results.json`、`artifacts/ui-v2-motion/results.json`、`artifacts/ui-v2-d060-history/results.json`。结果覆盖软件几何 / DPI / 反向 / idle / 设置配置保存路径，但明确未验证截图和真实硬件 DPI。D060 legacy/demo UIA 首轮进程关闭门禁失败且未有成功 artifact；其后精确目标窗探针确认真实主窗可响应并正常退出，strict harness 的完整同 SHA 重跑已 PASS，结果见 `artifacts/accessibility-results.json`。V2 专用 UIA 也尚未提供。

#### Risks
同 SHA 软件导航、resize / 合成 DPI、水平滚动与 motion 已通过；但没有实机 DPI / 多屏 / 工作区变更、截图目视、V2 专用 Settings / Spring provider UIA、物理屏幕阅读器或睡眠恢复证据。活动 / Shelf / Glass 真实实现继续 pending。

#### Needs from other agents
A6 已用已校验的实际主窗 HWND 完成 D060 同 SHA 的 legacy/demo UIA 兼容脚本重跑并保留自然退出证据；V2 专用 Settings / Spring provider UIA 仍单独标为未验收。如业务代码有修复则需新 SHA 重新门禁。继续保留真实硬件 DPI、多屏、截图与睡眠恢复作为未完成的方案级验证。

#### Suggested next step
在候选级报告中记录 placement / `ui_v2.py` / motion 软件回归已通过；legacy/demo UIA 兼容回归已通过严格 HWND helper 的自然退出复跑。全方案仍需 V2 专用 UIA、实机 DPI / 多屏、截图、屏幕阅读器、睡眠恢复，以及 Activities / Shelf / Glass 的实际实现与验证，不能以当前开发构建宣告正式完成。

## E6 Manifest 前置包增量审查

本节只审查 manifest 前置改动及其独立候选 SHA-256 `E6DD5BACDC6F7CACD8FA5F24110BC63C72087F0EBB3EE5E20AAAADAB2C90A4E7`，不继承 D060 的源码或 UI 结果。已独立核对 `target/ui2-controls/release/isle-native.exe` 的 SHA 与 A0 报告一致。此包仅是 Common Controls v6 / Per-Monitor V2 activation 前置，不包含完整原生设置控件，不得据此标记 Phase 2 完成。

| 严重度 | E6 manifest 增量开放数 | 结论 |
|---|---:|---|
| BLOCKER | 0 | 静态复核与现有候选资源证据未发现确定 BLOCKER。 |
| HIGH | 0 | 同 SHA legacy/native 兼容动态证据通过；V2 控件与专用 provider 门禁仍待后续代码包。 |
| MEDIUM | 0 | 未发现 manifest 前置包自身的确定 MEDIUM。 |
| LOW | 0 | 本增量未新增 LOW；全候选已知 backlog 仍有 1 项未接入真实合成的 Glass blur。 |

### E6 Result

#### Scope
独立只读复核 `build.rs` 的 Windows 资源嵌入、`isle.manifest` 的 Common Controls / DPI / execution level 声明，以及 `manifest_controls.py` 的 PE 资源检查与隔离 activation-context 探针；运行证据只绑定 E6 SHA。

#### Files changed
业务包审查范围为 `crates/isle-app/build.rs`、`crates/isle-app/isle.manifest` 和 `scripts/manifest_controls.py`。本 Reviewer 只追加本审查文档，没有修改业务源码、构建脚本或二进制。

#### API changes
无外部 API 变更。build script 在 Windows 资源脚本中加入唯一 RT_MANIFEST 类型 24 / ID 1，同时保留原 ICON 与 VERSIONINFO 资源；现有 MiSans 字体、字体许可证文件的构建输出路径与复制策略保持。`rc_quoted_path` 对路径分隔符和引号进行处理，并对唯一 manifest resource 行作断言。

#### Behavior
Manifest 声明 `Microsoft.Windows.Common-Controls` v6、PerMonitorV2 并保留 `true/pm` 兼容声明，requested execution level 为 `asInvoker` 且 `uiAccess=false`。静态检查未见覆盖现有图标、版本信息或字体打包的确定缺陷。检查器把候选 PE 作为资源映像读取；它不启动目标程序，但在自身进程创建并激活 ACTCTX、加载解析出的 ComCtl32 并调用 `DllGetVersion`。该探针只证明检查器进程内的 activation 解析；目标 EXE 的 runtime、custom draw 与视觉状态不由它证明。脚本注释与 README 已将此探针限定为可信本地构建。

#### Tests
本 Reviewer 未运行 build 或 UI。A0 报告 fmt、check、clippy `-D warnings` 和 workspace 112 tests passed、1 个 opt-in network test ignored，E6 release 成功；本人只读核对 EXE SHA。`artifacts/manifest-controls/results.json` 的同 SHA 记录含单一 manifest resource ID 1、Common Controls identity 6.0.0.0、PerMonitorV2 与 legacy DPI 声明、asInvoker、icon resource 62 bytes、version resource 488 bytes，以及检查器实际解析到 WinSxS 的 ComCtl32 v6。该检查器记录 `targetApplicationLaunched=false`、`targetRuntimeVersionVerified=false`、`screenshotsVerified=false`。它只检查图标组资源存在及 VERSIONINFO 资源非空，没有逐项验证 ICO 中每个图像或各版本字符串字段。

同 SHA 的 `artifacts/native-settings-results.json` 另有运行时兼容证据：宿主、Settings 与重启宿主的 DPI context 均为 PMv2；ComCtl32 实际路径位于 WinSxS，host/Settings restart 路径一致；两次 owned 进程均 graceful close、exit code 0、未强杀。这是 legacy/native 设置兼容和运行时 activation 证据，不是新 V2 设置控件或 V2 UIA 验收。

随后 A6 对同 SHA 的 V2 `ui_v2.py` 软件门禁通过，证据为 `artifacts/ui-v2/results.json`（D060 旧 artifact 已备份至 `artifacts/ui-v2-d060-history`）。结果覆盖 debounced/coalesced 保存、unknown 字段保留、inspection 离开 Appearance 后解锁、外部配置冲突保护与 revert、退出 debounce flush 后重启恢复，以及 30 次真实设置窗口 open / 切页 / resize（WM_SIZE）/ invalidate+paint（WM_PAINT）/ close。每轮关闭后诊断均报告 `settingsWindowAlive=false`；5 次预热后 GDI baseline 为 54，30 次关闭样本范围 2，满足脚本的 ±4 稳定断言。静态核对脚本发现关闭窗口后期待存活状态消失；各完整主进程场景均在 `finally` 关闭后断言 `returncode == 0`，5 秒超时分支会 terminate 并抛错，因此该分支不会生成成功 artifact。候选 EXE 的 SHA 与 artifact 一致。该结果关闭 E6 的 V2 Settings lifecycle 软件门禁，但 `screenshotsVerified=false`，不构成截图、硬件 DPI、物理屏幕阅读器或完整方案验收。

Timer 原生控件补充门禁现已通过严格复测，canonical evidence 为 `artifacts/manifest-timer-controls/results.json`，SHA 与 E6 一致。PID 28860 的同进程 Timer 窗口与唯一 Edit ID 300 经 class / PID 校验；脚本用稳定 UTF-16 `WM_SETTEXT` 写入，并用 `WM_GETTEXT` 读回 Edit buffer `25`。严格断言要求启动与暂停的 fresh diagnostics 落在 1495–1500 秒、reset 精确为 1500 秒；artifact 记录 `1499.99999 / 1499.98296 / 1500`。demo 配置有效且无 audio/media service fields；host / Timer 经校验后 graceful `WM_CLOSE`，47 ms 后 exit code 0、未强杀。旧 PID 33568 的 30 分钟误通过已撤回并保留在 `partial-false-positive-30-minute.json`；两次早期 helper 失败仍分别保留，归因为临时 LPWSTR 指针与单次快照时序，均没有确证产品缺陷。该最终证据关闭 E6 Timer 控件 smoke 补充门禁，不抹去先前错误结果的审计记录。

#### Risks
本节增量开放数为 `0 / 0 / 0 / 0`；不得与 D060 增量计数混用。整个候选已知 LOW backlog 仍为 `0 / 0 / 0 / 1`（未接入真实合成的 Glass blur）。PE inspector 的 ACTCTX 探针会在 inspector 进程加载并执行 ComCtl32，故仅应对可信本地构建使用；它不会执行目标 EXE。该探针与 legacy/native runtime 证据均不覆盖完整 V2 控件的绘制、UIA、截图、真实硬件 DPI 或屏幕阅读器。Activities / Shelf 的模型与集成、Glass 实现及硬件目视验收仍属于未完成方案范围，不据此虚报为源码缺陷或正式通过。

#### Needs from other agents
E6 的 manifest 资源、legacy/native 兼容、V2 Settings lifecycle 和 Timer 控件 smoke 门禁均已有同 SHA 证据。后续完整控件源码和二进制必须使用独立新 SHA、独立计数和 V2 专用 UIA / 视觉验收；不得把 A4 controls WIP 或后续 fault hooks 混入 E6 结论。

#### Suggested next step
E6 已关闭 manifest 资源、legacy/native 兼容、V2 Settings lifecycle 与 Timer 控件 smoke 软件门禁；下一步应以独立新候选评审完整 V2 设置控件。全方案在真实 V2 控件、专用 UIA、截图、硬件 DPI / 多屏及 Activity / Shelf / Glass 实现完成前仍不得宣告正式完成。

## Native Settings controls frozen-candidate source review (94E3)

本节只记录冻结候选的有限源码复读，不沿用 E6 manifest-only 的计数或运行时结论。候选 release binary SHA-256 为 `94E33D2E6C56E64A9C54CE86D76B4899B862239266A7934DDCD312739AC01930`。复读范围为 `docs/native-ui-v2-settings-controls-plan.md`、`crates/isle-app/src/settings/{window,controls,render,model,mod}.rs` 以及 `crates/isle-app/src/main.rs` 的控件诊断输出；本 reviewer 未运行 build、GUI 或动态测试，也没有编辑业务源码。此节不是全候选最终计数或 UI 验收结论。

在本次限定路径中，未发现新的确定源码 blocker。`ThemeHandle` 本身是 raw-pointer wrapper，不能独自证明没有别名或重入；当前关键调用点通过短块复制 HWND、控件映射、字体、DPI、滚动位置及绘制状态，再进入 `SendMessageW`、`SetWindowPos`、`ShowWindow`、`EnableWindow`、`SetFocus`、销毁或公开同步路由。`position_controls` 在同步窗口操作前克隆 placement / 导航快照；`sync_theme` 在消息调用前克隆时区与形状映射，字段缓存更新也没有跨调用的借用。`WM_DPICHANGED` 先应用窗口建议位置，再分块更新主题并切换子控件字体，最后以新 DPI 再定位。该判断针对当前实现，不消除以后改动引入重入别名的风险。

高对比颜色路径在当前冻结源码中覆盖两种 renderer 状态：painter 存在时使用已刷新的真实系统状态（或测试合成状态）；painter 为 `None` 时，`WM_CTLCOLOR*` 直接查询 `SPI_GETHIGHCONTRAST`，为 EDIT / STATIC / BTN / LISTBOX 返回系统窗口色与文本色。创建 painter 失败不会因此把固定深色 palette 套到原生输入控件。

Dropdown subclass 在 `DefSubclassProc(WM_PAINT)` 后通过当前 HWND 的 HDC 全客户区清理并完整绘制闭合字段、边框和箭头；`COMBOBOXINFO.rcItem` / `rcButton` 按已经核验的 client 坐标直接使用，没有再次做 screen-to-client 转换；所选文字绘制与 chrome 绘制分开，箭头只由全客户区绘制一次。鼠标移动设置 `TME_LEAVE`，离开、焦点和启用状态变化触发失效。成功画完 chrome 才增加独立 `dropdown_chrome_paints` 计数。`GetDC` 失败不在当前 WM_PAINT 中直接重试，而是投递去重的延迟恢复；恢复在执行时采集 selection、enabled、visible、focus、当前字体、DPI、位置与 item 文本，再替换 HWND 并刷新映射。恢复创建失败有失败 ID 门闩，避免无界重试。该结论是代码路径复读；未证明各 Windows 版本上的实际 ComboBox 消息序列、像素结果或 UIA 行为。

DC painter 每个调用都绑定当前 HDC / RECT，使用 `DXGI_FORMAT_B8G8R8A8_UNORM` 与 `D2D1_ALPHA_MODE_IGNORE`，并清理当前绘制边界。连续 target-lost 由有限策略限制为首次重建、再次禁用；一次成功 `EndDraw` 后才清除连续失败状态。Native fallback / deferred combo recovery 的源码分支存在，但这轮没有进行运行时故障注入。

Root 报告冻结候选的格式 / clippy 检查、115 项测试通过（1 项 opt-in test ignored）及 release build 已完成；本 reviewer 没有重跑这些命令。键盘状态矩阵、UIA / MSAA、IME、真实窗口视觉截图、实际 OS DPI / 多屏以及目标丢失 / ComboBox 恢复故障注入仍应按同一 release binary SHA 单独留证；在这些门禁完成前，不据此宣称设置控件实机或无障碍验收完成。

### Artifact path correction for the earlier E6 record

E6 的 Settings 软件结果保存在 `artifacts/ui-v2-e6dd-history/results.json`；canonical `artifacts/ui-v2/results.json` 后续用于当前候选时，必须检查文件内 `binarySha256`，不能仅凭路径或旧段落中的路径描述把 94E3 证据归给 E6。当前 `artifacts/manifest-controls/results.json` 的 `binarySha256` 是 94E3；未找到独立 E6 inspector 结果副本。因此 E6 历史资源审查保留为历史记录，但当前 canonical manifest artifact 不能再作为 E6 同 SHA 的 inspector 证据。

### Result

#### Scope
针对 94E3 候选的 Settings native-control 源码有限复读，仅覆盖 ThemeHandle 重入边界、真实与无 painter 高对比路径、ComboBox closed-field/chrome / 坐标 / hover-leave / 延迟恢复，以及 Direct2D target 的有限恢复策略。

#### Files changed
只在 `docs/native-ui-v2-review.md` 追加本节与 E6 artifact 路径更正；没有修改业务源码或 QA artifact。

#### Tests
本 reviewer 未运行 build、测试或 UI。检查报告由 root 提供：格式 / clippy、115 项通过与 1 项 ignored、release 完成；binary SHA 为 94E3。以上不能替代同 SHA 的 Windows 状态矩阵、截图、UIA / MSAA、键盘、IME 或故障注入证据。

#### Risks
本节不给 BLOCKER / HIGH / MEDIUM / LOW 最终计数，也不把静态复读等同于 PASS。上述动态与目视门禁仍 pending；E6 inspector artifact 目前缺少可核验的独立同 SHA 副本。
