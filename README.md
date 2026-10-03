# Isle 原生 UI（main 默认版本）

main 默认的 Windows x64 原生程序。使用 Rust、Win32、Direct2D、DirectWrite、DXGI 与 DirectComposition；不依赖 Tauri、Wry 或 WebView2。旧版 WebView 源码使用根目录 web:* 命令构建。

UI 2.0 开发路径通过 `--ui-v2` 启用，默认保留兼容版本。V2 设置直接更新真实 Isle 并合并后台保存，提供保存错误、重试和恢复；运行时 revision 与已落盘 revision 分开。`--motion-scale 1|0.5|0.2` 用于本次运行的动画检查。完整完成范围和待验收项目见 [进度表](../docs/native-ui-v2-progress.md) 与 [企业级执行门禁](../docs/native-ui-v2-execution.md)，当前开发构建不代表正式发布验收。

V2 集成回归使用隔离配置和演示媒体，运行 `python native/scripts/ui_v2.py`、`python native/scripts/ui_v2_players.py` 与 `python native/scripts/ui_v2_motion.py`。后者覆盖五档模拟 DPI、四边布局、共享封面、指针 seek、30 次展开收起和 idle 帧 / Region 更新检查；真实硬件 DPI、截图及屏幕阅读器另行记录。

`python native/scripts/ui_v2_placement.py` 验证 V2 设置窗位置的手动保存、重开 / 重启、默认不写回、离屏恢复、未知字段和父窗口退出补取，以及紧凑导航、横滚与 Tab / End。位置使用物理坐标和外窗 DIP 尺寸，恢复按当前目标显示器 DPI；模拟 DPI 不改变 OS，实际视口仍受真实工作区和最小跟踪尺寸限制。该脚本记录请求 / 实际尺寸与 SHA，不能替代小屏、混合 DPI 和截图验收。

程序运行时可用 `--target-dir native/target/ui2-candidate` 构建独立候选，避免替换被 Windows 锁定的可执行文件。测试脚本支持 `ISLE_TEST_EXE` 指定该候选，默认仍使用 `native/target/release/isle-native.exe`；QA 记录必须包含实际二进制 SHA-256。当前回归及崩溃修复证据见 [QA 记录](../docs/native-ui-v2-qa.md) 与 [独立审查](../docs/native-ui-v2-review.md)。

## 构建与运行

需要 Rust MSVC 工具链、Visual Studio C++ Build Tools 和 Windows SDK。

```powershell
cargo build --release --manifest-path native/Cargo.toml
./native/target/release/isle-native.exe
```

启动默认连接真实媒体。点击空白展开/收起，F8 打开设置，Alt+F4 退出。首次可沿用旧版配置，迁移保存保留备份。--demo 使用演示数据；诊断脚本使用隔离数据，显式 live 参数可覆盖。--open-floating 和 --open-timer 可在正常启动时打开辅助窗口。

构建会把仓库已有的三个 MiSans 字重复制到可执行文件旁的 `fonts/`。复制原型到其他目录时请同时复制该目录；字体通过 DirectWrite 私有集合加载，不安装到系统。缺少字体时回退到 Segoe UI，诊断报告记录实际 `fontFamily`。

```powershell
./native/target/release/isle-native.exe --page music --attached --long-title
./native/target/release/isle-native.exe --page weather --edge left --attached
./native/target/release/isle-native.exe --page volume --paused --reduced-motion
./native/target/release/isle-native.exe --live-media --page music
./native/target/release/isle-native.exe --live-audio --page volume
./native/target/release/isle-native.exe --live-spectrum --page music
```

| 操作 | 行为 |
|---|---|
| F1 / F2 / F3 / F4 | 顶部 / 右侧 / 底部 / 左侧 |
| F5 | 切换悬浮和贴边 |
| F6 | 切换减少动画 |
| F7 | 切换长短标题 |
| F8 | 打开原生设置（全部工具关闭时仍可用） |
| 方向键，Enter | 选择并打开工具；聚焦音量刻度尺时调节音量 |
| Tab / Shift+Tab，Enter / Space | 遍历并激活工具与页面控件 |
| Escape | 详情返回音乐，音乐收起 |
| 拖动工具栏或滚轮 | 浏览后面的时间、天气按钮 |
| Space | 切换模拟播放状态 |

`--tools 0..7` 用于工具数量验证；`--paused` 停止模拟频谱；`--exit-after 秒数 --log 路径` 用于自动采样；`--scripted` 循环切页。

## 已实现范围

- 透明合成窗口、浮动和四边凹肩轮廓；可见轮廓与原生窗口区域共用几何数据。
- 可中断的宽、高、圆角、凹肩弹簧；高精度单次计时器请求约 60 次/秒，静止时取消帧等待，按需绘制，不修改系统全局计时精度。
- 纯图标紧凑工具栏、横向拖动、返回、空白收起与手势取消。
- 音乐、音量、倒计时、时间、天气五个原型页；只有当前页面实例，收起释放页面。
- 本地倒计时截止时间独立于页面；时间按保存的时区显示，默认跟随系统。
- 长标题往返滚动和渐变，切歌、返回音乐和外形变化时重置停留；标题宽度只在创建布局时测量。
- 自动读取并响应 Windows 客户端动画设置；F6 或命令行提供本次运行的覆盖。时间页下一次刷新对齐整分钟，响应系统时间变化。
- 处理 DPI 变更建议矩形，保持所选贴边方向；小工作区下等比缩小容纳区域，绘制、命中与无障碍坐标使用同一比例。
- 隐藏/最小化时释放页面、字体布局缓存和图形设备引用，并停止帧计时器；倒计时状态独立保留，隐藏期间完成后在恢复时展示提示。
- 原生 MSAA 控件接口与 Windows UIA 桥接：名称、按钮/滑块角色、焦点、物理位置、激活和音量值操作。旧页面控件引用会失效，收起和隐藏后控件从树中移除。

**main 已切换到原生 UI，部分旧版功能仍待迁移。** 默认连接真实媒体，`--live-media` 可读取 Windows 媒体会话的真实歌名、歌手、播放状态和进度，并通过播放、上一首、下一首按钮发送实际控制请求。不支持的控制显示为禁用；F7 在真实模式下无效。已读取原生/迁移配置中的播放器白名单和优先顺序；设置窗口的“播放器…”可发现当前会话、勾选和排序。允许全部与明确全部禁用分别保留为 null 和空列表。优先显示播放器缩略图；网易云使用严格歌名/歌手匹配补全时长和缺失封面。匹配失败时保留占位，不采用无关图片。播放器未提供实际位置时，本地外推仍有精度限制。

`--live-audio` 单独启用真实系统音量和输出设备；`--live-media` 同时启用它们。音量页刻度尺支持拖动和键盘，点击数值切换静音，点击设备名进入输出设备列表。列表每页最多 4 项，方向键、滚轮和翻页按钮切换；Escape 先返回音量页，再返回音乐页。设备切换会更改 Windows 默认输出，包括通信角色。默认演示模式不修改系统音量。

`--live-media` 已接入 WASAPI 六段频谱：读取当前默认输出设备的系统混音，音乐页或收起岛可见且媒体正在播放时采集；其他功能页、暂停或隐藏时停止并释放。`--live-spectrum` 单独验证输出采集，标题和播放按钮仍为演示数据。分析最多约 20 次/秒，界面平滑插值沿用约 60Hz 帧调度；减少动画模式直接显示采集结果。无输入时回到平线，不用模拟数据替代真实频谱。设备变更约一秒内检测并重建，采集失败退避重试。仅分析输出，不读取麦克风，也不保存原始声音。

天气页已接入 Open-Meteo 当前天气和未来三天预报；工具栏设置按钮和天气齿轮打开原生城市设置窗口，支持中文搜索和保存。配置写入 `%APPDATA%/IsleNative/settings.json`，首次可只读沿用旧版天气城市；`--settings-path` 指定独立配置时不读取旧配置。天气缓存最多 4 个城市，有效期 30 分钟，失败保留同城缓存并退避 60 秒。窗口关闭释放控件，切页或隐藏取消页面请求，过期结果不会回写。

原生悬浮播放器、独立倒计时与设置窗口已实现；托盘和 MV 视频尚未迁移。基础控件无障碍接口已接入，实际屏幕阅读器、文本阅读语义、完整 UIA RangeValue 模式及 tooltip 仍待补齐；多 DPI 实机、设备丢失、睡眠恢复和旧版动画轨迹校准仍待验收。

天气验证：`python native/scripts/live_weather.py`，天气可见、隐藏各采样 60 秒并循环打开/关闭城市设置 12 次；`--quick` 使用 5 秒样本。只写 `native/artifacts/` 中的测试配置，不修改安装版配置。需要能访问 Open-Meteo，阶段结果见 [天气与城市设置](../docs/native-ui-weather.md)。

配置保存已迁移到独立后台线程，保留旧版完整字段及未知嵌套数据。首次从旧版迁移并保存时写入原始备份，后续保存保留上一版；损坏文件和外部修改会阻止覆盖。天气、顶部工具栏开关及动画偏好已接入运行时，其余设置等待 Studio 迁移。`python native/scripts/configuration_faults.py` 验证隔离配置的启动保护；天气脚本同时验证保存保真、备份和重启。详见 [原生配置兼容](../docs/native-ui-configuration.md)。

测试显示器：`--test-monitor '\\.\DISPLAY2'` 或设置进程环境变量 `ISLE_TEST_MONITOR`，启动前选定目标显示器；城市设置跟随主窗口所在屏幕。无效显示器名称报错，不回落到主屏。自动化测试使用 `--benchmark` 配合窗口消息，窗口显示和再次打开设置均不抢前台焦点。按用户要求，本机后续 UI 测试使用左侧 `DISPLAY2`，右侧主屏留给用户。

系统音量服务仅在可见音量页采集，每秒更新；切页、收起或隐藏释放 COM 端点与枚举器并停止轮询。拖动命令合并且队列最多 8 条，执行前核对默认设备身份，丢弃超过 2 秒的命令。设备列表有独立无障碍名称，设备重排后旧控件引用失效。默认设备切换沿用旧后端的非公开 `IPolicyConfig` 接口，仍需多系统版本和真实切换验收。

只读音量与生命周期测试：`python native/scripts/live_audio.py`。需要至少一个可用输出设备；可见、隐藏各采样 60 秒并执行 12 次页面释放/重建，验证期间不发送音量、静音或设备切换命令。阶段报告见 [系统音量迁移](../docs/native-ui-system-audio.md)。

频谱验证：`python native/scripts/live_spectrum.py`，静态 UI 基线、实时频谱、隐藏各采样 60 秒，随后检查 12 次切页和快速隐藏/恢复；`--quick` 使用 5 秒样本。需要系统已有声音播放，脚本不会自动播放或修改音量。基线使用同一原生构建，不是旧 WebView2 完整程序。阶段结果和限制见 [实时频谱迁移](../docs/native-ui-spectrum.md)。

页面状态是 `Option<PageInstance>`，切换直接替换，收起设为 `None`。渲染器只保留当前标题布局，离开音乐页后释放。真实媒体服务在专用 WinRT 线程运行，可见音乐页及收起状态按秒读取；其他功能页、隐藏和最小化期间释放媒体会话、事件订阅与管理器。只保留一个最新快照和最多 8 条待处理命令；异步操作支持取消及 2 秒超时。当前只有一个原生窗口，未来多窗口共享消费者计数仍需实现。

只读媒体生命周期测试：`python native/scripts/live_media.py`，可见和隐藏各采样 60 秒，再执行 12 次切页释放/重建。测试不发送播放控制；输出不包含歌名或播放器身份。

添加 `--require-artwork` 可验证真实封面和补全时长（需正在播放可匹配曲目）。网络/解码使用独立线程、8 秒任务预算和切页取消；响应限制 2 MiB，解码源最大 4096 边长且不超过 16MP。缩略图输出 128×128 BGRA，V2 展开异步请求 512×512；同曲目高清结果不会被晚到的缩略图降级。CPU 缓存上限为 8 张缩略图和 4 张高清图，V2 GPU 只保留当前图、过渡前一图及当前 Blur 缓存。默认单元测试不联网；可选的网易云 CDN 回退验证使用 `cargo test --manifest-path native/Cargo.toml live_provider_downloads -- --ignored --nocapture`。

## 验证

```powershell
cargo fmt --manifest-path native/Cargo.toml --all -- --check
cargo clippy --manifest-path native/Cargo.toml --workspace --all-targets -- -D warnings
cargo test --manifest-path native/Cargo.toml --workspace
python native/scripts/interaction.py
python native/scripts/display_lifecycle.py
python native/scripts/lifecycle.py
powershell.exe -NoProfile -File native/scripts/accessibility.ps1
python native/scripts/visibility_resources.py
./native/scripts/measure.ps1 -Seconds 60 -Repetitions 3
```

截图脚本需要 Pillow。`interaction.py` 按进程 ID 定位自己的窗口，并创建另一个进程的背景窗口检查透明区域的窗口路由；不会向安装版 Isle 发送指令。该检查使用 `WindowFromPoint` 和向返回窗口发送消息，不等价于鼠标硬件输入的全链路验收。

资源可见性测试还需要 psutil；无障碍脚本通过 Windows PowerShell 的 .NET Framework interop 从另一个进程访问接口。`--test-dpi`、`--test-work-area WxH`、`--test-countdown-ms` 仅为测试覆盖参数，不修改系统显示或现有应用设置。`WM_APP+60` 在启用 `--log` 时写出当前诊断快照，不触发绘制。

设置控件的资源前置检查：`python native/scripts/manifest_controls.py --exe native/target/ui2-controls/release/isle-native.exe`。脚本映射 EXE 资源并在自身进程创建临时 activation context，核对 manifest #1、Common Controls v6、PMv2、asInvoker，以及原 ICON / VERSIONINFO；输出到 `native/artifacts/manifest-controls/results.json`。仅用于可信的本地构建：它不运行目标 EXE，但会加载该 context 解析的 ComCtl32 DLL 并调用其 DllGetVersion，结果保留实际 DLL 路径。它不证明目标进程 custom-draw、DPI 或视觉状态，须配合同一 SHA 的窗口兼容回归。Python 标准库即可运行。

所有测试输出位于忽略提交的 `native/artifacts/`。性能报告里的 `drawAndPresentP95Ms` 包含 Present 等待，**不是帧间隔，也不是输入延迟**。CPU 百分比按机器全部逻辑处理器归一化；私有内存和工作集分别记录。

`presentCallIntervalMeanMs/P95Ms` 是预热 5 秒后连续动画期间相邻绘制/Present 返回的间隔，不代表 DWM 最终上屏时间。`highResolutionTimer` 记录是否使用了高精度路径；不支持时回退普通等待计时器。日志采样数组有上限，未指定 `--log` 时不收集逐帧数据。

实现与验收进展见 [首轮记录](../docs/native-ui-prototype-results.md)、[帧调度改进记录](../docs/native-ui-frame-timing-results.md) 和 [显示、生命周期与无障碍记录](../docs/native-ui-display-accessibility-results.md)。

原生设置新增功能栏总开关、七个工具开关、启用动画和减少动画；点击“应用设置”保存并实时生效，关闭窗口丢弃未应用草稿。`python native/scripts/native_settings.py` 验证稀疏工具、全关闭恢复、草稿取消、重启及副屏边界。阶段记录见 [原生工具与动画设置](../docs/native-ui-settings.md)。

时钟时区已接入原生设置，沿用系统、台北、东京、纽约、伦敦、UTC 六项；后台保存后即时更新并在重启后恢复。使用 Windows 动态夏令时转换，旧配置中不支持的时区原样保留并显示提示。详见 [时钟时区迁移](../docs/native-ui-clock.md)。

播放器选择验证：`python native/scripts/native_players.py`。使用隔离配置、两个未运行测试项和当前系统会话，覆盖允许全部/全部禁用、单项勾选、排序、刷新、重启及 12 次窗口循环；只读取会话，不发送媒体控制或音量命令。阶段记录见 [原生播放器选择](../docs/native-ui-players.md)。
