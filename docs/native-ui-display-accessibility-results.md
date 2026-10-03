# 原生原型：显示、生命周期与无障碍

日期：2026-09-27。延续独立原型，正式程序及其设置保持原样。未生成安装包，也未连接真实媒体/天气服务。

## 显示与缩放

- 提取工作区内窗口定位函数，保留顶部/底部/左侧/右侧锚点。
- 收到 `WM_DPICHANGED` 时立即复制建议矩形，根据目标显示器工作区重新定位，再统一更新图形目标、轮廓、输入与无障碍的物理坐标。
- 工作区小于固定容纳区域时，等比缩小整个原型以防裁切；这是一种兜底策略，小区域的文字和点击区会变小，尚未替代后续自适应布局设计。
- `WM_SETTINGCHANGE` 的工作区变化同样会触发重新定位。

验证：96/120/144/192 DPI × 正常/320×240 工作区 × 四方向，共 32 组；另向实际 HWND 发送 120→144→192→96 的 DPI 变更消息，验证图形重建和页面保留。150% 缩放下按无障碍返回的坐标发送点击，可打开音量页。

这些是**不修改系统设置的模拟 DPI 测试**，不能代替真实混合 DPI 多显示器迁移或硬件点击。API 处理依据：[WM_DPICHANGED](https://learn.microsoft.com/en-us/windows/win32/hidpi/wm-dpichanged)。

截图已检查：[150% 音乐页](images/native-display/music-150pct.png)、[小工作区天气页](images/native-display/weather-small-work-area.png)。

## 隐藏、最小化与恢复

- 隐藏/最小化后，当前页、渲染器、DirectWrite 布局/字体集合、D2D/D3D/DXGI/DirectComposition 对象引用被释放；连续帧等待取消。
- 普通隐藏状态没有刷新定时器。倒计时运行时仅保留秒级状态检查，完成后也停止；不会重新挂载隐藏页或创建图形设备。
- 隐藏期间完成倒计时，记录待展示提示；恢复时创建图形资源并展示完成页。普通恢复保持收起状态，再展开回音乐。
- 关闭时使外部无障碍对象失效，并通知 UIA 释放窗口的提供程序引用。

自动测试通过：隐藏时页面数 0 / rendererAlive=false；倒计时完成期间帧数不变；恢复后完成提示出现；最小化再次释放，恢复可重新绘制。30 次独立窗口开关回归通过。

## 原生无障碍接口

实现 `IAccessible`（MSAA）与 `IAccessibleEx`，由 Windows 的 `UiaProviderFromIAccessible` 桥接通用 UIA 属性，并为音量滑块提供原生 `RangeValuePattern`；没有透明网页或隐藏 WebView 控件。

- 暴露当前可见控件的名称、角色、焦点、状态、屏幕坐标、默认操作、音量数值和导航。
- UI 操作投递到主线程；调用方不能直接改动图形或页面状态。页面/可见控件集合变更后，旧子对象与排队旧操作通过版本号拒绝。
- 只发布当前页面/可见按钮，隐藏和收起后移除子控件。命中检测使用实际岛轮廓。
- 从另一个进程验证：UIA 发现 8 个音乐页控件，InvokePattern 打开音量；MSAA 读取名称/角色、焦点、坐标、设置并读取音量 73、返回音乐、拒绝旧控件写入、收起后子控件为 0。
- 补充验证：UIA RangeValue 报告音量最小值 0、最大值 100、步进 1；UIA 写入 67 后可重新读取，超出范围的 101 被拒绝。值变化通过 WinEvent 通知辅助技术。

本轮副屏证据：[UIA RangeValue 回归](performance/native-display-accessibility/accessibility-range-results.json)。

此处是基础 MSAA + UIA 桥接，尚未完成 Narrator/NVDA 的实际朗读验收、文本模式、tooltip 和脚本晚绑定 IDispatch。不能称为全部无障碍工作完成。桥接依据：[UiaProviderFromIAccessible](https://learn.microsoft.com/en-us/windows/win32/api/uiautomationcoreapi/nf-uiautomationcoreapi-uiaproviderfromiaccessible)。

## 可见性资源采样

资源采样构建 SHA-256：`22F101B582A3AAB618E267AEA67CC5EC30886390FF5535D82CA3468536D23FC3`。采样后又调整了错误恢复分支，确保先释放旧合成目标再重建；最终构建 SHA-256 为 `6B5135247C8B0D437A44B1164C9F9E26B2BDB169B89081807BD7C239E1DB0CAD`，无障碍回归已在最终构建上重跑。实际设备丢失仍未注入验证。

同一个原型进程：预热 5 秒后采样可见音乐 10 秒、隐藏 60 秒、恢复收起状态 10 秒。音乐与频谱仍为模拟数据。测试期间还有其他开发/验证进程运行，数据只用于验证隐藏路径，不能替代前后的受控全功能性能比较。

| 场景 | 采样时长 | 平均 CPU（全机归一） | 平均私有内存 | 平均工作集 |
|---|---:|---:|---:|---:|
| 可见音乐 | 10 秒 | 0.560% | 58.46 MiB | 50.92 MiB |
| 隐藏 | 60 秒 | 未检测到 CPU 时间增量 | 10.58 MiB | 24.87 MiB |
| 恢复收起 | 10 秒 | 0.273% | 56.81 MiB | 44.94 MiB |

隐藏期间帧计数不变、页面数 0、渲染器不存在、刷新定时器为 0。恢复后模拟频谱正常运行。保留的模块/驱动映射使工作集不会降到零；不能将此结果推广为完整正式程序的内存承诺。

原始证据：[可见性采样](performance/native-display-accessibility/visibility-resources.json)、[布局和隐藏恢复](performance/native-display-accessibility/display-lifecycle-results.json)、[无障碍](performance/native-display-accessibility/accessibility-results.json)。

## 检查与剩余工作

Release 构建、格式检查、严格 Clippy、15 项单元测试、32 组布局测试、4 次动态 DPI 变更、隐藏/最小化/倒计时恢复、30 次窗口开关与跨进程无障碍测试通过。

仍需真实多显示器/屏幕阅读器验收、设备丢失与睡眠恢复、旧版弹簧轨迹比对、辅助窗口原生化及完整旧版同功能对照。第一阶段尚未全部验收，P2 的真实媒体与音频服务仍待接入。
