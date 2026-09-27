# Isle 原生 UI 原型

独立的 Windows x64 原型。使用 Rust、Win32、Direct2D、DirectWrite、DXGI 与 DirectComposition；不依赖 Tauri、Wry 或 WebView2。现有正式程序的源码与设置保持原样。

## 构建与运行

需要 Rust MSVC 工具链、Visual Studio C++ Build Tools 和 Windows SDK。

```powershell
cargo build --release --manifest-path native/Cargo.toml
./native/target/release/isle-native.exe
```

启动即显示独立原型岛。点击空白展开/收起，Alt+F4 退出。原型不注册开机启动、不读取或写入旧版设置、不占用旧版快捷键。

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
- 本地倒计时截止时间独立于页面；时间读取系统本地时间。
- 长标题往返滚动和渐变，切歌、返回音乐和外形变化时重置停留；标题宽度只在创建布局时测量。
- 自动读取并响应 Windows 客户端动画设置；F6 或命令行提供本次运行的覆盖。时间页下一次刷新对齐整分钟，响应系统时间变化。
- 处理 DPI 变更建议矩形，保持所选贴边方向；小工作区下等比缩小容纳区域，绘制、命中与无障碍坐标使用同一比例。
- 隐藏/最小化时释放页面、字体布局缓存和图形设备引用，并停止帧计时器；倒计时状态独立保留，隐藏期间完成后在恢复时展示提示。
- 原生 MSAA 控件接口与 Windows UIA 桥接：名称、按钮/滑块角色、焦点、物理位置、激活和音量值操作。旧页面控件引用会失效，收起和隐藏后控件从树中移除。

**这是迁移中的独立原型，尚不能替换日常版本。** 默认仍使用演示数据，`--live-media` 可读取 Windows 媒体会话的真实歌名、歌手、播放状态和进度，并通过播放、上一首、下一首按钮发送实际控制请求。不支持的控制显示为禁用；F7 在真实模式下无效。尚不读取旧版播放器选择设置，使用自动选择策略。优先显示播放器缩略图；网易云使用严格歌名/歌手匹配补全时长和缺失封面。匹配失败时保留占位，不采用无关图片。播放器未提供实际位置时，本地外推仍有精度限制。

`--live-audio` 单独启用真实系统音量和输出设备；`--live-media` 同时启用它们。音量页刻度尺支持拖动和键盘，点击数值切换静音，点击设备名进入输出设备列表。列表每页最多 4 项，方向键、滚轮和翻页按钮切换；Escape 先返回音量页，再返回音乐页。设备切换会更改 Windows 默认输出，包括通信角色。默认演示模式不修改系统音量。

`--live-media` 已接入 WASAPI 六段频谱：读取当前默认输出设备的系统混音，音乐页或收起岛可见且媒体正在播放时采集；其他功能页、暂停或隐藏时停止并释放。`--live-spectrum` 单独验证输出采集，标题和播放按钮仍为演示数据。分析最多约 20 次/秒，界面平滑插值沿用约 60Hz 帧调度；减少动画模式直接显示采集结果。无输入时回到平线，不用模拟数据替代真实频谱。设备变更约一秒内检测并重建，采集失败退避重试。仅分析输出，不读取麦克风，也不保存原始声音。

天气页已接入 Open-Meteo 当前天气和未来三天预报；工具栏设置按钮和天气齿轮打开原生城市设置窗口，支持中文搜索和保存。配置写入 `%APPDATA%/IsleNative/settings.json`，首次可只读沿用旧版天气城市；`--settings-path` 指定独立配置时不读取旧配置。天气缓存最多 4 个城市，有效期 30 分钟，失败保留同城缓存并退避 60 秒。窗口关闭释放控件，切页或隐藏取消页面请求，过期结果不会回写。

浮动播放器按钮仍仅显示说明，完整 Studio、托盘和视频均未迁移。基础控件无障碍接口已接入，实际屏幕阅读器、文本阅读语义、完整 UIA RangeValue 模式及 tooltip 仍待补齐；多 DPI 实机、设备丢失、睡眠恢复和旧版动画轨迹校准仍待验收。

天气验证：`python native/scripts/live_weather.py`，天气可见、隐藏各采样 60 秒并循环打开/关闭城市设置 12 次；`--quick` 使用 5 秒样本。只写 `native/artifacts/` 中的测试配置，不修改安装版配置。需要能访问 Open-Meteo，阶段结果见 [天气与城市设置](../docs/native-ui-weather.md)。

配置保存已迁移到独立后台线程，保留旧版完整字段及未知嵌套数据。首次从旧版迁移并保存时写入原始备份，后续保存保留上一版；损坏文件和外部修改会阻止覆盖。当前仅天气字段接入运行时，其余设置等待 Studio 迁移。`python native/scripts/configuration_faults.py` 验证隔离配置的启动保护；天气脚本同时验证保存保真、备份和重启。详见 [原生配置兼容](../docs/native-ui-configuration.md)。

测试显示器：`--test-monitor '\\.\DISPLAY2'` 或设置进程环境变量 `ISLE_TEST_MONITOR`，启动前选定目标显示器；城市设置跟随主窗口所在屏幕。无效显示器名称报错，不回落到主屏。自动化测试使用 `--benchmark` 配合窗口消息，窗口显示和再次打开设置均不抢前台焦点。按用户要求，本机后续 UI 测试使用左侧 `DISPLAY2`，右侧主屏留给用户。

系统音量服务仅在可见音量页采集，每秒更新；切页、收起或隐藏释放 COM 端点与枚举器并停止轮询。拖动命令合并且队列最多 8 条，执行前核对默认设备身份，丢弃超过 2 秒的命令。设备列表有独立无障碍名称，设备重排后旧控件引用失效。默认设备切换沿用旧后端的非公开 `IPolicyConfig` 接口，仍需多系统版本和真实切换验收。

只读音量与生命周期测试：`python native/scripts/live_audio.py`。需要至少一个可用输出设备；可见、隐藏各采样 60 秒并执行 12 次页面释放/重建，验证期间不发送音量、静音或设备切换命令。阶段报告见 [系统音量迁移](../docs/native-ui-system-audio.md)。

频谱验证：`python native/scripts/live_spectrum.py`，静态 UI 基线、实时频谱、隐藏各采样 60 秒，随后检查 12 次切页和快速隐藏/恢复；`--quick` 使用 5 秒样本。需要系统已有声音播放，脚本不会自动播放或修改音量。基线使用同一原生构建，不是旧 WebView2 完整程序。阶段结果和限制见 [实时频谱迁移](../docs/native-ui-spectrum.md)。

页面状态是 `Option<PageInstance>`，切换直接替换，收起设为 `None`。渲染器只保留当前标题布局，离开音乐页后释放。真实媒体服务在专用 WinRT 线程运行，可见音乐页及收起状态按秒读取；其他功能页、隐藏和最小化期间释放媒体会话、事件订阅与管理器。只保留一个最新快照和最多 8 条待处理命令；异步操作支持取消及 2 秒超时。当前只有一个原生窗口，未来多窗口共享消费者计数仍需实现。

只读媒体生命周期测试：`python native/scripts/live_media.py`，可见和隐藏各采样 60 秒，再执行 12 次切页释放/重建。测试不发送播放控制；输出不包含歌名或播放器身份。

添加 `--require-artwork` 可验证真实封面和补全时长（需正在播放可匹配曲目）。网络/解码使用独立线程、8 秒任务预算和切页取消；响应限制 2 MiB，解码源最大 4096 边长且不超过 16MP，输出 128×128 BGRA。最近 8 首共约 512 KiB 像素缓存，GPU 仅保留当前可见封面。默认单元测试不联网；可选的网易云 CDN 回退验证使用 `cargo test --manifest-path native/Cargo.toml live_provider_downloads -- --ignored --nocapture`。

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

所有测试输出位于忽略提交的 `native/artifacts/`。性能报告里的 `drawAndPresentP95Ms` 包含 Present 等待，**不是帧间隔，也不是输入延迟**。CPU 百分比按机器全部逻辑处理器归一化；私有内存和工作集分别记录。

`presentCallIntervalMeanMs/P95Ms` 是预热 5 秒后连续动画期间相邻绘制/Present 返回的间隔，不代表 DWM 最终上屏时间。`highResolutionTimer` 记录是否使用了高精度路径；不支持时回退普通等待计时器。日志采样数组有上限，未指定 `--log` 时不收集逐帧数据。

实现与验收进展见 [首轮记录](../docs/native-ui-prototype-results.md)、[帧调度改进记录](../docs/native-ui-frame-timing-results.md) 和 [显示、生命周期与无障碍记录](../docs/native-ui-display-accessibility-results.md)。
