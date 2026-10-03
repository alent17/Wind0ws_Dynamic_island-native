# Native UI 2.0 Settings Controls: D2D Visual Plan

状态：**manifest 前置软件通过，完整控件实现中，尚未验收**。D060 软件候选与动态 Windows QA、增量审查已收束；E6DD 前置已完成资源 / activation 与 legacy 兼容、目标 PMv2 / WinSxS ComCtl32 v6 检查。当前完整控件源码正在实施，未有新 SHA / 控件 QA；已有父窗口 Direct2D 外壳不代表完整子控件完成。本方案不把原生控件改色或仅绘制覆盖层当作统一 D2D 控件完成。

## Scope

为 Settings V2 的 Toggle、Slider、Segmented、Dropdown、Push Button 和导航按钮建立统一的 Direct2D 状态视觉，同时保留现有原生 HWND、交互路由、键盘语义和 Windows 自动化代理。天气查询与颜色输入继续使用原生 `EDIT`，不 subclass、不换输入实现，以保持中文 IME、候选窗、编辑选择和系统 Edit 控件的无障碍行为。

可行路线是把 Win32 原生控件继续作为行为和状态的所有者：普通 BUTTON 与 TRACKBAR 使用 Common Controls `NM_CUSTOMDRAW`；下拉框保留 `COMBOBOX` HWND 并用 `CBS_OWNERDRAWFIXED | CBS_HASSTRINGS` 绘制选项和所选项；一个由 Settings `Theme` 持有的共享 `ID2D1DCRenderTarget` 在 UI 线程串行服务绘制回调。不能把现有 `BS_OWNERDRAW` 按钮直接留在 NM_CUSTOMDRAW 路线上，因为 owner-draw button 不发送 `NM_CUSTOMDRAW`。Microsoft 文档也明确按钮 `NM_CUSTOMDRAW` 要求应用 manifest 绑定 Common Controls 6.0。[按钮 NM_CUSTOMDRAW](https://learn.microsoft.com/en-us/windows/win32/controls/nm-customdraw-button)、[trackbar NM_CUSTOMDRAW](https://learn.microsoft.com/en-us/windows/win32/controls/nm-customdraw-trackbar)

## Files

- `crates/isle-app/build.rs`：在保留当前图标和 VERSIONINFO 资源的前提下，将新的 manifest 作为 `RT_MANIFEST` 嵌入资源，并添加 `cargo:rerun-if-changed`。
- `crates/isle-app/isle.manifest`：声明 `Microsoft.Windows.Common-Controls` 6.0 activation dependency；保留现有 Per-Monitor-V2 DPI 配置和身份元数据。
- `crates/isle-app/src/settings/window.rs`：保持控件 HWND、ID、COMMAND / WM_HSCROLL 路由和行为状态；处理 `WM_NOTIFY/NM_CUSTOMDRAW`、ComboBox `WM_DRAWITEM/WM_MEASUREITEM`，将目前 owner-draw 的导航按钮切成标准 push button。
- `crates/isle-app/src/settings/render.rs`：实现共享 D2D DC painter 和角色/状态绘制；现有 `ShellRender` 的 Hwnd render target 继续负责 Settings 壳层，两种 target 分开持有。
- `crates/isle-app/src/settings/model.rs`：放纯状态映射、颜色/对比度检查和 D2D 几何计算，避免 Win32 消息过程持有第二份业务状态。
- `scripts/native_settings.py` 或新增 `scripts/native_settings_controls.py`：扩展隔离配置控件回归、键盘操作和多状态截图；新增 Settings 专用 UIA/MSAA 检查入口。`scripts/accessibility.ps1` 当前针对 Isle 浮窗 provider，不应当作 Settings 已覆盖的证据。
- `crates/isle-app/src/settings/legacy.rs`：优先只作为 manifest 全局影响的回归对象；除非门禁发现兼容缺陷，不纳入这一包的实现改动。

当前 `build.rs::embed_windows_identity` 只生成带 ICON 和 VERSIONINFO 的 `identity.rc`，没有 manifest 资源；Common Controls 6.0 manifest 是开启按钮 custom draw 的前置，不是可以省略的部署优化。现有 Per-Monitor-V2 行为由 `main.rs` 调用 `SetProcessDpiAwarenessContext` 实现；新 manifest 应显式声明等价的 Per-Monitor-V2 DPI awareness，避免引入资源后 DPI 声明不一致。官方资源形式为 `CREATEPROCESS_MANIFEST_RESOURCE_ID RT_MANIFEST "isle.manifest"`（RC 中也可用等价资源 ID / 类型）。[启用 Visual Styles 与 manifest](https://learn.microsoft.com/en-us/windows/win32/controls/cookbook-overview)

上述是方案建立时的基线；E6DD 前置已将等价 `1 24 "isle.manifest"` 资源嵌入，并保留原身份资源和字体复制。manifest 声明 PMv2 / Per-Monitor fallback、asInvoker 与 uiAccess=false。`scripts/manifest_controls.py` 只用于可信本地构建，在 inspector 进程解析并调用 ComCtl32 的版本函数；目标进程是否加载 v6 与实际窗口 DPI context 由 A6 另行只读检查，不能从 inspector 推断。

## API

在 `settings/render.rs` 由 `Theme` 独占一个按窗口生命周期创建/销毁的 `ControlPainter`：

```rust
enum ControlRole {
    PushButton,
    Toggle { checked: bool, mixed: bool },
    Segmented { selected: bool, first: bool, last: bool },
    Slider { position: i32, min: i32, max: i32, ticks: Vec<i32> },
    Dropdown { selected_text: String, selected: bool, hot: bool },
}

struct ControlVisualState {
    enabled: bool,
    hot: bool,
    pressed: bool,
    focused: bool,
    selected: bool,
    high_contrast: bool,
}

enum PaintResult { Painted, NativeFallback }

impl ControlPainter {
    fn paint(&mut self, dc: HDC, bounds: RECT,
             role: &ControlRole, state: &ControlVisualState) -> Result<PaintResult>;
    fn set_dpi(&mut self, dpi: u32);
}
```

这只是最小边界而非要求按字面照搬的 public API。Painter 只消费当前 native state snapshot，不修改 Settings model，也不持有长寿命 HDC。`window::procedure` 用 `hwndFrom/idFrom` 找 `ControlRole`：Toggle 从 `BM_GETCHECK`，Radio 从 `BM_GETCHECK`，Slider 从 `TBM_GETPOS/MIN/ MAX` 及 ticks，Enable/Focus 从 HWND 查询；hover/pressed 取 custom-draw item state，若系统未报告某状态则以 `WM_MOUSEMOVE/LEAVE`、focus 和按钮 capture 消息维护只用于绘制的瞬态位。系统状态消息始终驱动原生控件本身。

每个绘制回调都将该回调提供的 HDC 和物理像素子矩形绑定到同一个 `ID2D1DCRenderTarget`，并在该回调完成 `BeginDraw/EndDraw`；HDC、RECT、DPI 改变时不得复用上次绑定信息。绑定和绘制几何的原点/像素到 DIP 转换集中在 painter 内。Direct2D DC target 要求通过 `BindDC` 关联当前 DC，并在 DC 或绘制范围变化时重新绑定。[BindDC 合约](https://learn.microsoft.com/en-us/windows/win32/api/d2d1/nf-d2d1-id2d1dcrendertarget-binddc)

## Behavior

1. **先建立 manifest 前置。** 增加 ComCtl32 6.0 dependency，并验证最终 exe 含正确 `RT_MANIFEST`。它改变进程 activation context，Common Controls 版本影响不局限于 V2 Settings；先验证 legacy Settings、浮窗/计时器及现有原生子控件，再进入 D2D 实现。Common Controls 6 的视觉样式取决于 manifest，owner/custom-draw 区域不会自动得到系统替代画法。[Visual Styles 说明](https://learn.microsoft.com/en-us/windows/win32/controls/using-visual-styles)
2. **统一绘制器先行。** `ControlPainter` 在创建自绘样式之前预检可用。它管理 D2D target、DirectWrite 格式/共享字体资源、DPI 和 target-generation；target lost 后重建一次。按钮和 trackbar 绘制错误返回 `NativeFallback`，映射为 `CDRF_DODEFAULT`，保留系统绘制。再次失败后停止启用 D2D 绘制，不在回调里反复重建。
3. **控件逐类迁移但保留原生行为。**
   - Toggle 保留 `BUTTON + BS_AUTOCHECKBOX`，使用当前 BM_CHECK 状态绘 capsule、thumb、标签。
   - Segmented 保留 `BS_AUTORADIOBUTTON`、WS_GROUP、组内方向键和 Space 行为，以选中状态绘选项段。绘制时必须表现 hover、按下、焦点、禁用和选中，不能只改静态底色。
   - Slider 保留 `msctls_trackbar32`、TBM 范围/位置和 WM_HSCROLL 行为。用 `NM_CUSTOMDRAW` 的 trackbar 部件信息分别画 channel、thumb 和 ticks；若当前 OS/控件没有给出可完整绘制的部件通知，不能悄悄让一部分轨道继续以原生外观显示。
   - 普通按钮以及 6 个导航按钮用普通 `BUTTON` 样式接入 custom draw，保留命令 ID 和键盘默认行为。现有导航 `BS_OWNERDRAW` 需移除，因为该样式屏蔽按钮 `NM_CUSTOMDRAW` 通知。
   - Dropdown 保留 `COMBOBOX + CBS_DROPDOWNLIST`，增加 `CBS_OWNERDRAWFIXED | CBS_HASSTRINGS`；处理 `WM_MEASUREITEM`、弹出项及 `ODS_COMBOBOXEDIT` 所指的所选字段 `WM_DRAWITEM`，保留 CB_* 数据、CBN_SELCHANGE、selection、焦点和键盘路由。`CBS_HASSTRINGS` 让控件继续持有真实字符串并支持 MSAA 暴露列表项，但不免除 UIA selection/expand-collapse 实测。[ComboBox owner draw](https://learn.microsoft.com/en-us/windows/win32/controls/about-combo-boxes)、[owner-drawn ComboBox 的 MSAA 项](https://learn.microsoft.com/en-us/windows/win32/winauto/exposing-owner-drawn-combo-box-items)
4. **ComboBox 必须有明确故障退路。** owner-draw ComboBox 没有按钮/trackbar 那种可直接选择 `CDRF_DODEFAULT` 的逐项 fallback。预案是失败时投递窗口私有恢复消息，在 `WM_DRAWITEM` 返回后将这个真实 combo HWND 安全重建为普通 `CBS_DROPDOWNLIST`，恢复 items、selection、focus、rect、字体、ID、enabled 和 page 归属；恢复中用系统主题 API 绘制的单帧应急画面只用于防止空白，不计为 D2D 完成。若这条路径做不到无状态损失，则在 combo custom draw 启用前阻断该控件，不得继续发布“全控件统一 D2D”的结果。Combo owner-draw 让应用负责 list items，DropDownList 的选择字段也由 owner 绘制；边框/箭头未必完全属于 `WM_DRAWITEM` 区域，第一步需验证实际覆盖范围。若箭头/框无法一致，需扩展为保留 native 消息与自动化语义的窄 subclass 绘制边缘，不能盖一层无交互 UI 或把原控件隐藏。
5. **输入框隔离。** 城市搜索与颜色 `EDIT` HWND、输入处理和 `WM_IME_*` 路径保持原样。其边框/标签如需后续视觉调整，另立小范围方案；本工作包不能为了统一改造而影响拼音候选窗和编辑可达性。
6. **视觉状态与环境。** 覆盖 normal、hot、focused、pressed、checked/on、mixed（仅适用时）、selected/open、disabled。状态既用颜色也用形状/位置/描边表达，禁用/选中不能只靠色差。接收 `WM_DPICHANGED`，按当前窗口 DPI 更新 target、字体度量和 hit region；在同一进程两个显示器间移动要验证尺寸和文字栅格。监听 `WM_THEMECHANGED`、`WM_SYSCOLORCHANGE`、`WM_SETTINGCHANGE`，查询系统 high-contrast 设置；高对比模式优先使用系统色并保持焦点可见，小字号文本目标至少 4.5:1 对比度。不能获得可读系统调色或对比度不合格时回到可用原生主题控件。
7. **自动化语义来自真实控件。** 保持 HWND、class、control ID、label text、tab order 和可见 enabled state，确保系统代理仍暴露 Button/CheckBox/RadioButton/Slider/ComboBox 语义。owner draw 只接管像素，不应实现平行状态模型或靠视觉覆盖隐藏原生控件。官方 UIA ComboBox 要求含展开/收起及可选择列表的相应模式；检查 owner draw 后真实 Windows UIA 树和 Pattern，而不能从 `CBS_HASSTRINGS` 推断完整 UIA 已通过。[UIA ComboBox 契约](https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-supportcomboboxcontroltype)

## Tests

- **纯单测**：控件角色到 native state / visual state 的映射；Segmented 相邻段边缘与状态几何；Toggle checked/mixed/disabled；slider 范围端点、ticks、DPI scale；D2D 色盘明暗与 high-contrast palette；对比度计算及失败回退判定。
- **资源与 manifest 门禁**：检查 Release EXE 中有单一有效 `RT_MANIFEST`，assembly identity 为 `Microsoft.Windows.Common-Controls` version `6.0.0.0`；runtime/activation-context 诊断确认 Settings 子控件由 ComCtl32 v6 创建。资源合并须保留现有 ICON/VERSIONINFO 和 PMv2 设置。
- **消息与状态矩阵**：针对 Toggle、Slider、Segmented、Dropdown、Button/nav 全面采集 idle/hot/focus/pressed/disabled/checked/mixed/open/selected；确认 mouse click 不重复触发命令，slider 每个 WM_HSCROLL 路径仍只产生现有真实更新。
- **键盘**：Tab / Shift+Tab 穿过所有控件和两处 EDIT；Space 激活按钮/Toggle/Radio，方向键移动 Radio 组与 Slider 值，Home/End 按控件原生约定，Alt+Down/F4 展开 Dropdown、上下选择、Enter 确认、Esc 取消；关闭再打开后焦点不遗留悬空 HWND。
- **无障碍**：用 Windows SDK Inspect 或现有 UIA 客户端读取 Settings 原始树、Control View 和 Content View。对 Button 测 Invoke；Toggle 测 Toggle state；Segmented 检查 Radio role/selection；Slider 检查 RangeValue min/max/value 和 set；Dropdown 检查 ComboBox name、ExpandCollapse、Selection/SelectionItem、list item 名称与可见性。MSAA 同步检查角色、名称、状态、位置。真实 NVDA/讲述人操作及焦点朗读纳入 Windows QA；已有浮窗 UIA 脚本不能代替。
- **IME 与混合 DPI**：中文拼音在城市查询和颜色 EDIT 两处都完成候选、上屏、选择、删除；100/125/150/200% 单屏和混合 DPI 两屏移动，检查文本、D2D 边缘、子控件边界、focus ring、命中框一致。
- **配色/对比**：浅色、深色、高对比设置/主题切换；校验小字号对比度，禁用/选中/焦点能不靠颜色识别；`WM_THEMECHANGED` 与 `WM_SYSCOLORCHANGE` 后刷新而不重启。
- **故障注入**：target create、BindDC、BeginDraw/EndDraw、D2DERR_RECREATE_TARGET 失败；按钮和 slider 回到 native paint；owner-draw combo 完成其一次性原生 combo 重建且 selection/focus/items 不丢。测试连续多次失败没有死循环、空白、崩溃、重复 HWND map 或命令。
- **回归/截图**：现有 `native_settings.py` 的隔离 JSON、持久化和页面交互全部通过；legacy Settings、悬浮窗、计时器、天气 EDIT 与播放器原生按钮回归。逐页提供 100%/200% 和高对比截图，至少包含正常、hover、focus、pressed、disabled、selected 状态；截图应与自动化 HWND/UIA 快照同批生成。

## Risks

- **Manifest 是进程级前置**：ComCtl32 v6 可影响 legacy 与所有 common controls 的默认外观/细节；V2 Settings 之外的窗口必须先回归。不要假设只对新增 Settings 按钮局部生效。
- **消息 DC 生命周期短**：NM_CUSTOMDRAW/WM_DRAWITEM 的 HDC/RECT 来自每次回调；错误缓存、DPI 变换或 clip/origin 换算会导致错位、闪烁和资源重入。D2D DC target 只能在 UI 线程按当前 callback 信息串行使用。
- **Native semantics 不等于自动化全绿**：owner draw 本身通常仍保留系统输入交互，但绘制和 MSAA/UIA 暴露各有细节。`CBS_HASSTRINGS` 说明 MSAA 可暴露项，不证明具体 OS 上 UIA Selection/ExpandCollapse patterns 已满足。
- **Combo owner draw 的回退复杂度最高**：标准按钮/trackbar可要求系统继续绘制；owner-draw ComboBox 一旦 renderer 失败还须避免列表与选择字段白屏并安全恢复原生样式。箭头和边框可能仍由系统主题绘制，视觉不统一则不能通过门禁。
- **深色/高对比互相冲突**：程序自定义深色调色在高对比用户设置下可能压掉系统强调色、焦点环或禁用提示。系统色处理与随时切回原生绘制必须在第一包中实现和测试。
- **新增屏幕绘制面积带来性能风险**：每个 child hwnd 可能独立触发 custom draw；若频繁创建资源或重建字体，滚动/拖动 Slider 时可能出现卡顿。Painter 应复用 target 资源，更新只限定失效矩形，并用现有 profiler/消息计数观察实际滚动和拖动。
- **DPI 与高对比矩阵不能只靠纯几何**：D2D 光栅化、系统主题和 UIA 矩形必须在实际 Windows 运行时看；软件单测不能宣称视觉或屏幕阅读器验收通过。

## Needs

当前 checkout 已有 `Win32_Graphics_Direct2D`、DirectWrite、GDI 与 `Win32_UI_Controls` API 使用；不需要新增 crate。实现前需要：

- Windows SDK 的 `rc.exe`（仓库现有 build.rs 已依赖它）以合并 manifest 资源；验证工具能读取 EXE 的 RT_MANIFEST/activation context。
- Windows 10/11 测试环境至少一台可切换 high-contrast 和文本缩放的机器；有 100% 与 200% DPI 两台显示器更好。
- UIA 检查器（Windows SDK Inspect 或已存在的测试环境工具）、NVDA 或 Windows Narrator；这用于验证现存系统代理，不要求另造屏幕阅读器。
- 允许使用临时隔离配置与开发版窗口脚本；不操作安装版配置，不在没有窗口 QA 时把 D2D 截图叫作正式验收。

## Next

按独立可回滚顺序实施：

1. **Manifest spike / gate**：添加资源、校验 activation context；运行 legacy Settings、计时器、浮窗、native controls 回归。不通过就撤回 manifest，暂停后续 custom draw。
2. **Painter 基座**：在 Settings `Theme` 里创建单线程共享 `ControlPainter`，验证回调 HDC + RECT 绑定、DPI、剪裁、资源丢失重建和按钮/trackbar native fallback；先给一个 Toggle 和一个 Slider 做状态矩阵。
3. **完整 native child 迁移**：把全部 Toggle、Segmented、Push Button、导航和 Slider 接入同一 renderer/state/style，再通过页面范围 HWND、键盘和回归用例；不能停在样板单控件交付。
4. **Dropdown 收口**：实现 owner-drawn items/selection field，验证真实箭头/边框范围、CBS_HASSTRINGS 的 MSAA、UIA selection/expand-collapse、combo 故障重建。如果系统边缘造成混搭，先完成保留原生消息语义的细薄绘制适配；如果验证不可靠，就保留原生 Dropdown 并将“全控件统一 D2D”标记为未完成。
5. **发布门禁**：完成混合 DPI / high contrast / IME / keyboard / UIA/MSAA / failure injection / legacy regression 和逐页状态截图，跑 workspace fmt/check/clippy/tests/release，然后交给独立动态 QA 与只读审查。任何动态 QA、实机屏幕阅读器证据待补时只记录“软件通过”，不作正式 UI 验收结论。

最小完整交付包是步骤 1–5 的同一候选：manifest 前置、完整 Toggle/Slider/Segmented/Dropdown/Button D2D 绘制、保留真实 native HWND/EDIT IME 和键盘/UIA 语义、故障退回原生控件、可重复状态截图及回归。将 combo 或任一状态类别留作“以后补”的部分交付不满足本 P0 目标。

## 当前实施约定（未验收源码）

A4 独占 `settings/window.rs`、`render.rs`、`model.rs`、新 `controls.rs` 及 facade；A0 在 `main.rs` 整合构造与诊断；A6 独占脚本和串行窗口 QA；A7 只读审查。E6DD 前置 EXE 冻结，不被控件 WIP 构建覆盖。下一控件候选须重新记录 SHA 后统一编译 / QA / 审查。

V2 facade 新入口 `new_v2_with_placement_and_control_fixture(..., allow_test_faults)`；旧入口保留并关闭测试开关。`control_diagnostics()` 返回 owned 快照，`--log` 报告 `settingsControlPaint` 各 role 成功绘制数、target 重建 / native fallback / combo recovery、high-contrast、generation 与 renderer enabled。计数仅在成功 EndDraw 且实际取代原生绘制后增加，不把“收到回调”计为 D2D 成功。

显式 `--ui-v2 --test-fixture --log` 才启用主窗测试消息 `0x8056`，由 App gate 后调用 Settings 的 direct `test_control_fault(kind)`；Settings 构造开关再拒绝非 fixture。种类 1 / 2 / 3 / 4 分别注入下次 BindDC、EndDraw、RECREATE_TARGET 和持续 renderer off；5 / 6 模拟 high-contrast / 恢复当前系统模式。它不更改系统 high-contrast、DPI 或真实服务；模拟结果不能替代硬件主题 / 视觉验收。生产、legacy、无 log、未打开 Settings 的实例不启用这些故障。QA 需包含拒绝路径、绘制失败回退和 deferred combo 重建后的状态保留。

2026-10-03 上传增量：完整 closed-combo 的成功绘制另计 `dropdownChrome`，与弹出项 / 字段的 `dropdown` 分开，防止仅列表成功被误记为箭头和边框已绘制。薄 subclass 在 native 消息处理后绑定当前完整客户区，使用 `COMBOBOXINFO` 的客户区坐标绘制箭头；失败只调度一次延后原生重建，取执行时的选择、焦点、可见状态并保留原 DIP 布局。连续 target loss 最多重建一次，随后停用直到成功绘制重置策略；纯策略与配色测试通过。跨同步 Win32 调用使用 owned 快照和短借用；系统高对比在 painter 创建失败时仍独立查询。

本增量上传是可审阅的开发代码，不关闭上述步骤 1–5 整体交付门禁。短生命周期 / 绘制 smoke 的结果见进度与 QA；完整故障矩阵、键盘 / UIA / IME、真实 DPI / 高对比与逐页截图仍待执行。`ISLE_TEST_CONTROL_FAIL_CREATE=1` 仅在显式 fixture 构造允许测试时生效。
