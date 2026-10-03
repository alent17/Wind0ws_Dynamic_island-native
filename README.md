# Isle Native UI

独立的 Windows 原生灵动岛应用，使用 Rust、Win32、Direct2D、DirectWrite、DXGI 和 DirectComposition。运行时不依赖 Tauri、Wry 或 WebView2。

当前版本：**0.18**（Cargo / Windows 文件版本 `0.18.0`）。版本序列从 0.10 延续，每个完成并验证的步骤递增版本，在 `main` 提交并推送。

原 Tauri/WebView 项目位于 [Wind0ws_Dynamic_island](https://github.com/alent17/Wind0ws_Dynamic_island)。

## 功能

- Windows 系统媒体会话、封面、播放控制和进度。
- 灵动岛收起/展开、四边贴靠、动画与实时频谱。
- 系统音量与输出设备、时钟、天气和城市设置。
- 独立悬浮播放器、倒计时和原生设置窗口。
- 多显示器支持及基础 MSAA/UIA 桥接。

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

当前优先级是 P0 稳定性：继续调查历史 `0x80070057`，完成 Settings UIA 交互、视觉和混合 DPI 验收，再继续网易云专用适配、Full Player、Glass、Live Activities 和 Widget Shelf。UI 2.0 仍通过 `--ui-v2` 启用。

2026-10-03：当前源码在 Segoe UI 回退字体环境中成功启动并打开 Settings，历史 HRESULT 尚未复现，根因仍待查。0.15 副屏 Settings 生命周期验收：在 `\\.\DISPLAY2` 完成 30 次打开、切页、缩放、重绘和关闭；热身后 GDI 句柄连续保持 20，关闭后窗口均已销毁。0.17 副屏可访问性验收：主窗口 UIA 桥接及 MSAA 名称、角色、导航、焦点、边界、音量范围写入/拒绝越界和失效句柄检查通过；Settings 窗口完成 11 个 UIA 后代的只读清点，并正常退出。0.18 副屏交互与截图验收：8 种悬浮/贴边布局命中测试通过；150% DPI 音乐和 200% DPI/320×240 小工作区天气截图通过非空及副屏边界检查。Settings UIA 控件交互、Settings 专项视觉和真实混合 DPI 验收仍待完成。

0.16 错误诊断：启动、服务初始化、渲染器重建、运行事件和关闭路径返回阶段标签；保留 HRESULT/OS 错误号，临时错误日志记录时间和回溯。历史启动错误仍需真实复现以确定根因。

0.13 副屏验收：在 `\\.\DISPLAY2`（1920×1080，原点 `(-1920, 0)`）完成原生 Settings、原生及 UI 2.0 播放器、UI 2.0 Settings 放置验证；窗口均保持在副屏工作区。32 组 DPI/边缘布局及隐藏、最小化、恢复回归通过。该轮验证未改变系统显示器布局或 DPI 设置。

0.14 Dynamic Glass：UI 2.0 展开播放器上半部保持纯黑，下半部通过纵向透明度渐变融入桌面；以暗色内缘和低亮度边缘高光表现折射，保留现有动画轮廓和输入命中路径。

Windows CI 执行 fmt、check、Clippy、工作区测试及 Release 构建。CI 结果以 GitHub Actions 实际运行记录为准，构建通过不代表 UI 实机验收通过。

0.12 修复首轮远端 CI 的 Clippy 阻塞：blur 缩略图采样改用 `checked_div`，保持空样本为透明像素，继续将全部 Clippy 警告作为错误。当前播放器布局保持不变。

## MiSans 字体

出于字体许可条款，本仓库不再携带 MiSans 字体文件。若你已从[小米官方字体页面](https://hyperos.mi.com/font/zh/download/)取得相应使用许可，可将 `MiSans-Regular.ttf`、`MiSans-Medium.ttf`、`MiSans-Bold.ttf` 放入仓库根目录的 `public/fonts/`。构建会将找到的字重复制到可执行文件旁的 `fonts/`；缺少字体时程序回退到 Segoe UI。

## 许可证

程序代码按 [MIT License](LICENSE) 发布。Lucide 图标的许可见 `assets/icons/LICENSE`。MiSans 字体不包含在仓库内；请遵守[官方许可协议](https://hyperos.mi.com/font-download/MiSans%E5%AD%97%E4%BD%93%E7%9F%A5%E8%AF%86%E4%BA%A7%E6%9D%83%E8%AE%B8%E5%8F%AF%E5%8D%8F%E8%AE%AE.pdf)。
