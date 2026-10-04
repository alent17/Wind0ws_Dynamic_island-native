# Wind0ws Dynamic Island Native — 多 Agent UI 优先协作方案

> 项目：`alent17/Wind0ws_Dynamic_island-native`
> 当前主分支：`main`
> 当前阶段目标：**优先完成原生 UI 视觉、布局与动画收口，再继续扩展功能。**

---

## 1. 当前阶段原则

> **验收状态（2026-10-04）：UI 第一阶段已完成，代码验收版本 0.61。** 本文中的旧版本规划 0.61–0.65 与实际提交版本号不一一对应；相关实现已分别包含在 0.57–0.60。Playing Next、网易云 UI、Widget Shelf 和 Settings polish 属于 UI 第一阶段之后的路线图，不作为本阶段 DoD。

当前项目已经具备：

- Rust + Win32 原生窗口
- Direct2D / DirectWrite
- DXGI / DirectComposition
- UI 2.0 单窗口结构
- Single Surface Morph
- Shared Album Art
- Dynamic Glass
- Live Activities
- 基础播放器控制
- DPI / 副屏 / UIA 测试基础

因此这一轮**不继续大规模扩功能**，重点是把 UI 做到真正可长期使用的状态。

本阶段暂缓：

- 网易云新功能
- 新播放器适配
- 新 Widget
- 新 Live Activity 类型
- Playing Next Provider
- 新 Shader 实验
- 设置页新功能

允许：

- UI 修复
- 动画修复
- 视觉精修
- Glass 精修
- 必要的稳定性 Bug 修复

---

# 2. Agent 组织方式

采用：

**1 个 Lead Agent + 最多 3 个子 Agent**

| Agent | 职责 | 主要修改区域 | 优先级 |
|---|---|---|---|
| Lead Agent | 拆任务、Review、测试、版本、合并 | 全仓库 | P0 |
| UI Layout Agent | 尺寸、布局、字体、图标、播放器视觉 | `crates/isle-ui/`、部分 `render.rs` | P0 |
| Motion Agent | Morph、Spring、Hover、展开收起 | `visual_state.rs`、`motion.rs`、`spring.rs` | P1 |
| Glass Agent | 黑色主体、边缘玻璃、折射、高光 | `render.rs` Glass 区域 | P1 |

---

# 3. 关键协作规则

## 3.1 禁止多个 Agent 同时大面积修改 `render.rs`

`render.rs` 是当前冲突风险最高的文件。

必须分阶段：

1. UI Layout Agent 先完成静态视觉
2. Lead Review 并合并
3. Motion Agent 基于最新 `main`
4. Glass Agent 最后处理材质

---

## 3.2 修改顺序

严格按照：

```text
Static UI
↓
Motion
↓
Glass
↓
Live Activities
↓
Playing Next
```

不能跳过静态 UI 直接继续加动画和特效。

---

## 3.3 子 Agent 不直接合并 main

子 Agent：

- 建分支
- 修改
- 自测
- 提交

由 Lead Agent：

- Review
- 跑完整 CI
- 检查截图
- 合并 `main`

---

# 4. 第一阶段目标：Music UI

第一阶段只完成：

```text
Compact
↓
Expanded Music
↓
Compact
```

先把这一条路径做到足够稳定。

暂时不要求 Playing Next。

---

# 5. UI Layout Agent

## 目标

重做音乐灵动岛的视觉比例，使其接近参考图的感觉：

- 黑色主体
- 紧凑
- 圆润
- 控件克制
- Album Art 清晰
- 信息层级明确
- 不像普通桌面播放器面板

---

## 5.1 建议尺寸

初始建议值：

| 状态 | 建议尺寸 |
|---|---:|
| Compact | `156 × 36 DIP` |
| Hover | `176 × 40 DIP` |
| Expanded Music | `430 × 164 DIP` |
| Music + Toolbar | `430 × 202 DIP` |

允许 ±10 DIP 调整，以实际截图为准。

---

## 5.2 Expanded Music 布局

### Album Art

```text
72 × 72 DIP
radius ≈ 16~18 DIP
```

左边距：

```text
22~26 DIP
```

---

### Title

```text
15~16 DIP
Medium / SemiBold
```

要求：

- 单行优先
- 超长标题使用裁切 / marquee
- 不破坏右侧布局

---

### Artist

```text
12~13 DIP
opacity ≈ 0.60~0.68
```

---

### Progress

```text
height: 4 DIP
track opacity: 0.16~0.20
```

要求：

- 与左右时间标签对齐
- 保持横向平衡
- Seek Hit 区可以比视觉条更高

---

### Controls

```text
Previous    Play/Pause    Next
```

要求：

- Play 略大
- Previous / Next 对称
- 不做成三个明显的大按钮
- 不使用厚重按钮背景
- Press 状态只做轻微缩放/亮度变化

---

## 5.3 Favorite

收藏按钮：

- 不抢视觉中心
- 不挤压 Play
- 推荐放在控制区左侧
- 未收藏为低对比灰白
- 收藏后可用红色，但不要大面积发光

---

## 5.4 Spectrum

频谱：

- 只作为辅助信息
- 不压标题
- 不比 Controls 更抢眼
- 默认宽度小
- 音乐暂停后降低运动量

---

## 5.5 UI Layout 验收

- [x] Album Art 是主要视觉锚点
- [x] 标题与歌手基线整齐
- [x] Progress 居中
- [x] 时间标签对齐
- [x] Playback Controls 对称
- [x] Favorite 不破坏中心结构
- [x] Spectrum 不抢标题
- [x] 96 DPI 正常
- [x] 144 DPI 正常（合成 DPI）
- [x] 192 DPI 正常（合成 DPI）
- [x] 长标题正常
- [x] 无封面正常
- [x] 无媒体正常
- [x] 所有内容不越出 Surface
- [x] attached / floating 两种模式正常
- [x] 四边 Edge 模式不破布局

---

# 6. Motion Agent

Motion Agent 必须等待静态 UI 基本定稿。

---

## 6.1 核心原则

动画必须表现为：

> **同一个 Surface 连续 Morph。**

禁止：

```text
Compact UI disappear
↓
Expanded UI appear
```

应该：

```text
Compact Surface
↓
Width / Height / Radius 连续变化
↓
Album 连续移动和放大
↓
内容渐入
↓
Expanded Surface
```

---

## 6.2 Album Art Morph

必须复用同一个 Album Art Visual。

```text
Compact Album
↓
移动
↓
放大
↓
Expanded Album
```

禁止展开时创建第二张 Album Art。

切歌 Crossfade 属于例外。

---

## 6.3 Surface Morph

只允许以下参数连续变化：

```text
width
height
radius
shoulder
```

HWND Host 保持稳定。

禁止每帧改变窗口物理尺寸来模拟动画。

---

## 6.4 Text 动画

Title / Artist：

```text
opacity: 0 → 1
offset: 4~7 DIP → 0
```

禁止明显飞入。

---

## 6.5 Progress 动画

推荐：

```text
Opacity
+
Rect Width Expansion
```

不要突然出现完整进度条。

---

## 6.6 Controls 动画

建议顺序：

```text
Play
↓
Previous / Next
↓
Secondary Controls
```

轻微 stagger：

```text
20~35 ms
```

不需要明显延迟。

---

## 6.7 动画时间建议

| 动画 | 建议 |
|---|---:|
| Hover | 120–160 ms |
| Compact → Expanded | 260–340 ms |
| Expanded → Compact | 220–300 ms |
| Content Fade | 120–180 ms |
| Press | 70–100 ms |

允许非常轻微 overshoot。

禁止明显 Bounce。

---

## 6.8 Motion 验收

- [x] 展开无突然切换
- [x] 收起连续
- [x] Album 不跳位置
- [x] Surface 不跳位置
- [x] 快速点击可以反向
- [x] Hover 中点击不会重置动画
- [x] 动画中途反向无闪烁
- [x] 切歌 Crossfade 不破坏 Morph
- [x] Reduced Motion 正常
- [x] FPS / CPU 开销无明显退化（P95 诊断采样通过，静止两秒帧数不增长）

---

# 7. Glass Agent

Glass Agent 最后开始。

目标不是增加特效，而是**减少视觉噪声**。

---

## 7.1 材质结构

建议：

```text
Base Black
↓
Very Subtle Artwork Tint
↓
Bottom Glass Transition
↓
Edge Refraction
↓
Inner Highlight
↓
Content
```

---

## 7.2 黑色核心

建议保持：

```text
0% → 72%
```

区域接近纯黑。

推荐：

```text
RGB: 0~5
Alpha: 0.97~1.0
```

---

## 7.3 Transition

玻璃过渡：

```text
72% → 92%
```

才逐渐进入透明。

---

## 7.4 Transparent Tail

只保留最底部：

```text
92% → 100%
```

不要让整个播放器像半透明面板。

---

## 7.5 Refraction

现有折射带可以进一步收窄。

建议：

```text
3~6 DIP
```

原则：

- 只存在边缘
- 不影响文字
- 不扭曲 Album
- 不形成明显波浪

---

## 7.6 RGB Dispersion

建议最大偏移：

```text
0.10~0.18 px
```

第一版甚至可以暂时关闭。

参考 UI 的高级感不依赖强 RGB 色散。

---

## 7.7 Album Blur

建议：

```text
4~7%
```

只用来增加底部色彩层次。

禁止让整个播放器被封面染色。

---

## 7.8 Glass 验收

- [x] 中心明显是黑
- [x] 大面积区域不是透明玻璃
- [x] 白色桌面不灰蒙
- [x] 黑桌面还能看清轮廓
- [x] 亮封面不污染整块 UI
- [x] 暗封面有层次
- [x] RGB 彩边不明显
- [x] Refraction 不影响文字
- [x] 动画过程中材质不跳变
- [x] Capture 失败时回退自然（代码路径检查：捕获失败时禁用折射采样，保留玻璃渐变）

---

# 8. 分支结构

建议：

```text
main

agent/ui-layout
agent/ui-motion
agent/ui-glass
```

---

# 9. 合并顺序

必须：

```text
agent/ui-layout
↓
main

agent/ui-motion
↓
rebase latest main
↓
main

agent/ui-glass
↓
rebase latest main
↓
main
```

禁止三个 Agent 同时完成后一次性解决大量冲突。

---

# 10. Commit 规范

推荐：

```text
0.58 refine music surface proportions
0.59 align album metadata and timeline
0.60 refine transport controls
0.61 refine compact island layout
0.62 smooth shared album morph
0.63 support reversible player morph
0.64 simplify dynamic glass edge
0.65 tune refraction and inner highlight
```

禁止：

```text
update ui
fix stuff
improve design
misc changes
```

---

# 11. 版本规划

| Version | 内容 |
|---|---|
| 0.58 | UI 尺寸和比例 |
| 0.59 | Music 元素布局 |
| 0.60 | Playback Controls |
| 0.61 | Compact UI |
| 0.62 | Morph 动画 |
| 0.63 | 动画打断 / 反向 |
| 0.64 | Glass 简化 |
| 0.65 | Refraction 精修 |
| 0.66 | Live Activities 视觉 |
| 0.67 | Playing Next |
| 0.68 | UI 第一阶段 RC |

---

# 12. Round 执行方案

## Round 1 — Static UI

### UI Layout Agent

负责：

- Surface 尺寸
- Album
- Title
- Artist
- Progress
- 时间
- Controls
- Favorite
- Spectrum

### Motion Agent

只 Review：

- 检查未来动画是否容易实现
- 不写代码

### Glass Agent

只分析：

- 标记未来 Glass 区域
- 不写代码

完成后 Lead 合并。

---

# 13. Round 2 — Motion + Glass

UI 定稿后：

### Motion Agent

主要修改：

```text
crates/isle-ui/src/visual_state.rs
crates/isle-ui/src/motion.rs
crates/isle-ui/src/spring.rs
```

必要时少量修改 `model.rs`。

---

### Glass Agent

主要修改：

```text
crates/isle-app/src/render.rs
```

只允许修改：

- Dynamic Glass constants
- Glass drawing
- Refraction
- Inner highlight
- Blur / tint

禁止顺手修改 Player Layout。

---

# 14. Round 3 — QA

三个 Agent 共同进行 Review，但不随意重构。

### UI Agent

检查：

- Layout
- Typography
- Alignment
- Spacing

### Motion Agent

检查：

- Morph
- Interrupt
- Reverse
- Hover
- Reduced Motion

### Glass Agent

检查：

- Bright Desktop
- Dark Desktop
- Bright Artwork
- Dark Artwork
- DPI

Lead 负责最终合并。

---

# 15. 每个 Agent 提交前必须执行

```powershell
cargo fmt --all -- --check
cargo check --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --release --bin isle-native --locked
```

---

# 16. DPI 测试要求

每个 UI 阶段必须覆盖：

```text
96 DPI
144 DPI
192 DPI
```

至少检查：

- Compact
- Expanded Music
- 展开过程
- 收起过程
- 长标题
- 无封面
- 暂停状态

---

# 17. Lead Agent 总提示词

```text
你是 Wind0ws_Dynamic_island-native 项目的 Lead Agent。

当前阶段优先完成 UI，不开发新的播放器功能、网易云功能、
Widget、Provider 或其他非必要功能。

最多使用 3 个子 Agent：

1. UI Layout Agent
负责原生灵动岛尺寸、布局、字体、Album Art、Progress、
Controls 和视觉比例。

2. Motion Agent
负责 Single Surface Morph、VisualState、Spring、
展开 / 收起和内容动画。

3. Glass Agent
负责 Direct2D / DirectComposition Dynamic Glass、
边缘折射、透明度、Inner Highlight 和桌面融合。

当前目标不是增加功能，而是让 UI 接近提供的参考图。

必须遵循：

- 保持 Rust + Win32 + Direct2D + DirectWrite +
  DXGI + DirectComposition 原生架构。
- 禁止 WebView / HTML / CSS。
- 不重构已经稳定的媒体、音频、天气等功能。
- UI 优先。
- 先完成静态视觉，再动画，最后 Glass。
- 同一时间禁止多个 Agent 大面积修改 render.rs。
- Agent 修改前必须阅读现有实现。
- 所有改动分小 commit。
- 子 Agent 不直接合并 main。
- Lead Review 后按 UI → Motion → Glass 顺序合并。
- 每个阶段执行 fmt / check / clippy / test / release build。
- 每个阶段覆盖 96 / 144 / 192 DPI。
- 动画必须是同一 Surface 和同一 Album Art 连续 Morph。
- 不增加视觉效果来掩盖布局问题。
- Dynamic Glass 必须保持黑色核心。
- Refraction 必须限制在极窄边缘。

第一阶段目标：

Compact
→ Expanded Music
→ Compact

只有这一条路径达到视觉和动画稳定后，
才能继续 Live Activities、Playing Next 和其他状态。
```

---

# 18. Definition of Done

UI 第一阶段只有同时满足以下条件才完成：

- [x] Compact 外观达到可长期使用水平
- [x] Expanded Music 比例接近参考图
- [x] Album / Metadata / Progress / Controls 布局稳定
- [x] Compact ↔ Expanded 连续 Morph
- [x] 动画可打断、可反向
- [x] Dynamic Glass 不喧宾夺主
- [x] 96 / 144 / 192 DPI 通过（144/192 为合成 DPI）
- [x] 长标题通过
- [x] 无封面通过
- [x] Light / Dark Desktop 通过
- [x] Bright / Dark Artwork 通过
- [x] Clippy 无 Warning
- [x] Tests 通过
- [x] Release Build 通过
- [x] Lead Review 通过
- [x] 合并 `main`（本次验收提交位于本地 `main`，现直接推送 `origin/main`）

验收证据：工作区 fmt/check/clippy/test/release build 全通过；160 个测试通过、1 个显式 opt-in 网络测试忽略。DISPLAY2 的玻璃对比度、透明度、开合动画、内容边界、艺术封面裁切、交互宽限和 attached/floating 放置回归通过。144/192 DPI 使用合成 DPI；此设备的物理屏幕为 96 DPI。截图与 JSON 诊断保存在仓库 `artifacts/`（Git 忽略）。

---

# 19. 后续阶段

UI 第一阶段结束后再继续：

```text
Live Activities
↓
Playing Next
↓
NetEase UI Integration
↓
Widget Shelf
↓
Settings polish
↓
Release QA
```

核心原则保持：

> **先把一个状态做漂亮，再增加下一个状态。**
