# Isle 原生 UI 原型

独立的 Windows x64 原型。使用 Rust、Win32、Direct2D、DirectWrite、DXGI 与 DirectComposition；不依赖 Tauri、Wry 或 WebView2。现有正式程序的源码与设置保持原样。

## 构建与运行

需要 Rust MSVC 工具链、Visual Studio C++ Build Tools 和 Windows SDK。

```powershell
cargo build --release --manifest-path native/Cargo.toml
./native/target/release/isle-native.exe
```

启动即显示独立原型岛。点击空白展开/收起，Alt+F4 退出。原型不注册开机启动、不读取或写入旧版设置、不占用旧版快捷键。

```powershell
./native/target/release/isle-native.exe --page music --attached --long-title
./native/target/release/isle-native.exe --page weather --edge left --attached
./native/target/release/isle-native.exe --page volume --paused --reduced-motion
```

| 操作 | 行为 |
|---|---|
| F1 / F2 / F3 / F4 | 顶部 / 右侧 / 底部 / 左侧 |
| F5 | 切换悬浮和贴边 |
| F6 | 切换减少动画 |
| F7 | 切换长短标题 |
| 方向键 / Tab，Enter | 选择并打开工具 |
| Escape | 详情返回音乐，音乐收起 |
| 拖动工具栏或滚轮 | 浏览后面的时间、天气按钮 |
| Space | 切换模拟播放状态 |

`--tools 0..7` 用于工具数量验证；`--paused` 停止模拟频谱；`--exit-after 秒数 --log 路径` 用于自动采样；`--scripted` 循环切页。

## 已实现范围

- 透明合成窗口、浮动和四边凹肩轮廓；可见轮廓与原生窗口区域共用几何数据。
- 可中断的宽、高、圆角、凹肩弹簧；静止时停止动画计时器，按需绘制。
- 纯图标紧凑工具栏、横向拖动、返回、空白收起与手势取消。
- 音乐、音量、倒计时、时间、天气五个原型页；只有当前页面实例，收起释放页面。
- 本地倒计时截止时间独立于页面；时间读取系统本地时间。
- 长标题往返滚动和渐变，减少动画模式静止。

**这是绘制和交互原型，尚不能替换日常版本。** 音乐、封面图形、频谱、音量及天气是演示数据；浮动播放器和设置按钮仅显示说明。真实媒体、设备列表、网络天气、Studio、托盘、无障碍语义和视频均未迁移。系统减少动画偏好自动读取、多 DPI 实机、设备丢失、睡眠恢复和旧版动画轨迹校准仍待验收。

页面状态是 `Option<PageInstance>`，切换直接替换，收起设为 `None`。当前原型页面没有独占网络请求、Canvas 或后台采集任务；渲染器保留有界的字体和两个演示标题布局缓存。该模型测试不能替代未来真实业务接入后的订阅和请求释放测试。

## 验证

```powershell
cargo fmt --manifest-path native/Cargo.toml --all -- --check
cargo clippy --manifest-path native/Cargo.toml --workspace --all-targets -- -D warnings
cargo test --manifest-path native/Cargo.toml --workspace
python native/scripts/interaction.py
./native/scripts/measure.ps1 -Seconds 60 -Repetitions 3
```

截图脚本需要 Pillow。`interaction.py` 按进程 ID 定位自己的窗口，并创建另一个进程的背景窗口检查透明区域的窗口路由；不会向安装版 Isle 发送指令。该检查使用 `WindowFromPoint` 和向返回窗口发送消息，不等价于鼠标硬件输入的全链路验收。

所有测试输出位于忽略提交的 `native/artifacts/`。性能报告里的 `drawAndPresentP95Ms` 包含 Present 等待，**不是帧间隔，也不是输入延迟**。CPU 百分比按机器全部逻辑处理器归一化；私有内存和工作集分别记录。

实现与验收进展见 [首轮记录](../docs/native-ui-prototype-results.md)。
