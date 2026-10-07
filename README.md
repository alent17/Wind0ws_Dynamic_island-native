# Isle Native UI

独立的 Windows 原生灵动岛应用，使用 Rust、Win32、Direct2D、DirectWrite、DXGI 和 DirectComposition。运行时不依赖 Tauri、Wry 或 WebView2。

当前版本：**0.62**（Cargo / Windows 文件版本 `0.62.0`）。版本序列从 0.10 延续，每个完成并验证的步骤递增版本，在 `main` 提交并推送。

原 Tauri/WebView 项目位于 [Wind0ws_Dynamic_island](https://github.com/alent17/Wind0ws_Dynamic_island)。

## 功能

- Windows 系统媒体会话、封面、播放控制和进度。
- 灵动岛收起/展开、四边贴靠、动画与实时频谱。
- 系统音量与输出设备、时钟、天气和城市设置。
- 独立悬浮播放器、倒计时和原生设置窗口。
- 可配置的小组件架（音乐、音量、倒计时、时钟、天气、系统状态）；网易云播放模式通过本机 CDP 异步读取并验证切换。
- 多显示器支持及基础 MSAA/UIA 桥接；原生设置控件提供 Invoke、Toggle、Value、RangeValue、ExpandCollapse 和单选 Selection/SelectionItem 自动化模式。

## 构建与运行

需要 Windows 10/11 x64、Rust MSVC 工具链、Visual Studio C++ Build Tools 和 Windows SDK。

```powershell
cargo build --release --bin isle-native
./target/release/isle-native.exe
```

默认连接真实媒体；`--demo` 使用演示数据。F8 打开设置，Alt+F4 退出。

## 开发检查

```powershell
cargo fmt --all -- --check
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

自动化脚本位于 `scripts/`，测试结果写入被忽略的 `artifacts/`。集成测试使用自己的测试进程和隔离配置，不代表所有硬件、DPI、屏幕阅读器及视觉场景均已验收。

副屏 UI 测试需设置 `ISLE_TEST_MONITOR='\\.\DISPLAY2'`。交互与截图脚本会校验测试窗口位于副屏，并用全虚拟桌面捕获支持负坐标的显示器截图；空白截图会使测试失败。

## 迁移与验收状态

当前按 UI 优先方案完成静态布局、动画与 Glass，并完成第一阶段 Review 与验收。路线图清单保存在仓库根目录；QA 截图与诊断数据集中保存在被 Git 忽略的 `artifacts/`。

第一阶段 Compact ↔ Expanded Music 已完成。DISPLAY2 实屏为 96 DPI；144/192 DPI 使用合成 DPI 覆盖，未验证物理混合 DPI。Live Activities 已在 0.52–0.56 实现。0.62 增加 Widget Shelf 和网易云模式 UI；网易云实机验收需要正在运行并启用本机 CDP 的客户端。Playing Next 仍等待真实 Queue Provider；GSMTC 不提供队列，因此界面不会编造队列数据。单窗口 UI 2.0 是默认界面；需要回退诊断时可用 `--legacy-ui`。

0.22 为原生 ComboBox 和弹出选项列表提供 Selection / SelectionItem UIA 树；副屏实测列出全部六个时区选项，完成切换、还原和必选约束验证。

0.23 在 DISPLAY2 验证设置键盘 Tab、Space、F4 / Escape 和单选方向键；所有交互状态均恢复到隔离夹具初值。

0.24 在 DISPLAY2 实屏采集设置六个页面及完整时区下拉列表截图，并校验非空像素；修复设置控件主题色、外观卡片间距、折叠式下拉框尺寸和导航选中态刷新。当前两个物理显示器均为 96 DPI；真实混合 DPI 与跨显示器移动验收仍待完成。

0.25 扩展 Settings 键盘回归，覆盖 Shift+Tab 反向焦点移动与 Enter 激活聚焦按钮；副屏 14 项隔离交互验证通过。

0.26 在 DISPLAY2 的隔离 Settings UIA 夹具完成 144 / 192 DPI 渲染与滚动验收；正文视口裁剪在标题与状态栏之间，截图和控件交互分别验证。当前两个物理显示器均为 96 DPI，合成 DPI 验收不代表真实跨屏混合 DPI。

0.27 建立统一 `PlayerKind` 会话身份分类与可选 `PlayerExtension` 契约。网易云时间轴兼容和播放器显示名改用同一分类器；通用播放器扩展能力默认关闭，仍使用现有 GSMTC 路径。

0.28 网易云 CloudMusic / NetEaseMusic 会话标识采用精确段匹配；其他网易系应用、未知标识和相似名称回退为通用播放器，不进入网易云专用处理。

0.29 增加限时限长的本机 CDP `/json/version` 探测，只连接 `127.0.0.1`；并验证 Chromium 返回的 WebSocket 地址必须指向配置端口上的本机浏览器调试端点。

0.30 建立受限 CDP WebSocket 传输：仅本机握手、请求体/响应体限长、超时控制、请求 ID 匹配与事件忽略。尚未接入具体网易云模式或 Like 操作。

0.31 在高级设置中提供可持久化的网易云远程调试端口（默认 9223），限制为 1024–65535，并验证设置写入、重启回读和撤销流程。端口进入运行配置，供后续网易云 CDP 适配读取；播放器页不暴露传输细节，网易云专用命令仍待接入。

0.32 将标准媒体控制统一按 GSMTC 快照能力门控。网易云播放/暂停、上一首、下一首与进度跳转继续走 GSMTC；不支持或缺少有效时间轴时拒绝相应操作，不发送专用 CDP 命令。

0.33 将 GSMTC 命令执行接到可隔离验证的控制器接口。网易云 Next/Previous 的测试用控制器确认只触发对应系统媒体操作，并在能力缺失时不发送命令；真实播放会话的手动切歌验收仍待完成。

0.34 增加网易云 CEF 页面识别与 CDP 只读播放模式读取；仅连接配置端口上的本机页面端点，准确映射顺序/列表循环/单曲循环/随机，AI、FM 与未知模式保持未识别。Loopback mock 集成测试通过；真实网易云运行时验收仍待有活动调试会话时完成。

0.35 增加网易云播放模式切换白名单与读回验证，只允许顺序、列表循环、单曲循环和随机；当前模式为 AI/FM/未知时拒绝派发。Loopback mock 确认 dispatch payload 与目标模式读回匹配；实际客户端写入验收仍待有活动调试会话时完成。

0.36 接入本地私有 MiSans Regular、Medium、Bold 字体，并完成 UI 2.0 动态玻璃轮廓实验。首次 DWM 系统材质方案误将桌面采样盖成灰底，已在 0.37 移除并替换为实际桌面像素折射。

0.37 动态玻璃按 UI 参考保持上方纯黑、仅底部约四分之一渐隐透明；真实桌面像素折射与轮廓高光均限制在透明渐变区，横向边缘和上下边缘柔和收敛，透明外不添加折射像素。捕获失败时回退到纯透明渐变。副屏验收检查纯黑区无高光、彩色桌面透出、96/192 DPI、MiSans 和采样诊断。

0.38 Shared Album 切歌使用同一封面矩形交叉淡入淡出；切歌再次打断过渡时保留当前画面的封面权重并限制最多四层，避免快速切歌丢失过渡内容。DISPLAY2 96/192 DPI 验收通过，封面矩形无可见几何跳变。

0.39 展开播放器先显示 128px 封面；后台 512px 封面就绪后，在同一 Shared Album 矩形内淡入升级，不等待高清图才展开。DISPLAY2 96/192 DPI 验收确认封面矩形稳定，上传和绘制耗时低于 50ms。

0.40 把封面裁切统一为归一化 UV，保证横版、竖版封面在 128/512 两个分辨率层保持一致中心裁切；播放器控件由 `MediaCapabilities` 门控，Seek 命中区跟随动画进度条。Playing Next 仍待真实 Queue Provider，因此相关跨布局裁切验收暂不标完成。

0.41 为 UI-06/07 增加 Leave Grace / pointer-down 诊断，并提供 DISPLAY2 专用回归：验证 150ms 离开宽限、宽限内重入取消、正常超时收起，以及按下期间不误收起。测试只运行隔离 fixture，不触发系统媒体控制。

0.42 扩展交互包络回归，覆盖鼠标拖动跨出区域后的锁定，以及设置 Appearance 检查模式的 inspection lock；这些 fixture 输入只对测试窗口启用。

0.43 为 UI 2.0 的长艺术家名称添加 DirectWrite 单行省略号裁切，并扩展隔离 fixture 到长标题、长艺术家、无媒体和缺失封面状态；测试诊断暴露文本/控件矩形，用于副屏视觉验收。

0.44 将 Dynamic Glass 的折射贴图限制在透明度已从纯黑下降的下部区域（82% 起），渐变前段不再叠加折射；新增断言保证折射起点始终位于纯黑渐变之后。

0.45 让玻璃背景/折射跟随弹簧当前宽高绘制，而不是在收起动画开始时随目标状态立即切回纯黑；窄到紧凑岛尺寸后自动停止采样。DISPLAY2 96/192 DPI 开合过程逐帧截图通过，黑色核心与透明尾部持续稳定。

0.46 为 UI 2.0 增加横向与纵向封面夹具，DISPLAY2 96/192 DPI 截图确认展开视口保持方形，并以封面内部颜色签名比对紧凑态和展开态裁切一致，作为回归门禁。

0.47 将 96×96 封面模糊缓存以 8% 透明度绘制在玻璃表面之下，由黑到透明渐变自然遮住上半区；缓存只在封面源变化时重建。DISPLAY2 的纯黑像素、透明尾部和开合动画回归通过。

0.48 扩展折射采样到完整横向边缘，并在左右 18% 的透明下沿增强位移，呈现圆角 Lens；纯黑上半区仍由像素采样门禁保护。

0.49 在透明尾部的折射采样中加入最高 0.28px 的 RGB 子像素位移，随 DPI 等比缩放；采样条仍从 82% 透明渐变区域开始，纯黑核心不产生色散或光晕。

0.50 增加极亮/极暗封面与白/黑桌面组合的对比度回归；DISPLAY2 96/192 DPI 截图确认纯黑核心、标题前景和播放控件均可辨认。

0.51 扩展单窗口形态回归：同一播放器从紧凑岛展开为完整玻璃播放器并收回；DISPLAY2 96/192 DPI 验证专辑、标题、进度和控件始终留在动画表面内。

0.57 将 UI 2.0 设为默认单窗口界面，并把展开音乐播放器改为参考图的横向排布：封面与元数据并列、完整进度行、居中的传输控制。DISPLAY2 96/192 DPI 的开合截图验证布局不越出玻璃表面。收藏爱心及其 Provider 能力门控仍待下一步完成。

0.58 按参考图收紧 Music UI：430×164 DIP（工具栏 430×202），72 DIP 封面、16/13 DIP 标题与歌手、4 DIP 进度条和对称传输控制。Compact 默认 156×36 DIP，Hover 176×40；已有尺寸设置保留。四边 attached 布局与最大肩部边界测试通过；DISPLAY2 96/144/192 DPI 的长文本、无媒体、缺失封面及横竖封面裁切回归通过。物理显示器仍为 96 DPI，高 DPI 为合成测试。

0.59 标题和歌手在目标附近以 5 DIP 位移显现，进度从中点展开并沿原路径缩回；首次展开不再从宿主零坐标进入。所有内容沿已有 Spring 连续变化，中途反向保留当前位置、速度与透明度；Reduced Motion 立即稳定。完整构建门禁、五档合成 DPI×四边、30 次开合、快速反向、Seek 与 idle 停帧通过；DISPLAY2 96/144/192 DPI 的内容截图及 leave grace、drag、inspection lock 回归通过。

0.60 收紧底部动态玻璃：主要控件后方保持深黑，封面只以连续渐变轻微着色，桌面折射限制在内沿 2.5 DIP，并降低色散。封面与表面使用同一 Spring 节奏，首次 UI 初始化将容器、封面和已保存的 Compact 尺寸对齐。新增四边、attached/floating 开合期间封面 containment 测试。Rust 全工作区格式、check、clippy、test 和 release build 通过；DISPLAY2 96/144/192 DPI 的纯黑核心、透明尾部、明暗桌面及封面极值、开合截图、长文本/无封面/无媒体和横竖封面裁切回归通过。物理显示器为 96 DPI，高 DPI 使用合成测试。

0.61 完成 UI 第一阶段验收与 Review。Windows 上 `cargo fmt --all -- --check`、`cargo check --workspace --locked`、`cargo clippy --workspace --all-targets --locked -- -D warnings`、`cargo test --workspace --locked`（160 passed，1 个显式 opt-in 网络测试 ignored）及 `cargo build --release --bin isle-native --locked` 全部通过。DISPLAY2 上 bright/dark artwork × white/black desktop 的 12 组 96/144/192 DPI 对比、透明尾部与黑色核心、玻璃展开/收起逐帧、长标题/无媒体/无封面、横竖封面裁切、四边布局、attached/floating、30 次开合和快速反向回归通过。144/192 DPI 为合成 DPI；当前连接的物理显示器均为 96 DPI，未完成物理混合 DPI 验收。诊断采样最高 render CPU P95 为 2.72 ms、draw/present P95 为 8.78 ms；静止两秒帧数不增长。

0.62 增加可配置 Widget Shelf：实时摘要来自应用已有媒体、音量、计时、时钟与天气状态；系统状态在没有数据源时显示不可用，不生成示例数值。小组件开关经设置服务保存，未知 ID 和嵌套字段继续保留；布局/键盘焦点/配置重启回读回归通过。网易云页面异步读取本机 CDP 播放模式，模式写入限于白名单并要求读回匹配；Loopback mock 协议测试通过。本机没有网易云进程或 9223 监听端口，未做网易云实机验收。`Playing Next` 继续等待真实队列提供方。版本 0.62 的 fmt/check/clippy/release build 通过；workspace tests 164 passed、1 个 opt-in 网络测试 ignored；DISPLAY2 Shelf/网易云截图、Activity 回归和 Settings UIA 16 项交互通过。

0.21 Settings 为单选组根窗口提供 Selection、Radio 控件提供 SelectionItem；副屏隔离测试验证单选切换、容器关联、必选约束和原值恢复。

0.20 Settings 为原生按钮、复选框、文本框、滑块和组合框接入对应 UIA 控件类型与 Invoke、Toggle、Value、RangeValue、ExpandCollapse 模式。隔离配置下在 `\\.\DISPLAY2` 实测导航、复选框切换并还原、滑块设值并还原、组合框展开/收起、文本框设值并还原；主窗口与设置窗口都通过副屏检查，进程正常退出。ComboBox 项选择、键盘矩阵、Settings 专项视觉及真实混合 DPI 仍待验收。

0.19 Settings 无障碍探针校验主窗口与设置窗口实际位于 `\\.\DISPLAY2`，并修正 MSAA 原生 COM 接口声明。副屏只读清点得到 11 个后代控件，按钮、复选框、静态文本和组合框的 MSAA 角色与默认操作可读，进程正常退出且未强制结束。

2026-10-03：当前源码在 Segoe UI 回退字体环境中成功启动并打开 Settings，历史 HRESULT 尚未复现，根因仍待查。0.15 副屏 Settings 生命周期验收：在 `\\.\DISPLAY2` 完成 30 次打开、切页、缩放、重绘和关闭；热身后 GDI 句柄连续保持 20，关闭后窗口均已销毁。0.17 副屏可访问性验收：主窗口 UIA 桥接及 MSAA 名称、角色、导航、焦点、边界、音量范围写入/拒绝越界和失效句柄检查通过；Settings 窗口完成 11 个 UIA 后代的只读清点，并正常退出。0.18 副屏交互与截图验收：8 种悬浮/贴边布局命中测试通过；150% DPI 音乐和 200% DPI/320×240 小工作区天气截图通过非空及副屏边界检查。Settings UIA 控件交互、Settings 专项视觉和真实混合 DPI 验收仍待完成。

0.16 错误诊断：启动、服务初始化、渲染器重建、运行事件和关闭路径返回阶段标签；保留 HRESULT/OS 错误号，临时错误日志记录时间和回溯。历史启动错误仍需真实复现以确定根因。

0.13 副屏验收：在 `\\.\DISPLAY2`（1920×1080，原点 `(-1920, 0)`）完成原生 Settings、原生及 UI 2.0 播放器、UI 2.0 Settings 放置验证；窗口均保持在副屏工作区。32 组 DPI/边缘布局及隐藏、最小化、恢复回归通过。该轮验证未改变系统显示器布局或 DPI 设置。

0.14 Dynamic Glass：UI 2.0 展开播放器上半部保持纯黑，下半部通过纵向透明度渐变融入桌面；以暗色内缘和低亮度边缘高光表现折射，保留现有动画轮廓和输入命中路径。

Windows CI 执行 fmt、check、Clippy、工作区测试及 Release 构建。CI 结果以 GitHub Actions 实际运行记录为准，构建通过不代表 UI 实机验收通过。

0.12 修复首轮远端 CI 的 Clippy 阻塞：blur 缩略图采样改用 `checked_div`，保持空样本为透明像素，继续将全部 Clippy 警告作为错误。当前播放器布局保持不变。

0.52 将真实倒计时状态接入同一活动队列：运行、暂停、恢复、重置均同步剩余时间和进度；完成状态只保留 3 秒并自然过期，不会被逐帧重新创建。活动按优先级争用两个展示槽，计时器不会伪造媒体数据。

0.53 将系统确认的音量与静音状态接入同一活动队列；只有相对于已识别端点的真实变化才发布，显示真实百分比/静音状态并在 2.5 秒后清除。独立到期定时器在静止界面也会准确清理提示。

0.54 在同一个灵动岛 HWND 和渲染路径中为最高优先级的两个 Activity 增加紧凑侧槽；槽位跟随四边布局并限制在宿主范围，展开时隐藏。DISPLAY2 通过 0/1/2/3 项数量、可见像素、窗口输入命中、32 组 DPI/生命周期和 Dynamic Glass 黑区/透明尾部回归。

0.55 将有界 `ActivityManager` 接入 UI 模型与到期唤醒：队列最多 32 项，优先级更新即时争用两个展示槽，同一 ID 原位更新，隐藏队列项目也按 TTL 清理，到期后立即补位；内建计时器与音量活动统一参与仲裁。

0.56 为 Activity 槽位加入可逆弹簧入场/退场动画。进入从主岛中心展开，退出回收至中心；动画中途反向保留实时位置和速度，并同步驱动同一 HWND 的圆角输入区域。

## 旧版灵动岛 UI 对齐（2026-10-07）

参考固定为旧仓库提交 [`a9a855e`](https://github.com/alent17/Wind0ws_Dynamic_island/tree/a9a855ef62d2596775115c70b82f63af8a67229e)。尺寸来自 `islandGeometry.ts` 的 `geometryFor` / `navigationGeometry`、`App.svelte` 的工作区缩放与 `components/island/IslandSurface.svelte` 的最终 CSS。不能将文件开头的 600×210 当作实际音乐展开尺寸；最终导航布局覆盖它为 600×249.1。

| 状态 | 基础逻辑尺寸 | 圆角 |
| --- | --- | --- |
| 收起 / 隐藏轮廓 | 80×28；已有自定义长度继续生效 | 14 |
| 悬停 | 90×30；自定义长度 +10，上限 300 | 15 |
| 倒计时收起 | 最短 240×28；悬停 250×30 | 14 / 15 |
| 音乐展开 | 600×249.1 | 默认 45 |
| 音量 / 倒计时 / 时钟展开 | 300×188 | 默认 45 |
| 天气展开 | 300×240 | 默认 45 |

展开尺寸、字号和圆角乘以 `s = clamp(min(工作区物理宽 / DPI倍率 / 2100, 工作区物理高 / DPI倍率 / 1180), 0.62, 1) × 0.625`，之后才转换为设备像素。收起尺寸不乘 `s`。侧边收起互换宽高；工具栏基础高度额外 +40；侧边贴靠展开高度额外 +2×肩部，顶部/底部非音乐页宽度额外 +2×肩部。浮动间距为 22×s。肩部轮廓保持逻辑像素；原生导航控件保留必要肩部内缩，避免较小工作区里返回按钮落在轮廓外。

无工具栏、浮动音乐页：s=0.625 时为 375×155.6875 DIP，封面为 52.5×52.5 DIP；1920×1040 工作区、96 DPI 时 s≈0.55084746，音乐岛约 330.5085×137.2161 DIP。Chromium 的 1/64 像素布局量化会产生小于 1 像素的尾差。

封面基础尺寸 84×84、圆角 20；标题/歌手为 MiSans Bold 24 / Medium 18，封面与元数据间距 24；音乐内边距左右 32、顶部 24、底部 10；进度轨道高 10；传输按钮 38×48、间距 30，上一首/下一首图标 32、播放图标 38。CSS 边框占用的 1 DIP 在内容缩放之外处理。主体按本次要求统一为不透明 `#000000`，禁用背景封面模糊、桌面折射和透明尾部；旧版展开 CSS 的蓝灰底部渐变也不复刻。

外壳保留 Svelte 的 stiffness=0.18、damping=0.8、precision=0.01 弹簧采样轨迹，移除原生版额外的外壳时间倍率；封面展开采用旧版 250ms 三次缓出、收起隐藏两个端点；展开内容沿旧版 outwardProgress 的 0.18/0.42 阈值显现。页面退出 100ms、进入 180ms ease 的原有路径保留。播放/收藏/随机/循环能力门控、活动队列、媒体控制、设置和数据服务保持原有行为。原生独有的 Shelf、网易云独立页没有旧版对应页面，继续保留原生内容和命令。

[测量数据](reference-ui/legacy-measurements.json)与[旧版收起截图](reference-ui/legacy-compact.png)、[旧版展开截图](reference-ui/legacy-expanded.png)来自 Chromium、1920×1040 视口、deviceScaleFactor=1、MiSans 字体、Reduced Motion。截图是旧版基准，不是原生版验收截图；旧版展开截图保留了其蓝灰渐变，目标原生背景按要求为纯黑。

验证：Linux 下 isle-core 46 项与 isle-ui 60 项测试通过，包含工作区/DPI 缩放、浏览器测量基准、四边/贴靠导航绘制与命中映射及原有业务回归；格式检查通过。Windows GNU 目标的全工作区/all-targets Check 和 Clippy（警告作为错误）通过，使用临时验证副本跳过了依赖 Windows SDK 的资源编译步骤，正式仓库的构建脚本未改变。修正了原本缺少字段的测试初始化及五处与现有行为不符的测试夹具；三个非本次布局文件只有 rustfmt 格式变化。

尚未验收：Windows SDK 资源编译与 Release 构建、Direct2D/DirectWrite 实屏抗锯齿、相同工作区/DPI下的新旧逐像素和逐帧截图、播放图标与滚动数字的视觉过渡。当前原生渲染器与 Chromium 字形栅格化不同，不能仅凭代码或 Linux 检查声称已完成所有状态的 1:1 像素验收。

在有字体使用许可的环境中，先执行 `python scripts/fetch-reference-fonts.py`，即可从上述固定旧提交获取并校验同一组 MiSans 字体至现有的 `local-only/assets/fonts/` 私有加载目录。字体文件不提交到本仓库；不获取字体会使用 Segoe UI，无法满足字形一致性。

## MiSans 字体

MiSans 字体文件保存在被 Git 忽略的 `local-only/assets/fonts/` 中。若你已取得相应使用许可，可将 `MiSans-Regular.ttf`、`MiSans-Medium.ttf`、`MiSans-Bold.ttf` 放入该目录。构建会将找到的字重复制到可执行文件旁的 `fonts/`，并通过进程私有字体集合加载；缺少字体时程序回退到 Segoe UI。

## 许可证

程序代码按 [MIT License](LICENSE) 发布。Lucide 图标的许可见 `assets/icons/LICENSE`。MiSans 字体不包含在仓库内；请遵守[官方许可协议](https://hyperos.mi.com/font-download/MiSans%E5%AD%97%E4%BD%93%E7%9F%A5%E8%AF%86%E4%BA%A7%E6%9D%83%E8%AE%B8%E5%8F%AF%E5%8D%8F%E8%AE%AE.pdf)。
