# Isle Native UI 2.0 QA Record

更新日期：2026-10-03  
范围：Sprint 1 / 2 集成候选与 P0 回归。活动管理和 Shelf 当前只有模型或注册能力；本报告不把它们描述为已完成 UI。

## 固定候选和环境

- 本轮固定测试二进制：`target/ui2-candidate/release/isle-native.exe`
- SHA-256：`F699F378415C94D63055285CAE38369F322FA2E0AF3890A8142ACA763C549942`
- 所有 Python UI 脚本使用 `C:\Users\admin\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe`（Python 3.12.14），每条 UI 命令先设 `ISLE_TEST_MONITOR='\\.\DISPLAY2'`，再设 `ISLE_TEST_EXE` 指向上述候选。PowerShell UIA 另以 `powershell.exe -NoProfile -ExecutionPolicy Bypass` 启动；只影响该子进程，不更改系统执行策略。
- A0 报告此候选通过 `cargo fmt`、`cargo clippy -- -D warnings`、workspace 91 tests（另 1 项联网测试 ignored）及 release 构建。以上 build 结果由 A0 提供，本 A6 回合未重编。
- 结果和配置 fixtures 均在 `artifacts/`。测试未发送真实播放、音量、静音或设备切换命令，未改全局 DPI、安装配置或硬件设置；用户预览实例未被测试脚本控制或关闭。
- 主机未发现 GSMTC 媒体 session；Legacy 日志为 `managerAlive=false`、`polls=0`、`error=false`。V2 / UIA 使用离线 fixture 或 `--demo`，无真实音频或媒体服务。
- 本节为 F699 当时的历史结果；后续候选复用了若干固定输出文件名。现存的 F699 原生设置结果副本为 `artifacts/f699-native-settings-results.json`，F699 accessibility 副本为 `artifacts/f699-accessibility-*`，玩家结果仍含 F699 SHA。F699 独立 V2 lifecycle JSON 副本没有找到，表格保留当时执行结论与摘要数值但不伪造备份路径。当前 D060 V2 设置结果保存在 `artifacts/ui-v2-d060-history/`，motion 与 placement 结果仍可通过各自 JSON 中的 SHA 核验；早期候选已覆盖的文件不由当前 canonical 路径代表。

## 执行结果

| 顺序 | 场景 / 命令 | 结果 | 证据与范围 |
| --- | --- | --- | --- |
| 1 | `scripts/ui_v2.py` | **PASS（F699 历史结果）** | F699 当时覆盖即时 runtime/persisted 保存、未知字段保真、Appearance inspection unlock、外部文件冲突/安全 revert、退出 flush 与重启恢复，以及 Settings 窗口 30 轮生命周期；该次记录的 GDI 句柄为 49–51，跨度 2。F699 的独立 V2 JSON 结果已被后续候选覆盖，当前 `artifacts/ui-v2/results.json` 属于 E6DD，不能作为 F699 文件证据。 |
| 2 | `scripts/ui_v2_players.py` | **PASS** | `artifacts/ui-v2-players/results.json`。Allow All 即时应用并落盘；关闭后恢复 OfflineA 手动选择；勾选 OfflineB、上下排序即时保存；关窗重开及进程重启均恢复选择和优先级；外部冲突后 revert 保留外部文件。覆盖无 Save 按钮、关闭和 conflict/revert 路径。系统媒体会话只读检查；未发送媒体控制。 |
| 3 | `scripts/native_players.py --no-screenshots` | **PASS（Legacy 兼容）** | `artifacts/native-players-results.json`。手动/全部/空选择、排序、重启与 12 轮旧式对话框关闭通过。Win32 进程采样 private memory 约 89.49→89.60 MiB、handles 稳定为 892。此 host 无系统媒体 session，所以真实 session 发现/选择未覆盖。 |
| 4 | `scripts/ui_v2_motion.py` | **PASS（合成输入）** | `artifacts/ui-v2-motion/results.json`。21 个场景：96/120/144/168/192 合成 DPI × 四边、共享封面几何与合成 seek，以及 30 次展开/收起/快速反向压力。idle 前后约 2 秒内 frames 保持 4010、Region updates 保持 2060、timer 为 0。该次短样本 `renderCpuP95Ms=0.9145`、`regionCpuP95Ms=0.6541`、`artworkUploadMs=0.1692`、`blurBuildMs=1.857`。20/50/80% 精确反向由 Spring 模型测试覆盖；HWND 使用时间采样。 |
| 5 | `scripts/configuration_faults.py` | **PASS** | `artifacts/configuration-fault-results.json`。损坏 JSON、错误类型、超限、有效未知字段及未知时区五类输入保护通过；测试文件保持预期且天气请求为 0。 |
| 6 | `scripts/native_settings.py --no-screenshots` | **PASS（功能 / 几何）** | F699 原始副本：`artifacts/f699-native-settings-results.json`。设置切换、草稿取消、保存/重启保真、外观/时区和副屏工作区边界检查通过；12 个窗口边界和 7 个时钟场景均有结果。截图和视觉外观未验证。 |
| 7 | `scripts/interaction.py --no-screenshots` | **PASS** | `artifacts/interaction-results.json`。floating/attached × 四边共 8 路 Region、取消手势不折叠、透明角落跨进程命中路由通过。抓屏关闭，未做视觉检查。 |
| 8 | `scripts/display_lifecycle.py` | **PASS（合成显示条件）** | `artifacts/display-lifecycle-results.json`。32 个合成 DPI/布局场景，以及隐藏、倒计时、最小化和恢复检查通过。不是物理显示器、异 DPI 设备或睡眠恢复验收。 |
| 9 | `scripts/lifecycle.py` | **PASS（最终重跑）** | `artifacts/lifecycle-results.json`。30 个 owned 进程/窗口均正常退出、`returncode=0`；页面、timer 与字体释放检查通过。clock 边界 timer 为 32634 ms、`intervalSamples=0`。 |
| 10 | `powershell.exe -NoProfile -ExecutionPolicy Bypass -File scripts/accessibility.ps1` | **PASS（隔离 demo）** | `artifacts/accessibility-results.json`。UIA Invoke 导航、RangeValue 0–100 / 设置 67 / 拒绝 101、MSAA 名称和 role、焦点、边界、值、stale provider、缩放点击及折叠后子节点释放均通过。新鲜的初/末诊断都没有 `audioPolls`/`mediaPolls` 服务字段，字段缺席按“服务未创建”验证，poll 数记为 `null`。owned PID 24412 通过 `WM_CLOSE` 在 29 ms 退出，exit code 0，`forcedKill=false`。 |

## 失败尝试及处理记录

- 修复前候选 SHA-256 `171E85639D70CF3205DB97B3217C0401C96F6A1A6E6278DD93BCE3B0373D53F0` 在 F8 设置窗场景失败，出现同进程 Settings 窗口后诊断不再更新；A0 修复 Theme 所有权缺陷并生成本报告固定候选。旧候选失败未计入新候选通过结果。
- V2 player 首轮 helper 强行点击当前已 disabled 的 Media 导航，测试假设错误；改为识别当前页并跳过该点击后完整通过。业务控件保持原样。
- Legacy player 首轮受 mandated Python runtime 缺少 `psutil` 阻塞；脚本改用只读 Win32 进程采样，没有安装软件。随后确认此 host 没有真实 Media session，移除对 `mediaPolls>0` 的错误假设；最终只对离线选择/配置功能给出 PASS，真实会话场景保持未覆盖。
- Lifecycle 首轮在 0-based index 8 的 clock 项关闭时超过 5 秒，helper 随后终止了该 owned 子进程；该次未记录 PID，也没有 `lifecycle-8.json`，所以根因未知，不能宣称为应用崩溃或确定的 IME 错窗。等价单例 clock 重现使用 PID 2092、HWND class `IsleNativePrototype`，fresh snapshot 的 `elapsedSeconds=0.4410`、`timerIntervalMs=0`，`WM_CLOSE` 后 exit code 0。之后 helper 改为按窗口 class 精确匹配，完整 30 轮 lifecycle 重跑通过。首次失败保留在本记录中。
- interaction 的精确 class helper 首次只允许主窗 class，导致测试自身的 `IsleCrossProcessProbe` 被漏选；本轮唯一 probe PID 18656/HWND 1638748 被识别后通过 `WM_CLOSE`、exit code 0 清理。helper 增加显式 `expected_class`（主窗默认 `IsleNativePrototype`，probe 明确传 `IsleCrossProcessProbe`）后重跑 8 路通过。没有把首次 helper 失败算作候选应用缺陷。
- UIA 旧尝试在 `--benchmark` 下得到 `ElementNotEnabledException`；测试在 Invoke 前停止，没有执行 RangeValue 写入。根因是 benchmark 禁用了 host。脚本移除 `--benchmark`，保留 `--demo`、隔离设置与日志，并在每次值写入前后 fail-closed 检查服务字段缺席；新增 WM_CLOSE / exit code / 无强杀断言后重新完整通过。旧脚本未对真实系统音量写值。

## 性能、视觉与平台限制

- V2 motion 的最终 stress 快照证明本次短 idle 窗口内没有新增 frames 或 Region updates；样本用于回归，不代表 60/120 Hz 硬件稳定性，也没有做单独连续 60 秒 V2 idle 验收。`visibility_resources.py` 是 Legacy 路径的可选长时采样，没有用它冒充 V2 证据。
- 所有相关结果中的 `screenshotsVerified=false` 或使用 `--no-screenshots`；当前抓屏环境曾报告 screen grab failed，所以 V2 外观、透明折射、对比度和控件视觉仍未验收。
- 96–192 DPI 和显示器切换均为软件合成。没有真实硬件 DPI/异 DPI 多屏、主屏物理切换、睡眠恢复或读屏器实机证据。UIA/MSAA provider 回归通过不等于屏幕阅读器完整体验已通过。
- 活动/Shelf、Glass 合成、系统媒体会话、实际播放/seek 控制和跨重启窗口位置不在本轮已完成结论中。

## Result

### Scope

对 SHA `F699F378415C94D63055285CAE38369F322FA2E0AF3890A8142ACA763C549942` 固定 release 候选执行 Sprint 1 / 2 的隔离 UI 回归、配置保护、DPI/生命周期、UIA/MSAA 与性能采样。非固定候选的后续工作树变化不属于本轮二进制结果。

### Files changed

`docs/native-ui-v2-qa.md`；`scripts/ui_v2.py`、`ui_v2_players.py`、`ui_v2_motion.py`、`interaction.py`、`accessibility.ps1`、`configuration_faults.py`、`native_settings.py`、`native_players.py`、`display_lifecycle.py`、`lifecycle.py`、`visibility_resources.py`、`measure.ps1`。脚本增加候选二进制选择、隔离/回归覆盖或可靠的 Win32 采样；interaction 窗口发现现按预期 class 定位。

### API changes

无应用 API 或业务代码更改。

### Behavior

固定候选在 V2 设置实时保存/冲突恢复、玩家选择与顺序、Legacy 选择兼容、布局命中、合成 DPI、配置错误保护、生命周期与隔离 UIA/MSAA 回归中通过；最终通过的各场景均满足其 owned-process 正常退出断言。

### Tests

表格中 10 个场景均在最终适用版本上 PASS。初始失败及 helper 修复见“失败尝试及处理记录”。全部修改的 Python QA 脚本通过 bundled Python `py_compile`；accessibility.ps1 通过 PowerShell AST 解析。A0 提供的 fmt/clippy/workspace tests/release 构建结果见环境部分。

### Performance

V2 stress 样本记录 render、Region、upload、blur 指标及静态 idle 前后 counters。GDI 30 轮稳定区间跨度 2。未执行可选 Legacy visibility 长采样，也不将短样本解释为显示硬件帧率验收。

### Known Issues

真实系统媒体 session、硬件 DPI/多屏/睡眠、截图视觉和读屏器实测缺失；相关功能不可据此宣称已完整。活动/Shelf/Glass 等待后续工作包。首次 lifecycle timeout 根因未知，最终回归通过不能抹除该环境/测试失败记录。

### Needs from other agents

A7 当前独立审查 `docs/native-ui-v2-review.md` 已确认 BLOCKER = 0、HIGH = 0、MEDIUM = 2、LOW = 1；UIA 安全 HIGH 已闭合。本固定候选没有待补的 UIA 复测依赖。后续 placement 若产出新候选，需按新 SHA 单独安排适用 QA。

### Suggested next step

继续下个 placement 工作包；如产生新候选，单独记录其 SHA 并执行适用回归。当前审查为 BLOCKER = 0 / HIGH = 0 / MEDIUM = 2 / LOW = 1，但真实硬件/视觉验收及其余待办范围仍未完成，因此不把整个 Isle Native UI 2.0 标记为企业级正式验收完成。

## D060 增量候选补充（2026-10-03）

本节记录 placement、A3 导航布局和 accessibility harness 修正后单独验证的下一候选，不覆盖或改写上面的 F699 历史结果。

- 固定二进制：`target/ui2-next/release/isle-native.exe`
- SHA-256：`D060EDFA55D189766F41AB4DF48EF665F6E84BE65AF6759182B99D96C7F0974E`
- A0 提供此 frozen source 的 `cargo fmt`、`cargo check`、`cargo clippy -- -D warnings`、workspace 112 tests（另 1 项联网测试 ignored）及 release build PASS 结果；A6 未重编。
- 下表仅为该 SHA 实际重跑的增量场景；F699 已通过的其余套件不冒充 D060 复测。

| 场景 / 命令 | 结果 | D060 证据 |
| --- | --- | --- |
| `scripts/ui_v2_placement.py` | **PASS（软件合成布局）** | `artifacts/ui-v2-placement/results.json`。7 个场景覆盖默认/可避让空间摆位、离屏及坐标溢出恢复且不写回、移动/缩放后关闭重开和进程重启恢复、父窗关闭时 flush manual dirty、未知嵌套字段保留；另检查导航 bounds、六页按钮可达、400% 合成 compact 2×3 导航、字体、横向滚动与 Tab/End 键盘路径。所有 owned 进程 `returncode=0` 且未强杀。 |
| `scripts/ui_v2.py` | **PASS** | D060 副本：`artifacts/ui-v2-d060-history/results.json`。即时 runtime/persisted 保存、未知字段、inspection unlock、外部冲突与安全 revert、退出 flush/restart；设置窗真实 `SetWindowPos` / `WM_SIZE`、`WM_PAINT` 后反复打开/切页/关闭 30 次。预热后关闭句柄为 49（此前基线 51），最后设置窗不存在。 |
| `scripts/ui_v2_motion.py` | **PASS（合成 DPI / 输入）** | `artifacts/ui-v2-motion/results.json`。21 项覆盖 96/120/144/168/192 合成 DPI × 四边、共享专辑封面、seek 与 30 次展开/收起/快速反向。约 69 秒压力样本之后额外约 2 秒 idle，frames 均为 4013、Region updates 均为 2066；`renderCpuP95Ms=1.6459`、`regionCpuP95Ms=0.6468`、`artworkUploadMs=0.2869`、`blurBuildMs=2.3993`。短时软件采样，不代表硬件帧率或 60 秒验收。 |
| Windows PowerShell 5.1 `powershell.exe -NoProfile -ExecutionPolicy Bypass -File scripts/accessibility.ps1` | **PASS（Legacy 兼容 / 隔离 demo）** | `artifacts/accessibility-results.json`、`accessibility-snapshot.json`。UIA Invoke、RangeValue 0–100 / demo 值写入 67 / 拒绝 101、MSAA、焦点/边界/值、stale provider 和折叠释放通过。启动使用 `--demo` 与无 BOM UTF-8 fixture；初末 fresh diagnostics 均 `configurationValid=true`，`audioPolls` / `mediaPolls` 字段缺席（按服务未创建记录为 `null`）。owned PID 29892 / HWND 1446544 精确匹配 `IsleNativePrototype`；校验后的 `WM_CLOSE` 等待 48 ms，exit code 0，`forcedKill=false`。这项脚本不带 `--ui-v2`，不能当作 V2 provider/Spring bounds 的 UIA 验收。 |

### D060 失败尝试与限制

- D060 accessibility 早期同二进制运行完成 UIA 操作后，在旧清理路径的 5 秒 `WM_CLOSE` 等待中超时；当时没有强杀。之后枚举确认 `.NET MainWindowHandle` 报告的是 `UAC Input Indicator` helper，而该 PID 的精确主窗 class 是 `IsleNativePrototype`。旧 finally 缓存的确切目标 HWND 未记录，因此没有把首次 timeout 的根因说成已完全证明。以 strict PID/class HWND 单独探测后，diagnostic 更新、`PostMessage(WM_CLOSE)` 成功且进程结束；探测句柄权限不足以读取 exit code。修正脚本为按 PID + 主窗 class 唯一发现并缓存 HWND、每次操作验证 HWND/PID/class、检查 `PostMessage` 返回值；随后同一 D060 上的完整 UIA 回归最终正常结束并记录 exit code 0。早期失败保留在记录中。
- 运行主机实测 monitor work area 为 1920×1032、DPI 96。placement 脚本请求的 1366×690 viewport 被真实 `WM_GETMINMAXINFO` 最小跟踪尺寸限制到 1466×1032；没有得到真实 1366×200% 工作区证据。compact 2×3 分支使用 synthetic 400% viewport 覆盖，没有改 OS DPI、分辨率或工作区。离屏恢复以当前无重叠的 monitor topology 模型检查，没有增删实际显示器。
- 本节全部 screenshot/visual/hardware-DPI 状态仍未验证；UIA 是 Legacy demo 兼容回归，V2 provider 真实 bounds 和读屏器实机体验仍待单独验收。D060 motion idle 仅为约 2 秒软件样本，不能替代长时间或真实硬件性能测试。

## E6DD Common Controls v6 前置候选（2026-10-03）

本节是独立候选记录，范围限于 Common Controls v6 manifest 兼容、原生 Legacy Settings/倒计时窗口和 V2 设置生命周期。F699 与 D060 记录保留在上方，不把未重跑的其它套件并入此 SHA。

### 1. Candidate and build

固定二进制 `target/ui2-controls/release/isle-native.exe`，SHA-256 `E6DD5BACDC6F7CACD8FA5F24110BC63C72087F0EBB3EE5E20AAAADAB2C90A4E7`。A0 提供 `cargo fmt`、`cargo check`、`cargo clippy -- -D warnings`、workspace 112 tests（另 1 项联网测试 ignored）和 release build PASS；A6 未重编。E6DD 当时的静态资源 inspector 结论为通过：唯一 RT_MANIFEST resource ID 1、Common-Controls 6.0 dependency、PerMonitorV2/per-monitor DPI、`asInvoker`、ICON/VERSIONINFO 保留。其 canonical `artifacts/manifest-controls/results.json` 已被后续 SHA `94E33D2E6C56E64A9C54CE86D76B4899B862239266A7934DDCD312739AC01930` 覆盖；只读查找未发现 E6DD inspector 副本，因此该路径现在仅代表 94E3。Inspector 的 activation-context / ComCtl32 结果不等同于 E6DD 目标进程运行时证明。

### 2. Legacy Settings controls

`scripts/native_settings.py --no-screenshots` 在 E6DD 上两次通过。覆盖原生 Button、ComboBox、TrackBar/Slider 的设置状态、时钟选择、应用/取消/重启保真、副屏边界和 floating placement/shape/color。最终证据 `artifacts/native-settings-results.json` 包含两个 owned PID 7392、29384：都经 WM_CLOSE 返回 code 0，`forcedKill=false`。第一次尚未写 exit-code 字段的通过结果另存为 `artifacts/e6dd-native-settings-initial-run.json`。

### 3. Native countdown Edit control

`scripts/manifest_timer_controls.py` 使用独立 `--demo` fixture 和 PID/class 校验，测试真实 `IsleNativeTimerWindow` 的原生 Edit ID 300。稳定缓冲区跨进程 `WM_SETTEXT` / `WM_GETTEXT` 后 Edit 内容为 25；App fresh snapshot 证明 timer start `1499.99999` 秒、pause `1499.98297` 秒、reset 精确 `1500` 秒，确认实际选择了 25 分钟。`artifacts/manifest-timer-controls/results.json` 记录 PID 28860、主窗与倒计时 HWND，正常 WM_CLOSE 47 ms、exit 0、无强杀。初始/最终 fixture 有效，`audioPolls`/`mediaPolls` 字段缺席；没有真实播放或系统音量动作。

### 4. V2 settings lifecycle

`scripts/ui_v2.py` 在 E6DD 上 PASS：实时保存、未知字段、inspection unlock、外部冲突/revert、退出 flush/restart，以及同一 owned host 对设置窗打开/切页/实际 resize/paint/关闭 30 次。`artifacts/ui-v2/results.json` 的 30 轮 GDI 句柄预热基线 54、关闭采样 52、跨度 2，最后 `settingsWindowAlive=false`。脚本每个 owned close 后断言进程 return code 0；命令以 PASS 和 shell exit 0 结束。E6DD 运行复用了 D060 的 canonical 结果目录；D060 原整目录已经复制到 `artifacts/ui-v2-d060-history/`。

### 5. Runtime DPI awareness and ComCtl32

E6DD 实际 owned native host 与 Legacy Settings HWND 均通过 `GetWindowDpiAwarenessContext` / `AreDpiAwarenessContextsEqual` 只读核对为 PMv2（context `0x22`）；重启后的 host 仍为 PMv2。对两个候选进程只读枚举模块，加载路径均为 `C:\Windows\WinSxS\amd64_microsoft.windows.common-controls_6595b64144ccf1df_6.0.26100.9278_none_3e0d1ba8e3303201\comctl32.dll`，证明实际进程加载了 side-by-side ComCtl32 v6；没有注入代码。

### 6. Process isolation and exits

UI 脚本均先设置 `ISLE_TEST_MONITOR='\\.\DISPLAY2'` 和 `ISLE_TEST_EXE` 到固定 E6DD 二进制；原生设置和计时器测试使用独立 JSON fixture，timer/UIA 输入仅发送到通过本次 PID + window class 校验的 owned HWND。Settings 两个进程及 timer PID 28860 退出 code 均为 0、没有强制终止。已存在的 PID 4936 保持不变且未控制。

### 7. Preserved artifacts and harness corrections

候选历史分别保存在 `artifacts/f699-native-settings-results.json`、`artifacts/e6dd-native-settings-initial-run.json` 和 `artifacts/ui-v2-d060-history/`；E6DD final 输出仍可从 `artifacts/native-settings-results.json`、`artifacts/ui-v2-e6dd-history/` 及 `artifacts/manifest-timer-controls/results.json` 核对。E6DD 当时的 `artifacts/manifest-controls/results.json` 已被 94E3 覆盖且没有历史副本，不再是 E6DD 的可核验输出。计时器辅助测试前两次的 harness 失败另存 `first-run-failure.json` 与 `second-run-failure.json`。PID 33568 的早期结果保存在 `partial-false-positive-30-minute.json`：Edit caption 显示 25，但 App timer 状态实际为 30 分钟（约 1800 秒），该结果撤回且不计为 PASS。根因是跨进程 `SetWindowText`/`GetWindowText` 没有证明真实 Edit 消息和 EN_CHANGE 路径；改为稳定缓冲区 `WM_SETTEXT`/`WM_GETTEXT` 并检查精确 App timer 状态后复测通过。此过程没有确认产品缺陷。

### 8. Limits and next scope

所有 E6DD 设置/计时器运行均用 `--no-screenshots` 或没有抓屏，未验证视觉外观。当前测试证明了选定的原生控件与 timer Edit 命令路径可工作，但没有覆盖所有 ComCtl 控件绘制回调、UIA pattern 全面性或 D2D/native fallback 诊断；静态 manifest activation-context inspector 不等于目标进程行为，实际目标进程的 PMv2 和 ComCtl32 DLL 已由独立只读运行时探针补证。没有变更系统 DPI、分辨率、用户安装配置或真实音频/媒体服务。A4 的后续控件绘制故障注入、callback counts 与 Settings UIA pattern 覆盖应在其新 frozen SHA 上另记，不回填到 E6DD 结论。

### 9. Settings UIA proxy investigation (pending)

The original E6DD UIA inventory (`artifacts/settings-uia-E6DD5BACDC6F/uia-inventory.json`) found 11 descendant elements, all exposed as Pane without useful patterns. A follow-up in Windows PowerShell 5.1 / .NET Framework 4.8 loaded the Microsoft GAC `UIAutomationClientsideProviders.dll`, but `ClientSettings.RegisterClientSideProviderAssembly` failed in `MS.Internal.Automation.ProxyManager.LoadDefaultProxies` with `NullReferenceException`; the resulting managed-provider inventory is therefore inconclusive and is not evidence of an E6DD control defect (`uia-inventory-microsoft-proxy.json`).

An independent PS 5.1 process with neither `UIAutomationClient` nor `UIAutomationTypes` loaded queried stock Win32 controls through native `CUIAutomation` and MSAA. Button, checkbox, radio, ComboBox, and trackbar returned their expected UIA control types and Invoke/Toggle/SelectionItem/ExpandCollapse/Value/RangeValue patterns, with matching MSAA roles and Microsoft unmanaged `uiautomationcore.dll` proxies (`artifacts/settings-uia-proxy-reference.json`). This establishes that the host's native UIA core works for a minimal reference set; it does not determine why the E6DD Settings descendants appeared as Panes.

The first independent E6DD COM-only probe opened Settings and verified a fresh fixture diagnostic (`settingsWindowAlive=true`, configuration valid, no audio/media service fields), then aborted its inventory on an overly broad HWND/UIA-PID assertion. Its preserved failure (`artifacts/settings-uia-E6DD5BACDC6F/failure-native-core-20261003-195941.json`) records the offending HWND but not enough per-HWND metadata to diagnose the mismatch. The owned E6DD process had already exited with code 0 before cleanup posted `WM_CLOSE`; no forced termination occurred. The probe has been updated to persist each HWND's native class/owner and the UIA-reported PID/provider separately, but has not been rerun. Settings UIA patterns remain **pending** for a later frozen candidate and must not be reported as either pass or product defect based on these E6DD results.

### 10. 94E3 settings-painter candidate: limited launch failure

The new frozen candidate is `target/ui2-settings-painter/release/isle-native.exe`, SHA-256 `94E33D2E6C56E64A9C54CE86D76B4899B862239266A7934DDCD312739AC01930`. Before testing, the E6DD `artifacts/ui-v2/` directory was copied to `artifacts/ui-v2-e6dd-history/`; all seven copied files matched by SHA-256 and the preserved `results.json` identifies E6DD. The existing `scripts/ui_v2.py` was run against 94E3 with an isolated fixture and `DISPLAY2`. It failed before the lifecycle cases began: the first fresh diagnostic snapshot timed out and no `snapshot.json` was created. The owned process showed an exact `Isle Native — error` dialog with `Isle Native: 参数错误。 (0x80070057)`; the Temp error log was written at 20:23:44 +08:00. Failure and shutdown details are preserved in `artifacts/ui-v2-94e33d2e6c56/failure-ui-v2-20261003-202344.json` and `timeout-owned-29452-20261003-202350.json`.

The harness posted `WM_CLOSE` only to the verified owned `IsleNativePrototype` HWND. After it timed out, the same exact main window received a second graceful close request; the owned error dialog was then confirmed by PID/class/title and its “确定” button was clicked normally. The process was absent afterward, but its exit code was not captured, so this is a **failed run**, not a clean-exit pass. No force termination was used. Because startup diagnostics failed, service-field absence was not verified; the requested paint-role, `dropdownChrome`, fault/fallback, Settings interaction, and UIA checks were not run. These remain pending, and the HRESULT is not yet assigned to a product defect. `scripts/ui_v2.py` now verifies the owned PID/class on close and records a timeout without calling `terminate()`; it can write candidate artifacts to a SHA-scoped directory under `native/artifacts`.
