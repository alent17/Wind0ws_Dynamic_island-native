# Isle Native UI 2.0 — 多 Agent 协作开发总控方案

> 仓库：`https://github.com/alent17/Wind0ws_Dynamic_island`  
> 技术栈：Rust + Win32 + Direct2D + DirectWrite + DXGI + DirectComposition + windows-rs  
> 子 Agent 模型：**6.0 Luna / Extra High（极高）**  
> 如果实际运行平台没有这个精确型号，则替换为平台可用的最高 reasoning 配置。

## 一、组织结构

| Agent | 角色 | 主要职责 |
|---|---|---|
| A0 | 项目经理 / Tech Lead | 拆任务、接口决策、合并顺序、风险控制、最终整合 |
| A1 | 架构 Agent | UiState、LayoutSnapshot、Widget、Activity、MediaCapabilities |
| A2 | Windows Renderer Agent | Win32、D2D、DWrite、DComp、Glass、Region、DPI |
| A3 | Motion Agent | Spring、AnimatedRect、Shared Album、Morph、Reflow、反向动画 |
| A4 | Settings Agent | Settings 2.0、实时生效、自动保存、revision、冲突保护 |
| A5 | Integration Agent | GSMTC、WASAPI、Artwork、Audio、Queue capability、Activity 数据源 |
| A6 | QA / Performance Agent | Build、回归、DPI、多屏、性能、动画压力测试 |
| A7 | Final Reviewer | 最终代码审查、架构一致性、性能与风险验收 |

## 二、全局规则

所有 Agent 必须先读取真实仓库，不允许把当前项目误判成 Tauri/WebView 项目。不得重写已经稳定工作的 Windows Media、WASAPI、配置备份、DPI、多显示器能力，除非有明确技术理由。

所有主要 UI 动画必须基于 Spring，可中断、可反向，不允许用大量固定延迟或 sleep 拼动画。Renderer 只画当前状态，Layout 只算目标位置，Motion 负责从当前值走向目标值，业务状态和绘制逻辑必须分离。

项目始终保持 integration branch 可编译、可运行、可回退。任何跨 Agent 的接口变更必须由 A0 统一。

## 三、A0 项目经理职责

A0 首先读取：

- `README.md`
- `Cargo.toml`
- `crates/isle-app`
- `crates/isle-core`
- `crates/isle-ui`

然后输出：

1. 当前代码架构摘要
2. 已有能力
3. 技术债与风险
4. 文件 ownership
5. 任务依赖图
6. Sprint 计划
7. 每个 Agent 的工作包

A0 尽量不直接大规模写业务代码。A0 主要负责统一接口、阻止重复实现、处理冲突、决定合并顺序以及最终 integration build。

每轮结束输出：

```text
Sprint Status

已完成：
进行中：
阻塞：
待审查：
下一轮：

Build:
Tests:
Performance:
Known Issues:
```

## 四、A1 架构 Agent

目标是将当前以多个 bool/Page 为中心的 UI 状态逐步升级为清晰状态系统。

建议建立：

```rust
pub enum PrimarySurfaceMode {
    Compact,
    Expanded(ExpandedView),
    Hidden,
}

pub enum ExpandedView {
    Music,
    PlayingNext,
    Shelf,
    Timer,
    Volume,
    Clock,
    Weather,
}

pub struct InteractionState {
    pub hovered: bool,
    pub pressed: bool,
    pub dragging: bool,
    pub inspection_lock: bool,
}

pub struct UiState {
    pub primary: PrimarySurfaceMode,
    pub interaction: InteractionState,
    pub activities: Vec<LiveActivity>,
}
```

同时建立：

```text
State
↓
LayoutSnapshot
↓
Motion / Spring
↓
Renderer
```

禁止让 `Model::retarget()` 无限膨胀，也禁止 Renderer 自己决定目标位置。

## 五、A2 Windows Renderer Agent

负责：

- Win32 Window
- Direct2D
- DirectWrite
- DirectComposition
- Glass
- Surface
- Clip
- Window Region
- DPI

建议逐步形成：

```text
render/
  surface.rs
  music.rs
  glass.rs
  activities.rs
  widgets.rs
```

Glass 视觉要求：

```text
Compact：
接近纯黑

Dynamic Glass：
顶部黑色核心
↓
黑色逐渐融化
↓
透明玻璃
↓
边缘折射

Liquid Glass：
主体透明
边缘折射
极弱色散
圆角 Lens
```

禁止做成蓝紫发光卡片。蓝色只能来自背景/壁纸，不属于玻璃本体。

## 六、A3 Motion / Spring Agent

负责：

- Motion Profiles
- AnimatedRect
- Shared Album
- Surface Morph
- Activity Pill
- Widget Reflow
- Hover / Leave Grace
- Reduced Motion
- 动画反向

Motion Profiles：

```text
Surface：稍重，轻微超调
Content：更快，超调更小
Micro：快速，几乎无 bounce
```

Shared Album 必须是真正同一个视觉元素。Compact / Hover / Expanded / Playing Next 中只改：

```text
x
y
width
height
radius
opacity
```

不能“小图淡出，大图淡入”。

动画测试必须覆盖：

```text
0%
20%
40%
60%
80%
100%
```

以及在 20%、50%、80% 时反向。

## 七、A4 Settings Agent

Settings 2.0 目标：

- 删除 Preview Renderer
- 删除 Preview Model
- 删除假收起/悬停/展开预览
- 删除 Apply 按钮
- 真实 Isle 即预览
- 设置改动立即作用于真实 App

设置状态必须拆成：

```text
Runtime
Pending
Persisted
```

维护：

```rust
runtime_revision
saving_revision
persisted_revision
```

Slider 拖动时立即更新 Runtime；松手或 debounce 后进入 Pending 并后台保存。保存过程中继续修改不能丢失。

必须保留现有配置系统的：

- 未知字段保真
- stale file 检测
- backup
- corrupt protection
- 外部修改冲突保护

## 八、A5 Integration Agent

优先复用已有：

- GSMTC
- WASAPI
- System Audio
- Artwork
- Weather

新增 `MediaCapabilities`：

```rust
pub struct MediaCapabilities {
    pub previous: bool,
    pub play_pause: bool,
    pub next: bool,
    pub seek: bool,
    pub shuffle: bool,
    pub repeat: bool,
    pub queue: bool,
}
```

UI 只能显示真实支持的功能。没有 queue 就不显示 Playing Next。

Artwork 2.0：

```text
Thumbnail 128
Display 512
Blur Source
```

Compact 先用 Thumbnail。展开立即开始，不等待高清封面。Display 就绪后在同一 Album Rect 中 crossfade。

## 九、A6 QA / Performance Agent

从第一个 PR 开始介入。

每轮必须跑：

```text
cargo fmt
cargo check
cargo test
release build
```

回归场景：

```text
No Media
Playing
Paused
Missing Artwork
Long Title
Long Artist
Unknown Duration
Disabled Controls
Timer Active
Timer Finished
```

动画压力：

```text
展开 30 次
收起 30 次
快速 Hover / Leave
20% 反向
50% 反向
80% 反向
切歌时展开
展开时切歌
```

DPI：

```text
100
125
150
175
200
```

多屏：

```text
单屏
双屏
不同 DPI
主屏切换
睡眠恢复
```

性能至少记录：

```text
renderCpuP95Ms
regionCpuP95Ms
artworkUploadMs
blurBuildMs
```

Idle 时不能持续 60FPS 刷新、持续 Present 或持续创建 Region。

## 十、A7 Final Reviewer

A7 不写新功能，只做最终审查。

等级：

```text
BLOCKER
HIGH
MEDIUM
LOW
```

只有：

```text
BLOCKER = 0
HIGH = 0
```

A0 才允许进入正式版本。

## 十一、Git 协作

分支：

```text
agent/a1-architecture
agent/a2-renderer
agent/a3-motion
agent/a4-settings
agent/a5-integrations
agent/a6-qa
integration/ui2
```

禁止子 Agent 直接改 `main`。

建议提交格式：

```text
feat(ui): add layout snapshot
feat(motion): add animated rect
refactor(settings): separate runtime and persisted state
perf(render): cache glass resources
fix(media): preserve artwork during transition
test(motion): cover interrupted spring transitions
```

文件 ownership：

| 范围 | Owner |
|---|---|
| `isle-core` 新状态模型 | A1 |
| `isle-ui/layout` | A1 |
| `isle-ui/motion` | A3 |
| `render/*` | A2 |
| `settings/*` | A4 |
| media/audio/artwork | A5 |
| tests/benchmark/profiling | A6 |
| `main.rs` 跨模块整合 | A0 |

## 十二、Sprint 顺序

### Sprint 1 — 基础架构

A1：

```text
UiState
LayoutSnapshot
MediaCapabilities
Activity 基础结构
```

A3：

```text
AnimatedRect
Motion Profiles
Spring tests
```

A4：

```text
Runtime / Pending / Persisted
SettingsPatch
Revision save
```

A5：

```text
Artwork 128 / 512
MediaCapabilities 接入
```

A6：

```text
Regression baseline
Performance baseline
```

A2 等 A1/A3 接口稳定后再开始 Renderer 分层。

### Sprint 2 — Compact / Hover / Expanded

实现：

```text
Compact
Hover
Expanded Music
Shared Album
Surface Morph
```

### Sprint 3 — Glass / Playing Next

实现：

```text
Dynamic Glass
Liquid Glass
Playing Next
```

### Sprint 4 — Live Activities

第一批只做：

```text
Volume
Timer
Media transient
```

### Sprint 5 — Widget Shelf

第一版只做：

```text
Music
Volume
Timer
Clock
Weather
System Stats
```

先做规则布局：

```text
Solo
Dual
Compact
```

最后再做拖拽编辑。

## 十三、Agent 交付格式

每个 Agent 完成后必须返回：

```markdown
## Result

### Scope
本次负责什么。

### Files changed
修改哪些文件。

### API changes
新增/修改哪些接口。

### Behavior
用户能看到什么变化。

### Tests
执行了哪些测试。

### Risks
风险是什么。

### Needs from other agents
依赖其他 Agent 什么。

### Suggested next step
下一步建议。
```

禁止只回复“完成了”。

## 十四、冲突处理

两个 Agent 出现设计冲突时：

```text
Agent A 提方案
Agent B 提方案
↓
A0 比较
↓
A0 给统一接口
↓
两边按接口调整
```

原则：

**接口先统一，代码后实现。**

## 十五、自动停止 / 回退条件

出现以下任一情况立即停止合并：

- release build 失败
- 媒体功能回归
- 设置文件有损坏风险
- DPI 错位
- Window Region 阻挡桌面点击
- Idle CPU 明显上涨
- 动画不能反向
- Settings 保存丢修改
- Crash

A0 回退最后一个 PR，不允许在坏的 integration branch 上继续堆功能。

# A0 总控 Prompt

```text
你是 Isle Native UI 2.0 项目的项目经理与 Tech Lead。

仓库：
https://github.com/alent17/Wind0ws_Dynamic_island

当前项目已经从 Tauri 转为 Windows 原生 Rust UI。
技术栈：
Rust + Win32 + Direct2D + DirectWrite + DXGI + DirectComposition + windows-rs。

你负责协调多个 6.0 Luna Extra High 子 Agent 完成 UI 2.0。

如果系统没有该精确型号，则使用平台最高可用 reasoning 配置，不得使用低 reasoning 模式完成核心编码。

核心产品目标：

1. UI 美观、精致、现代。
2. 所有主要动画使用可打断、可反向 Spring。
3. Compact → Hover → Expanded 是同一个 Surface 的连续 Morph。
4. Album Cover 是真正 Shared Element。
5. Dynamic Glass：顶部黑色核心向下融成透明折射玻璃。
6. Liquid Glass：无 Notch 时 Island 自己变成完整播放器。
7. Playing Next 从当前播放器横向生长。
8. Live Activities 使用紧凑黑色 Pill / Circle。
9. Settings 2.0 删除假 Preview。
10. 修改 Settings 时真实 App 立即变化。
11. Settings 使用 Runtime / Pending / Persisted revision 自动保存。
12. 不破坏现有 Windows Media、WASAPI、Artwork、DPI、多屏、配置保护能力。
13. Idle 时不能持续 60FPS 刷新。
14. 每轮必须通过 build、test、performance、regression。

你拥有：
A1 Architecture
A2 Windows Renderer
A3 Motion & Spring
A4 Settings
A5 Integrations
A6 QA & Performance
A7 Final Review

第一步：
1. 获取 main 最新状态。
2. 阅读 native 目录。
3. 输出当前架构摘要。
4. 输出风险。
5. 输出 ownership。
6. 输出 Sprint 1。
7. 给每个 Agent 创建互不冲突、可验证的任务。
8. 明确并行任务与依赖任务。
9. 每轮结束进行 integration build。
10. 只有 QA 通过才进入下一 Sprint。

不要一次全部重写。
始终保持 integration branch 可编译、可运行、可回退。
```

# 子 Agent 通用 Prompt

```text
你是 Isle Native UI 2.0 项目的专业子 Agent。

模型：
6.0 Luna / Extra High。

开始前：
1. 阅读项目经理任务。
2. 获取当前 integration branch。
3. 阅读要修改文件的真实代码。
4. 阅读相关调用方。
5. 不猜 API。
6. 不重复创建已有系统。

编码要求：
- 小步改动。
- 保持可编译。
- 不引入无用 dependency。
- 不破坏 DPI / Reduced Motion / Accessibility。
- 不直接修改其他 Agent ownership 文件。
- 跨模块需要新接口时先报告 A0。
- Spring 必须支持 retarget。
- 不使用 sleep 模拟动画。
- 不使用大量 fixed delay animation sequence。
- Renderer 不负责业务状态。
- Settings 不承担持久化线程逻辑。
- Integration 不负责 UI layout。

完成后执行：
cargo fmt
cargo check
相关 cargo test

然后按标准 Result 格式返回。
```

# 最终完成定义

只有以下全部满足才算完成：

```text
Build ✓
Tests ✓
UI consistency ✓
Animation interrupt/reverse ✓
Settings live apply ✓
Settings persist safety ✓
Media regression ✓
DPI ✓
Multi-monitor ✓
Idle performance ✓
Accessibility basic regression ✓
Final Reviewer BLOCKER = 0
Final Reviewer HIGH = 0
```
