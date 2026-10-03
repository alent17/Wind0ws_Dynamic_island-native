# 原生外观与贴边位置

2026-09-27，原生设置窗口接入第一批 Studio 外观与位置选项。

## 已完成

- 可在“悬浮”和“贴边”样式间切换，选择上、右、下、左边缘，并设置沿边位置 0–100%。
- 应用后立即更新岛的形状、边缘方向和窗口位置；重启从原生配置恢复。
- 沿边位置按显示器工作区中可移动的距离计算，窗口保持在当前屏幕工作区内；显示器/DPI 定位仍使用原有窗口机制。
- 配置仅更新 `islandStyle`、`islandEdge` 和 `islandEdgePosition`，其余旧版与未知字段继续保留。默认配置为悬浮、顶部、50%。
- 外观控件使用标准 Win32 下拉列表；窗口可在左侧副屏显示，测试模式不抢焦点。

2026-09-27，继续接入岛体形状参数：收起长度、收起凹肩、展开凹肩和展开圆角。四个范围沿用 Studio：80–300、0–16、0–64、0–80 像素。原生滑杆拖动时数值即时更新；应用后复用岛体现有弹簧切换，重启后恢复。

## 验证

- `cargo test --workspace`：54 项通过，1 项既有联网用例忽略。
- `cargo clippy --workspace --all-targets -- -D warnings`、`cargo build --release`、`git diff --check` 通过。
- `scripts/native_settings.py` 在 `DISPLAY2` 上通过：样式和方向即时应用，右边缘 73% 的位置正确落点，四个滑杆值即时应用并在重启后恢复；主窗口和设置窗口均在左侧副屏工作区内。
- 已检查设置窗口与岛体截图，滑杆区域无重叠或裁切，贴边岛体落在预期边缘。截图保存在本机 `artifacts/native-appearance.png` 和 `artifacts/native-appearance-island.png`，不纳入提交。

结果：[副屏外观与形状设置回归](performance/native-settings/native-shape-settings.json)。

## 尚未覆盖

这是设置迁移的一部分，不是完整 Studio。还需迁移频谱样式、颜色、显示器选择、语言和字体等设置，并建立复用主岛渲染器的 Studio 实时预览、草稿撤销与分组布局。跨 DPI 设置窗滚动与多屏热插拔也需单独验收。
