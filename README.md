# Isle Native UI

独立的 Windows 原生灵动岛应用，使用 Rust、Win32、Direct2D、DirectWrite、DXGI 和 DirectComposition。运行时不依赖 Tauri、Wry 或 WebView2。

当前版本：**0.36**（Cargo / Windows 文件版本 `0.36.0`）。版本序列从 0.10 延续，每个完成并验证的步骤递增版本，在 `main` 提交并推送。

原 Tauri/WebView 项目位于 [Wind0ws_Dynamic_island](https://github.com/alent17/Wind0ws_Dynamic_island)。

## 功能

- Windows 系统媒体会话、封面、播放控制和进度。
- 灵动岛收起/展开、四边贴靠、动画与实时频谱。
- 系统音量与输出设备、时钟、天气和城市设置。
- 独立悬浮播放器、倒计时和原生设置窗口。
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

本项目按单 Agent 路线图推进。GitHub 仅维护本 README 作为项目说明和状态入口；路线图、参考图片、项目状态、QA 文档及性能记录集中保存在仓库内的 `local-only/` 并由 Git 忽略。代码、构建脚本和许可证继续纳入版本控制。

当前优先级是 P0 稳定性：记录历史 `0x80070057` 尚未复现的条件，并在可用的不同 DPI 显示器上完成真实跨屏验收；随后继续网易云专用适配、Full Player、Glass、Live Activities 和 Widget Shelf。UI 2.0 仍通过 `--ui-v2` 启用。

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

0.36 在 UI 2.0 展开播放器的透明层下启用 Windows DWM Desktop Acrylic，并将轮廓内缘高光扩为渐变玻璃带；不支持系统材质时保留透明渐变。副屏在 96 DPI 与合成 200% DPI 完成视觉检查；真实采样位移式边缘折射仍需继续打磨。

0.21 Settings 为单选组根窗口提供 Selection、Radio 控件提供 SelectionItem；副屏隔离测试验证单选切换、容器关联、必选约束和原值恢复。

0.20 Settings 为原生按钮、复选框、文本框、滑块和组合框接入对应 UIA 控件类型与 Invoke、Toggle、Value、RangeValue、ExpandCollapse 模式。隔离配置下在 `\\.\DISPLAY2` 实测导航、复选框切换并还原、滑块设值并还原、组合框展开/收起、文本框设值并还原；主窗口与设置窗口都通过副屏检查，进程正常退出。ComboBox 项选择、键盘矩阵、Settings 专项视觉及真实混合 DPI 仍待验收。

0.19 Settings 无障碍探针校验主窗口与设置窗口实际位于 `\\.\DISPLAY2`，并修正 MSAA 原生 COM 接口声明。副屏只读清点得到 11 个后代控件，按钮、复选框、静态文本和组合框的 MSAA 角色与默认操作可读，进程正常退出且未强制结束。

2026-10-03：当前源码在 Segoe UI 回退字体环境中成功启动并打开 Settings，历史 HRESULT 尚未复现，根因仍待查。0.15 副屏 Settings 生命周期验收：在 `\\.\DISPLAY2` 完成 30 次打开、切页、缩放、重绘和关闭；热身后 GDI 句柄连续保持 20，关闭后窗口均已销毁。0.17 副屏可访问性验收：主窗口 UIA 桥接及 MSAA 名称、角色、导航、焦点、边界、音量范围写入/拒绝越界和失效句柄检查通过；Settings 窗口完成 11 个 UIA 后代的只读清点，并正常退出。0.18 副屏交互与截图验收：8 种悬浮/贴边布局命中测试通过；150% DPI 音乐和 200% DPI/320×240 小工作区天气截图通过非空及副屏边界检查。Settings UIA 控件交互、Settings 专项视觉和真实混合 DPI 验收仍待完成。

0.16 错误诊断：启动、服务初始化、渲染器重建、运行事件和关闭路径返回阶段标签；保留 HRESULT/OS 错误号，临时错误日志记录时间和回溯。历史启动错误仍需真实复现以确定根因。

0.13 副屏验收：在 `\\.\DISPLAY2`（1920×1080，原点 `(-1920, 0)`）完成原生 Settings、原生及 UI 2.0 播放器、UI 2.0 Settings 放置验证；窗口均保持在副屏工作区。32 组 DPI/边缘布局及隐藏、最小化、恢复回归通过。该轮验证未改变系统显示器布局或 DPI 设置。

0.14 Dynamic Glass：UI 2.0 展开播放器上半部保持纯黑，下半部通过纵向透明度渐变融入桌面；以暗色内缘和低亮度边缘高光表现折射，保留现有动画轮廓和输入命中路径。

Windows CI 执行 fmt、check、Clippy、工作区测试及 Release 构建。CI 结果以 GitHub Actions 实际运行记录为准，构建通过不代表 UI 实机验收通过。

0.12 修复首轮远端 CI 的 Clippy 阻塞：blur 缩略图采样改用 `checked_div`，保持空样本为透明像素，继续将全部 Clippy 警告作为错误。当前播放器布局保持不变。

## MiSans 字体

MiSans 字体文件保存在被 Git 忽略的 `local-only/assets/fonts/` 中。若你已取得相应使用许可，可将 `MiSans-Regular.ttf`、`MiSans-Medium.ttf`、`MiSans-Bold.ttf` 放入该目录。构建会将找到的字重复制到可执行文件旁的 `fonts/`，并通过进程私有字体集合加载；缺少字体时程序回退到 Segoe UI。

## 许可证

程序代码按 [MIT License](LICENSE) 发布。Lucide 图标的许可见 `assets/icons/LICENSE`。MiSans 字体不包含在仓库内；请遵守[官方许可协议](https://hyperos.mi.com/font-download/MiSans%E5%AD%97%E4%BD%93%E7%9F%A5%E8%AF%86%E4%BA%A7%E6%9D%83%E8%AE%B8%E5%8F%AF%E5%8D%8F%E8%AE%AE.pdf)。
