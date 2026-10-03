# Isle Native UI

独立的 Windows 原生灵动岛应用，使用 Rust、Win32、Direct2D、DirectWrite、DXGI 和 DirectComposition。运行时不依赖 Tauri、Wry 或 WebView2。

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
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

自动化脚本位于 `scripts/`，测试结果写入被忽略的 `artifacts/`。集成测试使用自己的测试进程和隔离配置，不代表所有硬件、DPI、屏幕阅读器及视觉场景均已验收。

## 迁移与验收状态

UI 2.0 的计划、进度、QA 记录和独立审查位于 `docs/native-ui-v2-*.md`。当前源码检查、Clippy 和单元测试通过；最近的隔离设置窗口启动回归仍出现 `0x80070057` 参数错误，根因待查。UIA 实机、视觉及混合 DPI 验收也仍待完成。请以进度表和 QA 记录为准。

## MiSans 字体

出于字体许可条款，本仓库不再携带 MiSans 字体文件。若你已从[小米官方字体页面](https://hyperos.mi.com/font/zh/download/)取得相应使用许可，可将 `MiSans-Regular.ttf`、`MiSans-Medium.ttf`、`MiSans-Bold.ttf` 放入仓库根目录的 `public/fonts/`。构建会将找到的字重复制到可执行文件旁的 `fonts/`；缺少字体时程序回退到 Segoe UI。

## 许可证

程序代码按 [MIT License](LICENSE) 发布。Lucide 图标的许可见 `assets/icons/LICENSE`。MiSans 字体不包含在仓库内；请遵守[官方许可协议](https://hyperos.mi.com/font-download/MiSans%E5%AD%97%E4%BD%93%E7%9F%A5%E8%AF%86%E4%BA%A7%E6%9D%83%E8%AE%B8%E5%8F%AF%E5%8D%8F%E8%AE%AE.pdf)。