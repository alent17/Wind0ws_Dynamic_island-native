# Settings Window Placement 设计与实现记录

状态：**设计已审阅，代码已实现，统一软件门禁通过；真实 Windows UI / 多屏 DPI 验收待完成**。本文件记录已批准接口、实际实现与尚待验证的边界。源代码与自动化测试不构成截图、真实多屏或硬件 DPI 验收证据。

## 目标与现状

Settings 窗口恢复上次手动位置和尺寸；显示器被移除或保存位置不可达时，将窗口恢复到可操作的工作区。位置写入既有配置文档，并复用 Runtime Settings 的 latest-wins 快照保存、配置备份、冲突检测和损坏保护。

原 `LAST_RECT` 只在窗口销毁时写入、从未读取，不能跨重启恢复。实现已将手动位置加入现有 Runtime Settings 快照和后台保存通道；没有新增 sidecar 或独立保存队列。配置写盘仍复用原子替换、锁、baseline 检查、备份及外部修改保护。

## 数据和 API

### Core placement 类型

在 `crates/isle-core/src/settings.rs` 定义并导出：

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettingsWindowPlacement {
    pub x: i32,
    pub y: i32,
    pub width_dip: u32,
    pub height_dip: u32,
    pub dpi: u32,
}
```

坐标和大小语义见下节。提供集中验证方法，拒绝零尺寸、不可接受的超大尺寸、无效 DPI 和任何可能导致物理像素换算溢出的值；负 `x` / `y` 是合法的副屏坐标。建议初始上限为每个尺寸 8192 DIP、DPI 48–960，并在实现时以 checked 整数运算完成 DPI 换算。验证范围应作为测试固定下来。

### Runtime 快照和 patch

在 `SettingsSnapshot` 中加入：

```rust
pub window_placement: Option<SettingsWindowPlacement>,
```

在 `SettingsPatch` 中加入单一 typed variant：

```rust
WindowPlacement(Option<SettingsWindowPlacement>),
```

`Some` 表示保存新手动位置，`None` 表示清除已保存位置。`apply_patch` 继续先对快照副本验证，再原子更新 Runtime；相同值不增加 revision。位置与其他设置共享同一完整 Pending / SaveRequest，不额外建立位置保存队列。

### Document 序列化

通过 `crates/isle-core/src/configuration.rs` 的 `Document` 显式读取和写入 JSON key `settingsWindowPlacement`：

```rust
pub fn window_placement(&self) -> Result<Option<SettingsWindowPlacement>, String>;
pub fn set_window_placement(
    &mut self,
    placement: Option<&SettingsWindowPlacement>,
) -> Result<(), String>;
```

JSON 对象使用 camelCase 字段，例如 `{"x":-1920,"y":120,"widthDip":820,"heightDip":760,"dpi":144}`。缺 key 表示未保存；setter 只改这个 key，`None` 删除它。`Document::parse` / `preferences()` 应验证这个新增的已知 key；类型错误或越界值按现有已知配置错误策略拒绝，原始文件保持不变。未知根字段及未知嵌套字段继续原样保留。

为遵守本工作包的 ownership，不改旧版 `AppPreferences` 模型或 `preferences.rs`；placement 是原生 `Document` 中的显式可选字段。旧配置没有该字段时照常加载，首次仅因默认开窗而不写入新字段。

### App Service 和窗口 facade

在 `crates/isle-app/src/configuration.rs`：

- `Service::new` 从 `Document` 初始化 `SettingsSnapshot.window_placement`，并提供按引用读取的 `window_placement()`。
- patch/edit 映射覆盖新 variant；位置仍存于完整 runtime snapshot，不重复维护一份可变 public field。
- `Store::save_snapshot` 将位置写回克隆的 `Document`，与城市、外观、控制、播放器和 Widget 一起通过现有 `commit` 流程保存。
- 保留现有配置 worker、错误 / retry / revert、锁、baseline 冲突保护、备份和有限退出 flush；不增加 sidecar 文件。

在 `crates/isle-app/src/settings/window.rs` 和 `settings/mod.rs` 保留原 `new_v2(owner, controls, appearance)` 签名，新增明确的 `new_v2_with_placement(owner, controls, appearance, Option<SettingsWindowPlacement>)`。旧入口委托新入口并传 `None`，以保留现有调用者及 legacy facade。A0 负责在 `main.rs` 将 Service 中的位置传入新入口，并处理设置窗口发出的 typed 更新；此设计工作不修改 `main.rs`。

窗口 facade 提供 `take_placement_update()`，读取一次由 Win32 消息过程记录的最终值；Legacy facade 保留原行为并返回无更新。通过已有 `COMMAND` action `PLACEMENT_CHANGED = 111` 发“有更新”通知，主线程再从 facade 取 Rust 类型，不把指向临时 Rust 对象的裸指针塞入 `LPARAM`。A0 已在 `main.rs` 接入保存位置、事件处理和关闭前 drain；`Edit::WindowPlacement` 对 Isle 运行时行为不产生副作用。

## 坐标和保存时序

进程已使用 Per-Monitor-V2 DPI awareness。持久值约定如下：

- `x`, `y`：`GetWindowRect` 返回的外窗左上角物理屏幕像素坐标，采用有符号整数；副屏位于主屏左侧或上方时允许负数。
- `width_dip`, `height_dip`：外窗矩形的宽高 DIP（包括标题栏和边框），不是 client 区域尺寸。
- `dpi`：用户最后一次手动移动 / 缩放时 `GetDpiForWindow` 的 DPI，用于校验及记录采样环境。恢复尺寸时使用目标显示器的**当前 DPI**，不沿用旧 DPI。
- 物理宽高换算：`round(DIP × current_dpi / 96)`，所有中间运算使用 checked / 足够宽的整数类型；位置仍按物理绝对坐标解释。

仅在 `WM_ENTERSIZEMOVE` 记录用户开始了窗口移动 / 缩放，并在对应 `WM_EXITSIZEMOVE` 读取一次最终外窗 RECT 与当前 DPI。每个交互只产生一个 placement patch；不得在每条 `WM_MOVE` / `WM_SIZE` 中写配置。`WM_DPICHANGED` 自身的系统建议位置变化不视为用户选择，不单独持久化。`take_placement_update()` 在父窗口退出时也会收取仍在进行中的手动移动；关闭补取只在本次会话有真实手动操作且值不同于最后上报值时提交。

首开默认位置、程序初始化 `SetWindowPos`、DPI 自动调整和拓扑变化都**不自动写盘**。单纯打开再关闭 Settings 不应把临时默认位置变成用户偏好。手动移动 / 缩放产生的有效保存位置具有优先级；和普通 settings patch 一样进入 `RuntimeSettings`，建议使用 150–250 ms debounce，与同时发生的其他改动合并成同一个最新完整快照。

## 恢复和默认避让规则

在 Settings 首次显示前确定最终 RECT 和 DPI，避免窗口先出现在默认位置再跳动：

1. 枚举当前显示器及物理 `rcWork`。以保存的物理原点和保存 DPI 换算出的旧外窗大小构造候选矩形，找与它有有效交叠的当前显示器；不能只用 `MonitorFromRect(...NEAREST)` 判定存在，因为它会为已断开的屏幕返回最近显示器。
2. 找到对应显示器后，按该显示器当前 DPI 将保存的 DIP 外窗尺寸转换为物理像素；把最终矩形完整夹回其 `rcWork`。若工作区小于常规窗口尺寸，沿用现有小工作区适配逻辑将窗口缩小到可用区域。有效的手动保存位置优先，夹回可达范围后不再因 Isle 重叠而自动改位。
3. 保存位置缺失、损坏（解析阶段拒绝）、完全不落在任何现有显示器，或无法构造可用矩形时，回退到 Isle owner 当前显示器。此路径沿用 `monitor_position`：优先右侧 / 左侧并避开 Isle，空间不足时在工作区内居中并按现有策略适配。
4. 窗口在 `ShowWindow` 前已经有最终 RECT；程序设置位置不触发保存。

无持久 Monitor ID 是刻意的最小实现：当显示器拓扑变化后，如果原物理位置恰好落到另一块显示器上，按物理坐标和当前 `rcWork` 处理；无法推断用户想跟随哪台物理显示器。该限制需保留在风险记录中。

## 窄工作区和键盘可达性

窄窗口下，水平滚动条控制 Settings 内容区；Direct2D 的标题、说明和卡片，与原生子控件使用同一横向偏移并各自裁切到内容视口。左侧导航和底部保存状态栏保持固定。滚动条可拖动或用键盘操作；焦点在内容控件中时，`Ctrl+Shift+Left/Right` 可按固定步长横移，Tab 顺序仍覆盖最右侧控件。窗口最小跟踪宽高按当前显示器工作区上限裁切；小工作区默认布局会缩小窗口。`WM_DPICHANGED` 继续刷新 Direct2D DPI、原生控件字体及控件位置。

短窗口导航由 `settings/model.rs` 的纯函数 `navigation_layout(client_height)` 提供共享矩形，Shell 高亮和六个原生导航 HWND 使用同一布局；resize 与 DPI 变更都重新定位按钮。正常高度保留原六行，中等高度压缩行距，低于 270 DIP 隐藏侧栏品牌并改为 2×3 导航。六个页面在紧凑布局中仍可见、可点击且保持键盘 Tab 顺序。纯几何测试覆盖正常高度、270/320 DIP 和 220 DIP（小窗口按 200% DPI 的短客户区），断言矩形在客户区内且互不重叠。

## 验证计划

### Core 与 Document（已添加；统一 workspace gate 通过）

- patch 接受负物理坐标和正常尺寸；相同 patch 不增加 revision；无效 DPI、零尺寸、过大尺寸 / 换算溢出不改变 Runtime 或 revision；验证 revert 可恢复最后已保存 placement。
- 旧 JSON 缺失 key 可读取；位置 setter 保存、清除、重新 parse 后结果一致；只修改位置时未知根字段及嵌套字段保持不变。
- 已知 placement key 的类型错误、字段缺失、非法范围会使 parse / Store 进入安全错误路径，原字节不改；默认首开不会添加 placement。

### App 保存通道（已添加；统一 workspace gate 通过）

- 同一 latest-wins 测试先启动一个保存，再同时更新窗口位置与外观 / 天气；最终重启读取到最新两项，不丢任一项。
- 保存后确认 unknown fields、backup、baseline / stale 外部修改冲突保护行为不变；损坏配置和冲突重试不得覆盖原字节。
- 位置值与其他 settings 共用同一 `SettingsSnapshot` / revision / worker；确认没有创建独立文件或第二条写盘路径。

### 恢复几何、导航和 Windows 消息

- 已添加纯几何测试，覆盖：主屏有效位置、左侧副屏负坐标、异 DPI 以目标当前 DPI 换算、离屏位置夹回、原显示器移除后退回 owner 屏、无保存值时避让 Isle、极小工作区适配、极端有符号坐标面积不溢出、水平滚动可达最右控件。
- 短窗口导航布局已加入三组纯几何测试，覆盖正常六行、270/320 DIP 紧凑六行及 220 DIP 两列三行，并验证矩形有界、非零且不相交。
- 已添加 manual change gating 测试，覆盖不变值不提交和重复最终值不重复提交。窗口消息实际事件序列、初始化 / 关闭时序及退出补取仍需由 A0 software gate 与 Windows QA 验证。
- 真实多屏、显示器拔插、混合 DPI 和视觉检查仍由独立 QA 在可用 Windows 硬件完成。自动化几何测试不能替代这些证据；不得据此宣称截图或硬件验收已通过。

## 当前软件验证记录

A0 在 D060 当前候选（2026-10-03）完成统一门禁：`cargo fmt --check`、workspace `cargo check`、`cargo clippy --workspace --all-targets -- -D warnings` 均通过；workspace 测试 112 项通过（core 41、app 33、ui 38），另有 1 项联网测试按配置忽略；release build 通过，耗时 37.42 秒。短窗口共享导航布局和对应三项几何测试已包含在此候选中。先前 D86 的 108 项测试记录为历史验证，不代表当前候选。

本记录包含 D060 的 A6 placement 软件回归：7 个隔离场景通过、进程均正常退出且未强杀；手动保存 / 重开 / 重启、原配置保真、父关窗 active-move flush 和默认不写回均通过。实际 workarea min-track 按当前真实工作区约束：模拟 200% 请求 1366×690 实际得到 1466×1032；另用 synthetic 384 DPI（400%）取得 244 DIP 客户高以覆盖 2×3 紧凑导航，六页切换及字体通过。恢复模拟 200% 后横滚标签与控件同位移，实际 Tab / End 操作通过。证据为 `artifacts/ui-v2-placement/results.json`；前四次脚本前提失败均保留历史。

真实 200% DPI short-window、截图、混合 DPI、多屏拔插与屏幕阅读器验收仍待完成。A7 已按 D060 同 SHA 闭合 A3 / A4 软件增量审查，计数 0 / 0 / 0 / 0；不能据软件门禁宣称 placement 实机最终验收通过。

## 文件范围与依赖

本工作包建议修改的文件限于：

- `crates/isle-core/src/settings.rs`
- `crates/isle-core/src/configuration.rs`
- `crates/isle-app/src/configuration.rs`
- `crates/isle-app/src/settings/window.rs`
- `crates/isle-app/src/settings/model.rs`（短窗口导航共享纯几何布局）
- `crates/isle-app/src/settings/render.rs`（同步裁切并绘制可横向滚动的内容区）
- `crates/isle-app/src/settings/mod.rs`
- `crates/isle-app/src/weather_settings.rs`（若兼容 facade 转发需要）

`main.rs` 的入口参数和更新事件整合归 A0；位置实现未改 scripts、media 或主 UI。没有引入新依赖，也没有创建单独 sidecar 配置。A0 统一 workspace fmt/check/clippy/tests/release；真实截图、多屏和硬件 DPI 仍待独立 QA。
