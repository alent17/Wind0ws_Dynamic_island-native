# Isle 原生 UI 2.0 最终改造方案

> 项目：`alent17/Wind0ws_Dynamic_island`  
> 基线：当前 `main` 原生 UI，commit `41626bc3ecf2342ea72e72aef152fc65feafe0b5`  
> 技术栈：Rust + Win32 + Direct2D + DirectWrite + DXGI + DirectComposition + windows-rs  
> 本文目标：在不推翻现有原生实现的前提下，完成一套可长期扩展的灵动岛 UI、Spring 动画、完整播放器、Live Activities、模块化 Shelf 与全新设置系统。

---

## 0. 最终结论

这一轮不应该继续在当前页面和设置窗口上“加功能”。

真正需要做的是一次 **UI 架构升级**：

1. 将现在以 `expanded + hovered + Page` 为中心的页面模型，升级为“主 Surface 状态 + 交互状态 + Live Activity”的组合状态。
2. 将布局计算、Spring 动画和绘制彻底分离。
3. 将专辑封面升级为真正的 Shared Element，同一个视觉元素完成收起、悬停、展开和完整播放器之间的运动。
4. 将当前独立 GDI 悬浮播放器与灵动岛展开播放器区分开；主体验改为 Island 本身 Morph 成 Full Player。
5. 将设置窗口彻底重写，删除假预览，真实运行中的 Isle 就是预览。
6. 设置改动实时作用于 App，但持久化保存必须做合并、节流、版本控制，不能直接把每一个滑块事件都写到 JSON。
7. 保留现有配置文件的未知字段保真、备份、冲突检测、损坏保护能力。
8. 为 Live Activities 和 Widget Shelf 预留底层模型，但不要在第一阶段同时把几十个功能都做进去。
9. 所有动画以 Spring 为核心，要求可打断、可反向、无多次弹跳、无瞬移。
10. 视觉目标不是“复刻 iPhone”，而是做一套属于 Windows 的动态系统 Surface。

---

# 1. 当前项目基础与可利用资产

当前原生版已经具备不少正确基础，不应重写：

- `isle-ui/src/spring.rs`：已有刷新率无关 Spring，并验证 60 / 120 Hz 轨迹一致。
- `isle-ui/src/model.rs`：宽、高、圆角、凹肩已经由 Spring 驱动。
- `isle-app/src/render.rs`：主界面已经使用 Direct2D / DirectComposition 路线。
- `media.rs`：Windows Media Session 已接入真实媒体。
- `system_audio.rs`：系统音量和输出设备已接入。
- `spectrum.rs`：WASAPI 真实频谱已接入。
- `artwork.rs`：封面获取、网络补全、解码和缓存已经存在。
- `configuration.rs`：配置保存已经具备后台线程、原文件保护、备份、外部修改冲突检测。
- UIA / MSAA、DPI、多显示器、减少动画、资源生命周期等基础也已经存在。

因此这一轮的原则是：

**复用底层能力，重构 UI 状态、布局、动画与设置交互。**

---

# 2. 之前方案里不够周到的地方

下面这些问题如果不提前处理，后面很容易再次返工。

## 2.1 “实时设置”不能直接套用现有保存逻辑

现在 `configuration::Service`：

- 使用容量为 1 的 `sync_channel`
- `busy == true` 时新的 `save()` 会直接返回 `false`
- 一次保存完成前，不接受新的保存任务

这个设计很适合现在“点击应用按钮”的模式，但不适合实时设置。

如果滑块在 300 ms 内产生几十次变化，直接复用当前逻辑会导致：

- 大量变化被丢弃
- Runtime 已经变成新值，但 `configuration.appearance` 仍然是旧值
- UI 上显示“已保存”但磁盘里不一定是最后状态
- 快速关闭程序时最后几次修改可能没有落盘

因此必须新增 **Runtime Settings + Coalescing Persistence**，详见第 12 节。

---

## 2.2 当前设置窗口的 Preview 会造成双状态源

现在 `weather_settings.rs` 内有：

- 独立 `Preview Model`
- 独立 `Preview Renderer`
- 收起 / 悬停 / 展开 / 隐藏四个预览状态
- 设置控件变化后调用 `redraw_preview()`

这意味着 App 和 Preview 是两套状态。

以后加入：

- Album Shared Element
- Live Activity
- Widget
- Glass
- 动态颜色
- 实时媒体能力

两套状态会越来越难保持完全一致。

最终方案：**删除 Preview，真实 App 即 Preview。**

---

## 2.3 当前 `expanded: bool` 会越来越难扩展

现在主要依赖：

- `expanded`
- `hovered`
- `current Page`
- `pending_page`

当加入：

- Expanded Music
- Shelf
- Playing Next
- Live Activities
- 设置编辑锁定
- 隐藏
- 临时 HUD
- Widget 编辑模式

继续增加 bool 会出现大量非法组合。

最终应该将状态拆为三个正交维度：

### Primary Surface

- Compact
- Expanded(view)
- Hidden

### Interaction

- Hovered
- Pressed
- Dragging
- Editing / Inspection Lock

### Activities

独立的 Activity 列表，不与 Primary Surface 互斥。

这样音乐展开时旁边仍然可以同时存在倒计时或音量 Activity。

---

## 2.4 当前 `Model::retarget()` 同时承担了布局和动画目标

现在 `retarget()` 直接根据 Page 计算：

- Width
- Height
- Radius
- Shoulder

以后 Widget 和 Full Player 加进来后，如果继续把所有尺寸都写进 `Model::retarget()`，会快速失控。

最终要拆成：

1. **Layout Engine**：只算目标位置和尺寸。
2. **Motion Engine**：负责从当前值 Spring 到目标值。
3. **Renderer**：只负责画当前动画值。

这是整个重构中最重要的结构调整之一。

---

## 2.5 独立悬浮播放器目前使用另一套 GDI UI

当前 `floating_player.rs` 是独立 Win32 / GDI 绘制路径。

如果主灵动岛升级成新的 Direct2D Glass + Shared Album，而独立播放器继续用旧 GDI：

- 两套视觉会不一致
- 两套媒体布局会重复维护
- 两套控制逻辑会分叉
- 后续 Glass、动态颜色、Playing Next 都要做两遍

因此：

**Full Player 与 Detached Player 必须明确区分。**

V2 主体验为：

> Island 自身 Morph 成 Full Player。

旧 `floating_player.rs` 第一阶段保留作为兼容功能，不继续扩展。等新播放器稳定后，再决定：

- 重写为共享 Direct2D Renderer
- 或降级为可选“独立播放器”
- 或逐步弃用旧外观

不要现在同时重写两套播放器。

---

## 2.6 当前封面 128×128 不足以承担大播放器

当前文档中封面输出为 128×128 BGRA。

对于小灵动岛足够，但 Full Player 放大后可能明显发糊。

最终封面管线应改成多级：

- Thumbnail：128×128，优先快速得到
- Display：512×512，展开时异步加载
- Glass Source：可使用 96～192 px 的降采样版本做模糊

展开时先继续使用 Thumbnail，不阻塞 UI；Display 就绪后在同一个 Album Rect 内做短 Crossfade。

这样可以保证：

- 展开立刻响应
- 不等待网络或解码
- 大图足够清晰
- Glass 不浪费 GPU

---

## 2.7 Hover 在形变过程中容易抖动

灵动岛扩大/缩小时，如果点击区域和 Window Region 同时变化，鼠标可能反复产生 Leave / Enter。

结果会变成：

Hover → 放大 → Mouse Leave → 缩小 → Mouse Enter → 再放大。

必须加入：

- Hover 进入的最小延迟或 Intent 判断
- Leave Grace Period
- 动画过程中稳定的 Interaction Envelope
- Press / Drag 时禁止自动收起

建议初值：

- Hover Enter：0～50 ms
- Hover Leave Grace：120～180 ms

这些不是为了制造延迟，而是防止边界抖动。

---

## 2.8 Live Activity 会突破当前 `HOST = 480`

未来主岛左右存在 Activity Pill 时，480 的固定 Host 空间不一定够。

不建议每一个动画帧都改变顶层窗口大小，因为 Win32 / DWM 窗口 resize 会引入额外抖动和成本。

最终应改为：

- 每个显示器上使用稳定的透明 Host Envelope
- 主 Island、Full Player、Activity Pill 都在这个 Envelope 中运动
- 只有跨边方向、DPI、显示器改变时才重新布置顶层 Host
- Surface 本身的变化只改变内部 Direct2D / DComp 几何

Host 的最终尺寸通过真实屏幕测试决定，不在此文档写死。

---

## 2.9 `SetWindowRgn` 每帧变化可能成为新瓶颈

目前 Island 形状变化时会重新创建 Region。

现有实现已经会跳过相同像素多边形，这是好的。

但是当未来：

- Surface 更大
- 两侧出现多个 Pill
- 60 / 120 Hz 动画
- 区域变成多个几何联合

Region 更新成本可能增加。

最终方案不提前删除 `SetWindowRgn`，因为它目前能可靠提供透明区域点击穿透。

但必须增加性能测试：

1. 保持当前准确 Region 路线作为基线。
2. 测量动画时 Region 更新 CPU。
3. 如果成为瓶颈，再考虑：
   - Region 低频更新，视觉仍 60/120 Hz
   - 动画期间使用 current + target 的交互包络
   - settle 后恢复精确 Region

不能为了动画好看让透明区域吃掉桌面点击。

---

# 3. 最终设计原则

## 3.1 One Surface, Many States

Isle 是一个 Surface，不是播放器。

同一个 Surface 可以承载：

- Music
- Volume
- Timer
- Clock
- Weather
- System Status
- Live Activities
- Widget Shelf

播放器只是 Surface 的一种布局。

---

## 3.2 Layout 不负责动画

Layout 只输出“应该在哪里”。

Motion 决定“怎么过去”。

Renderer 只画“现在在哪里”。

禁止 Renderer 自己决定动画逻辑。

---

## 3.3 所有动画必须可中断

例如展开只进行到 37%，用户立即收起：

- 保留当前 position
- 保留当前 velocity
- 立刻修改 target
- Spring 从当前状态自然反向

禁止等待旧动画完成。

---

## 3.4 尽量不使用硬编码延迟链

不要：

- 先等 80ms 再显示标题
- 再等 100ms 出现按钮
- 再等 120ms 显示进度条

这种写法遇到动画反向非常难处理。

推荐做法：

所有元素同时获得新 Target，但使用不同 Spring Profile：

- Surface 较重
- Album 跟随较快
- Content 稍慢
- Micro Controls 最快

自然形成层次，而不是排队播放。

---

# 4. 新 UI 状态模型

建议建立：

```rust
pub enum PrimarySurfaceMode {
    Compact,
    Expanded(ExpandedView),
    Hidden,
}

pub enum ExpandedView {
    Music,
    Timer,
    Volume,
    Clock,
    Weather,
    Shelf,
    PlayingNext,
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

说明：

- Hover 不应该成为与 Expanded 互斥的大状态。
- Live Activities 不应该塞进 Primary Surface enum，因为它们可以和音乐同时存在。
- Settings 的“保持展开”只影响真实 Surface 的 `inspection_lock`，不是假 Preview。

---

# 5. Layout Engine

新增一个真正的 Layout Snapshot：

```rust
pub struct LayoutSnapshot {
    pub surface: Rect,
    pub radius: f32,
    pub shoulder: f32,

    pub album: Option<ElementLayout>,
    pub title: Option<ElementLayout>,
    pub artist: Option<ElementLayout>,
    pub progress: Option<ElementLayout>,
    pub controls: Vec<ElementLayout>,
    pub toolbar: Vec<ElementLayout>,

    pub activities: Vec<ActivityLayout>,
    pub hit_regions: Vec<HitRegion>,
}
```

Layout 输入：

- UiState
- Settings
- Media capabilities
- 当前 Edge
- Attached / Floating
- DPI-independent logical metrics
- Widget 配置

Layout 输出全是 **DIP**，不要混入物理像素。

DPI 转换继续留在窗口和 Renderer 边界。

---

# 6. Motion Engine

## 6.1 保留现有 Spring 的优势

现有 `spring.rs` 已经：

- 支持中断
- 保留 velocity
- dt 无关
- 60 / 120 Hz 行为一致
- reduced motion 可以立即落到 target

这些是非常好的基础。

不要为了“更物理”轻易全部替换。

---

## 6.2 增加 Motion Profile

建议在现有 Spring 之上增加命名 Profile，而不是让每个组件自己填任意参数。

### Surface

用于：

- Width
- Height
- Radius
- Shoulder
- Full Player 外形

感觉：

- 稍重
- 轻微超调
- 无二次明显弹跳
- 约 320～440 ms 进入稳定视觉区间

### Content

用于：

- Album
- Title
- Artist
- Progress
- Widget

感觉：

- 比 Surface 快一点
- 约 220～340 ms
- 超调更小

### Micro

用于：

- Hover
- Button Press
- Toggle
- Slider Thumb
- 小图标

感觉：

- 快
- 约 120～220 ms
- 基本不允许肉眼明显 bounce

这些时间只是手感目标，不是固定 duration。

---

## 6.3 AnimatedRect

新增统一动画矩形：

```rust
pub struct AnimatedRect {
    pub x: Spring,
    pub y: Spring,
    pub w: Spring,
    pub h: Spring,
}
```

以及：

```rust
pub struct VisualElement {
    pub rect: AnimatedRect,
    pub opacity: Spring,
    pub radius: Spring,
    pub scale: Spring,
}
```

Album、Activity Pill、Widget 都使用同一套。

---

# 7. Album Shared Element

这是 V2 的核心 P0。

同一首歌在 Compact 和 Full Player 中只存在一个 Album Visual。

## Compact

- 小尺寸
- 位于 Island 一侧
- 可能带旋转 Disc 效果
- Crop 固定

## Hover

- 只轻微放大或移动
- 不替换纹理
- 不重新创建 Bitmap

## Expanded

- 同一个 Rect 继续移动和放大
- Radius 连续变化
- 进入 Full Player 的 Hero 位置

## Collapse

完整反向。

---

## 7.1 封面裁切必须稳定

封面来源未必严格方形。

不能 Compact 使用一种 Crop，Expanded 又重新中心裁切，否则动画中会看起来像图像内容突然跳动。

每首歌应生成一个稳定的 `ArtworkUvRect`。

所有状态共用这个 UV。

---

## 7.2 切歌时不要移动 Rect

切歌只换纹理，不换 Album Visual 本身。

推荐：

- Rect 保持当前运动状态
- 旧纹理轻微弱化
- 新纹理 Crossfade
- 新专辑主色缓慢进入 Glass
- 标题和歌手重新启动内容动画

这样展开过程中切歌也不会破坏 Shared Element。

---

# 8. Full Player / Liquid Player

主播放器不再是“打开另一个窗口”。

默认行为：

**Island 本身变形成 Full Player。**

完整播放器内容根据媒体能力动态出现：

- Album
- Title
- Artist
- Progress
- Previous
- Play / Pause
- Next

Shuffle、Repeat、Seek、Playing Next 等只在 Provider 明确支持时显示。

不能为了和参考图一样，画出无法工作的按钮。

---

## 8.1 Media Capabilities

建议扩展媒体快照：

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

UI 完全 capability-driven。

如果当前 GSMTC 或具体 Provider 不支持，就不显示相应控件。

---

# 9. Glass Engine

第一版不要依赖系统 Acrylic 才能工作。

## 9.1 默认：Album Glass

流程：

1. 从当前 Album 得到降采样纹理。
2. 预先生成 Blur 结果。
3. 叠加深色 Tint。
4. 叠加低透明度的 Album Dominant Color。
5. 增加极弱顶部 Highlight。
6. 增加极弱 Border。
7. 前景文字保持高对比。

动画过程中只变：

- Opacity
- Transform
- Clip
- Tint interpolation

不要每帧重新做 Gaussian Blur。

---

## 9.2 动态颜色必须做安全限制

专辑主色不能原样铺背景。

必须限制：

- Saturation
- Luminance
- Contrast

目标永远是：

**专辑决定气氛，文字决定可读性。**

如果专辑颜色很亮，自动增加 Dark Tint。

---

## 9.3 Windows Acrylic 作为增强项

Windows 11 上可以后续测试系统 Backdrop。

但当前项目使用：

- 透明合成
- DirectComposition
- 异形 Region

必须先做兼容验证。

如果 Acrylic 与某些 Window / Region 组合表现不稳定：

自动回退 Album Glass。

Album Glass 永远是可靠默认路径。

---

# 10. Settings 2.0

## 10.1 删除所有假预览

删除：

- `Preview`
- `preview_mode`
- Preview Renderer
- Preview Model
- “收起 / 悬停 / 展开 / 隐藏”预览按钮
- Preview Stage
- `redraw_preview()`

真实 Isle 就是结果。

---

## 10.2 设置窗口不要继续以 `weather_settings.rs` 承担所有职责

建议拆为：

```text
isle-app/src/settings/
    mod.rs
    model.rs
    window.rs
    render.rs
    input.rs
    controls.rs
    sections/
        general.rs
        appearance.rs
        modules.rs
        media.rs
        weather.rs
        advanced.rs
```

不要求一次性拆完，但最终目标要明确。

---

## 10.3 设置 UI 使用自绘 Direct2D

为了达到“美观”目标，最终不要继续依赖传统 Win32 默认：

- Checkbox
- Button
- Trackbar
- ComboBox

推荐：

- Settings Window 仍然是原生 HWND
- Surface、Card、Toggle、Slider、Segmented Control、Dropdown 使用 D2D / DirectWrite 自绘
- 城市搜索、需要 IME 的文本输入可临时覆盖一个原生 EDIT 控件
- Color Hex 输入也可以复用原生 EDIT
- 其余控件统一视觉

这样既保留 Windows 原生输入和 IME，又不会出现传统 Win32 风格。

---

# 11. Settings 视觉规范

## 11.1 Window

建议默认：

- Width：约 820 px
- Height：约 760 px
- 最小尺寸：约 720 × 640
- 支持 DPI
- 支持用户移动
- 保存上次窗口位置
- 如果保存位置已经离开所有工作区，则自动恢复到当前显示器

---

## 11.2 Layout

左侧导航约 170～190 px。

右侧内容使用 Card。

导航：

- 常规
- 外观
- 模块
- 媒体
- 天气
- 高级

不放大面积 Preview。

---

## 11.3 Design Tokens

第一版以暗色为主：

- Window：`#0C0F13`
- Surface：`#12161C`
- Card：`#171C23`
- Hover：`#1D242D`
- Stroke：White 7%～10%
- Primary Text：`#F5F7FA`
- Secondary Text：约 62%～72% 白
- Disabled：约 34%～42% 白

间距系统：

- 4
- 8
- 12
- 16
- 24
- 32

圆角：

- 小控件：8～10
- Card：14～18
- 大面板：20～24

不要每张 Card 都画明显边框。

主要依赖：

- 明暗层级
- 空间
- 极弱 Stroke
- Hover 反馈

---

# 12. Runtime Settings 与自动保存

这是设置重构最重要的技术部分。

## 12.1 三份状态

必须区分：

### Runtime

App 当前正在使用的设置。

### Persisted

最后一次成功写入磁盘的设置。

### Pending

等待保存的最新修改。

不能继续只依赖 `configuration.appearance` 作为唯一真相。

---

## 12.2 SettingsPatch

所有控件发送 Typed Patch：

```rust
pub enum SettingsPatch {
    Appearance(AppearancePatch),
    Controls(ControlsPatch),
    Modules(ModulesPatch),
    Media(MediaPatch),
    Weather(WeatherPatch),
}
```

收到 Patch 后：

1. 验证
2. 立即 apply 到 Runtime
3. `model.retarget()`
4. 必要时重新 `position()`
5. 标记 dirty revision
6. 合并进入 Pending Save

---

## 12.3 Revision

建议维护：

```rust
runtime_revision
saving_revision
persisted_revision
```

保存过程中用户继续修改：

不要拒绝。

只更新“最新 Pending”。

旧 revision 保存完后：

如果 `persisted_revision < runtime_revision`，立刻继续保存最新状态。

这样永远不会丢最后一次设置。

---

## 12.4 Debounce

### Toggle / Combo

运行时立即生效。

保存可在 100～200 ms 内合并。

### Slider

拖动时每帧可实时影响 Runtime。

磁盘保存：

- `TB_ENDTRACK` 后提交
- 键盘调整或鼠标停止后约 300～400 ms 提交

### Text / Color

只有值合法才 Runtime Apply。

停止输入约 300 ms 后保存。

---

## 12.5 Save 失败

如果磁盘保存失败：

- Runtime 不突然回滚
- 顶部状态显示“保存失败”
- 提供“重试”
- 提供“恢复已保存设置”
- 不偷偷覆盖外部修改

现有的：

- stale file 检测
- exclusive lock
- previous backup
- legacy backup
- corrupt file protection
- 未知字段保真

全部必须保留。

---

## 12.6 退出程序

程序退出时：

- 如果有 dirty revision，尝试 flush 最新 Pending
- 设定有限等待时间
- 不能无限阻塞退出
- 如果失败，日志明确记录

---

# 13. “真实 App 即预览”的可用性补充

删除 Preview 后还存在一个问题：

用户调整“展开圆角”时，如果真实 Isle 此刻是收起的，就看不到变化。

不应该重新做假 Preview。

推荐在“外观”页提供一个小功能：

**保持当前展开状态**

它控制的是真实 App，不是设置窗口中的副本。

特点：

- 非持久化设置
- 离开外观页或关闭设置后恢复进入前状态
- 用户也可以关闭这个行为

同时设置窗口打开位置要主动避开 Isle 的当前 Screen Rect。

这样用户调整参数时能真正看到 App。

---

# 14. 设置窗口定位

当前设置窗口居中容易盖住 Isle。

新逻辑：

1. 找到 Isle 所在 Monitor。
2. 获取真实 Isle 最大可视 Envelope。
3. 尝试将 Settings 放到工作区中不与其重叠的位置。
4. 优先右侧或左侧。
5. 空间不足时允许用户移动，不强制追随。

用户把 Isle 从顶部移动到左/右时，Settings 不应在每一次滑块变化时跟着跳。

只在首次打开或显示器变化时做避让。

---

# 15. Hover / Pointer 行为

## 15.1 Hover

Compact Hover 只做：

- Surface 轻微放大
- Album 轻微放大
- 信息稍微增加
- 高光稍微增强

不要 Hover 就打开完整播放器。

---

## 15.2 Click

Click 才进入 Expanded。

---

## 15.3 Leave Grace

鼠标离开后不要立刻 collapse。

给予约 120～180 ms 的 grace。

如果期间鼠标重新进入，取消收回。

---

## 15.4 Press / Drag

用户：

- 拖 Progress
- 拖 Widget
- 调 Slider
- 拖动窗口

期间禁止自动 Collapse。

---

# 16. Live Activities

第一版只做已有数据源：

- Volume
- Timer
- Media-related transient state

不要一开始做下载、VPN、蓝牙、OBS 全部功能。

---

## 16.1 Activity Model

```rust
pub struct LiveActivity {
    pub id: ActivityId,
    pub kind: ActivityKind,
    pub title: String,
    pub value: String,
    pub progress: Option<f32>,
    pub priority: u8,
    pub expires_at: Option<f64>,
}
```

必须支持：

- 同 ID 更新
- 去重
- TTL
- Priority
- Dismiss
- Complete

---

## 16.2 Slot

默认同时最多展示：

- Main Island
- 左侧 1 个重要 Pill
- 右侧 1 个重要 Pill

超出进入 Activity Queue。

不要因为多个 Activity 让整个顶部无限横向增长。

---

## 16.3 Activity Spring

Activity 出现：

- 小 scale
- 宽度增长
- opacity 进入
- 内容跟随

Activity 结束：

- value 弱化
- 宽度收回
- opacity 退出

不使用简单 Fade In / Fade Out。

---

# 17. Widget Shelf

Widget 第一版只做 built-in，不做第三方插件 SDK。

建议：

- Music
- Volume
- Timer
- Clock
- Weather
- System Stats

用户配置保存：

- Widget ID
- Order
- Size / Span
- Enabled

未知 Widget ID：

- 保留配置
- 当前版本忽略
- 不删除用户数据

为未来 Plugin 做兼容。

---

## 17.1 Layout

### 单 Music

占据完整 Surface，成为 Full Player。

### 双 Widget

两块主要 Card。

### 3～4 Widget

Compact Layout。

第一版不要自由拖动任意像素位置。

先做规则化布局，才能保证不同 DPI 和边方向稳定。

---

# 18. Playing Next

Playing Next 必须是 Provider Capability。

Windows 系统媒体会话未必能提供完整播放队列。

因此：

- 有 Provider 队列：显示
- 没有：整个入口不出现
- 不伪造

未来 Spotify、特定播放器插件或自有 Provider 可以补充。

---

# 19. Artwork 2.0

建议改为多级资源：

```rust
enum ArtworkTier {
    Thumbnail128,
    Display512,
}
```

策略：

Compact：

只需要 Thumbnail。

Expanded：

先显示 Thumbnail，后台请求 Display。

Display Ready：

在原 Rect 内 Crossfade。

Glass：

使用 Thumbnail 或更低分辨率生成 Blur Cache。

---

## 19.1 Cache

建议控制：

- Thumbnail：最近 8 首
- Display：最近 3～4 首
- GPU：当前 + 过渡中的旧封面
- Blur：当前歌曲 1 份

设备丢失后可从 CPU 像素重新创建 GPU Bitmap。

---

# 20. Full Player 与旧 Detached Player

第一阶段：

- 新 Full Player：主 Island 内部 Morph
- 旧 Detached Player：继续存在但标记 Legacy Path
- 不继续给旧 GDI Player 增加复杂功能

第二阶段：

如果仍保留独立播放器：

让它复用：

- Layout tokens
- Album pipeline
- Media capabilities
- Direct2D Renderer
- Glass Engine

最终不能长期维护两套不同设计。

---

# 21. 四边布局

当前项目支持：

- Top
- Right
- Bottom
- Left

V2 不允许只把顶部做漂亮，其他三个方向退化。

建议定义统一的 Edge Layout Mapper：

Layout 在逻辑空间输出语义位置。

Edge Mapper 处理：

- 入口方向
- Shoulder Inset
- Surface Anchor
- Activity Pill 布局

文字始终保持正常阅读方向，不跟窗口旋转 90°。

---

# 22. DPI 与多显示器

所有动画值保持 DIP。

当 DPI 改变：

- 不重置 Spring state
- 重新计算 Layout Target
- 物理像素由 scale 转换
- Window Region 重建
- 缓存纹理按需要重建

测试：

- 100%
- 125%
- 150%
- 175%
- 200%

并覆盖不同 DPI 显示器之间切换。

---

# 23. Accessibility

美观不能以牺牲现有 UIA / MSAA 为代价。

新 Settings 自绘后必须重新提供：

- Name
- Role
- Focus
- Invoke
- Toggle
- Slider Value
- Bounds
- Keyboard navigation

文字 Edit 优先复用原生 EDIT，保证：

- IME
- 中文输入
- Clipboard
- Caret
- Selection

Widget 编辑未来还需要：

- 键盘排序
- “向左移动 / 向右移动”
- 不只依赖拖拽

---

# 24. Reduced Motion

继续尊重 Windows 系统动画设置。

Reduced Motion：

- Spring 直接或极快收敛
- 禁止 overshoot
- Glass 不做大范围动态变化
- Album 仍然保持正确 Shared Element 位置，但位移动画大幅缩短
- Activity 不做明显生长弹性

用户主动关闭动画时与系统 Reduced Motion 使用同一 Motion Policy。

---

# 25. Frame Scheduler

第一版目标：

**稳定 60 Hz 比不稳定 120 Hz 更重要。**

现有 Spring 已经验证 120 Hz 数值一致，可以为未来做准备。

V2 初期：

- 保持当前按需帧机制
- 无动画立即停止帧调度
- 静止不持续 Present
- 只在动画 / 频谱 / 时间进度需要时工作

后续经过真实性能数据验证后，再增加：

- 高刷显示器 120 Hz 动画模式
- 只在动画活跃时启用
- 低功耗或电池状态保持 60 Hz

不要一开始直接追求 144 / 240 Hz。

---

# 26. Glass 性能

禁止：

每帧从 CPU 重新 Blur。

应该：

封面变化时构建 Blur Texture / Effect Cache。

动画帧只更新：

- Transform
- Opacity
- Clip
- Tint

Blur Texture 不随窗口每一个像素变化重新生成。

---

# 27. Window Region 与 Input Region

视觉 Region 和 Input Region 应概念分离。

### Visual

Direct2D / DComp 决定。

### Input

Win32 Region / Hit Test 决定。

动画中如果视觉每帧变一点，不一定必须每帧构建复杂多 Region。

但任何优化必须满足：

- 透明区域不阻挡其他程序
- 可见区域可以正常点击
- 动画完成后 Region 完全准确

Region 优化必须以基准测试决定，不凭感觉修改。

---

# 28. 视觉设计系统

## 28.1 原则

- 大量留白
- 低对比边界
- 高对比内容
- 少量动态专辑色
- 不使用大面积发光
- 不使用多层重阴影
- 不使用夸张 Bounce
- 不使用“科技蓝”填满所有控件

---

## 28.2 Album 是主色来源

音乐模式：

Album Color 可以影响：

- Glass Tint
- Progress
- 小型高光

不能影响：

- 正文可读性
- 设置窗口所有页面
- 系统性的 Primary Text

设置页保持中性设计，避免用户切歌后整个 Settings 疯狂变色。

---

# 29. 设置页具体内容

## 常规

- 开机启动
- 灵动岛始终置顶
- 独立播放器始终置顶
- 系统减少动画
- 语言
- 显示器

## 外观

- 悬浮 / 贴边
- 方向
- 沿边位置
- Compact Length
- Collapsed Shoulder
- Expanded Shoulder
- Expanded Radius
- Glass Mode
- Album Color
- Custom Fill
- 保持展开查看

## 模块

- Music
- Volume
- Timer
- Clock
- Weather
- System Stats
- 排序（后续）

## 媒体

- Player Selection
- Player Priority
- Spectrum
- Spectrum Mode
- HD Artwork
- Detached Player

## 天气

- 城市
- 搜索
- 单位
- Refresh

## 高级

- Animation Debug Slow Motion
- Log Level
- Cache
- Reset
- 配置文件位置
- Debug Info

---

# 30. Animation Debug

新增只用于开发的：

- 1×
- 0.5×
- 0.2×

Slow Motion 不改变 Spring 目标，只改变 Motion Time Scale。

用来检查：

- Album Rect
- Surface Morph
- Clip
- Activity
- Widget reflow
- Radius
- Region

Release UI 可以把入口藏到高级设置或 Debug Flag。

---

# 31. 测试矩阵

## UI 状态

- No Media
- Media Playing
- Media Paused
- Missing Artwork
- Long Title
- Long Artist
- Unknown Duration
- Disabled Previous/Next
- Timer Active
- Timer Finished
- Volume Device Missing
- Weather Failure

## Motion

- Expand 30 次
- Collapse 30 次
- 在 20% 反向
- 在 50% 反向
- 在 80% 反向
- Hover 边界快速进出
- 切歌时展开
- 展开时切歌
- 设置 Slider 拖动时同时展开

## DPI

- 100 / 125 / 150 / 175 / 200%

## Edge

- Top / Right / Bottom / Left

## Monitor

- 单屏
- 双屏
- 不同 DPI
- 主屏变更
- 睡眠恢复

## Accessibility

- Tab
- Shift+Tab
- Enter
- Space
- Arrow
- UIA
- Screen Reader 基础读出

---

# 32. 性能验收

## Idle

- 不保持 60 Hz Frame Timer
- 不持续 Present
- 不重复创建 Blur
- Settings 无操作时不持续重绘

## Motion

记录：

- render CPU time
- Present interval
- Region update time
- Bitmap upload time
- Blur build time

不要只使用 `drawAndPresentP95Ms` 判断动画，因为现有该指标包含 Present 等待。

建议新增更细指标：

- `renderCpuP95Ms`
- `regionCpuP95Ms`
- `artworkUploadMs`
- `blurBuildMs`

---

# 33. 重构顺序

## Phase 0：保护层

先做：

- 新分支
- 现有 UI 自动化基线
- 配置备份测试继续保持
- 增加 V2 Feature Flag / Dev Flag
- 补充性能指标

不要直接删旧路径。

---

## Phase 1：Settings Runtime Engine

完成：

- Runtime / Persisted / Pending
- SettingsPatch
- Revision
- Save Coalescing
- Debounce
- Save Error UI
- Exit Flush

这一步先不大改视觉。

---

## Phase 2：Settings 2.0 UI

完成：

- 删除 Preview
- 新 D2D Settings Shell
- 左侧导航
- Cards
- Toggle / Slider / Segmented / Dropdown
- Native EDIT overlay
- 实时 App 更新
- 窗口避让 Isle

---

## Phase 3：Layout / Motion 分层

完成：

- UiState
- LayoutSnapshot
- VisualState
- AnimatedRect
- Motion Profile
- 旧 `retarget()` 职责缩小

---

## Phase 4：Shared Album

完成：

- 单 Album Visual
- 稳定 Crop
- Compact / Hover / Expanded targets
- 动画中途反向
- Track Crossfade
- 128 + 512 Artwork Tier

---

## Phase 5：Liquid Full Player

完成：

- Island Morph
- Album
- Title
- Artist
- Progress
- Capability-driven Controls
- Content Spring
- Hover / Leave Grace

---

## Phase 6：Glass

完成：

- Album Glass
- Dynamic Tint
- Contrast Clamp
- Blur Cache
- Optional Windows Acrylic 实验与 fallback

---

## Phase 7：Live Activities

先做：

- Timer
- Volume

完成：

- Activity Manager
- Priority
- TTL
- 左右 Slots
- Activity Spring

---

## Phase 8：Widget Shelf

完成：

- Built-in Widget Registry
- Solo / Dual / Compact Layout
- 配置存储
- 未知 ID 保留

---

## Phase 9：Widget Editor

最后再做：

- Add
- Remove
- Reorder
- Resize / Span
- Drag
- Keyboard alternative

---

## Phase 10：Detached Player 统一

根据用户实际使用情况：

- 重写旧 GDI Player
- 或复用新 Renderer
- 或把它降级为可选功能

---

# 34. 代码结构目标

```text
crates/isle-core/
    settings/
    activity/
    widgets/
    media_capabilities/

crates/isle-ui/
    motion/
        spring.rs
        profile.rs
        animated_rect.rs
    layout/
        mod.rs
        surface.rs
        widgets.rs
        activities.rs
    visual_state.rs
    geometry.rs

crates/isle-app/
    settings/
        window.rs
        render.rs
        controls.rs
        input.rs
        sections/
    render/
        surface.rs
        music.rs
        glass.rs
        activities.rs
        widgets.rs
    artwork.rs
    media.rs
    system_audio.rs
    main.rs
```

不要求一次完全达到，但每次新增代码应朝这个方向走。

---

# 35. P0 验收标准

只有下面全部完成，才能认为 UI 2.0 基础完成。

### Settings

- 无假 Preview
- 改设置真实 App 立即变化
- 无 Apply 按钮
- 快速修改不丢设置
- 保存失败可见
- 重启状态一致
- 外部配置冲突保护仍然有效

### Motion

- Expand / Collapse 全程 Spring
- 动画可随时反向
- 无明显二次 Bounce
- 无跳帧式位置改变
- Hover 不抖动

### Album

- Compact 与 Full Player 是同一个 Shared Visual
- 0 / 20 / 40 / 60 / 80 / 100% 都没有跳位
- 切歌不改变 Rect
- 高分辨率封面加载不阻塞展开

### UI

- Settings 不再像传统 Win32
- Card / Text / Spacing / Radius 统一
- Full Player 与 Island 属于同一设计系统
- 无功能按钮不显示，不做“假功能”

### Performance

- Idle 不持续刷新
- Blur 不逐帧重算
- Region 没有成为显著性能瓶颈
- 60 Hz 动画稳定

---

# 36. 暂不做的事情

为了防止范围失控，UI 2.0 第一轮明确不做：

- 第三方插件 SDK
- 任意外部 Widget 市场
- 全部 Droppy 功能复制
- Motion Artwork 全屏动态背景
- 所有软件的 Playing Next
- 复杂文件 Shelf
- 直接追求 144 / 240 Hz
- 同时重写旧 Detached Player
- 每个控件独立自定义 Spring 参数

先把：

**Surface、Settings、Shared Album、Full Player、Glass、Motion**

做对。

---

# 37. 最终产品体验目标

Isle 不应该给人：

“打开了一个播放器窗口”的感觉。

正确体验应该是：

- 小岛看到用户靠近，会有很轻的响应。
- 点击以后，原来的 Surface 自己开始生长。
- 专辑图没有消失，而是沿着连续轨迹移动并放大。
- 标题、进度和控制并不是突然出现，而是在同一组 Spring 中自然找到自己的位置。
- 动画中途反向时没有断点。
- 切歌时 Surface 不变，只是内容在原来的空间中自然更新。
- 音量或倒计时出现时，旁边的 Activity 像 Surface 自己长出来，而不是弹出另一个窗口。
- 用户打开设置后，调整圆角、位置、长度、Glass，真正的 Isle 当场响应。
- Settings 本身安静、克制、现代，不抢主界面的视觉。

最终不是：

**“Windows 上模仿 iPhone 灵动岛。”**

而是：

**“Windows 上一个具有物理感、可扩展、能和系统状态连接的动态 Surface。”**

---

# 38. 最终优先级

## P0

- Settings Runtime Engine
- Settings 2.0
- Layout / Motion 分层
- Shared Album
- Liquid Full Player
- Spring Interrupt / Reverse
- Artwork 多分辨率
- Hover 稳定性

## P1

- Album Glass
- Dynamic Tint
- Capability-driven controls
- Timer / Volume Live Activity
- 高 DPI / 四边完整适配
- Settings UIA

## P2

- Widget Shelf
- Widget Editor
- System Acrylic
- 120 Hz adaptive motion
- Detached Player 统一
- 第三方 Integration / Plugin 扩展

---

## 一句话执行标准

**任何 UI 改动都先问三件事：它是否和真实状态一致、它是否能被 Spring 自然打断、它是否会让后续 Widget / Live Activity 更容易而不是更难。**
