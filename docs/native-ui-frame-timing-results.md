# 原生原型：帧调度、字体和输入改进

日期：2026-09-27。延续 [首轮记录](native-ui-prototype-results.md)。仍是独立原型，尚未接入真实媒体与天气，正式版没有替换。

后续显示约束、隐藏释放与 MSAA/UIA 桥接见 [第三轮记录](native-ui-display-accessibility-results.md)。

## 本轮实现

- 连续动画使用高精度、单次等待计时器，以约 60 次/秒为目标；消息循环同时等待输入与计时器，不忙等、不修改系统全局计时精度。绘制耗时计入下一次等待，静止时取消帧计时器。系统不支持高精度时回退普通等待计时器，报告记录实际路径。
- 只在显式启用诊断日志时收集有上限的帧耗时和调用间隔；呈现间隔从预热 5 秒之后收集，与绘制耗时分开。
- 从已有字体文件加载 MiSans Regular/Medium/Bold 私有集合；随构建复制到 exe 同级 `fonts/`。时间和倒计时采用 Bold，标题采用 Medium，不向系统安装字体。
- 长标题宽度在创建 DirectWrite 布局时缓存；滚动按实际溢出决定，切歌、返回音乐和外形变化后重新停留。
- 读取 Windows 客户端动画偏好，并响应设置变更消息；F6/命令行提供本次运行覆盖。时钟定时器对齐整分钟，系统时间变化时重新计算。
- Tab/Shift+Tab 遍历全部工具与页面控件，Enter/Space 激活；音量刻度尺聚焦后用方向键调节。加入焦点轮廓、按压反馈，修复工具栏裁剪范围外的命中。

计时器 API 的兼容性依据：[CreateWaitableTimerExW](https://learn.microsoft.com/en-us/windows/win32/api/synchapi/nf-synchapi-createwaitabletimerexw)。消息等待依据：[MsgWaitForMultipleObjectsEx](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-msgwaitformultipleobjectsex)。

## 三轮音乐模拟场景

采样版本：`4a3f05a`，SHA-256 `2FBB1CE19B98D5989195A92FAF0334DDADAF28290B0B4130F387912928CE88C9`。这是帧调度/字体改进后的版本，后续键盘焦点与实际溢出判断另做回归检查。

每轮预热 5 秒，CPU/内存采样 60 秒，独立重启。场景为展开音乐、模拟频谱和长标题，禁止测量窗口交互。机器环境同首轮，200 Hz 显示器；其他桌面程序保持运行。

| 轮次 | 平均调用间隔 | P95 调用间隔 | 估算调用频率 | CPU 全机归一 | 私有内存均值 | 工作集均值 |
|---|---:|---:|---:|---:|---:|---:|
| 1 | 17.16 ms | 17.81 ms | 58.3/s | 0.697% | 57.60 MiB | 45.63 MiB |
| 2 | 16.99 ms | 17.82 ms | 58.9/s | 0.724% | 56.64 MiB | 45.62 MiB |
| 3 | 17.05 ms | 18.14 ms | 58.6/s | 0.781% | 57.10 MiB | 45.51 MiB |

上一轮约 39.5 次/秒的问题已显著改善。CPU 与之前单轮的 0.740% 处于接近范围，不能依据这组探索性采样宣称统计显著下降。

**这里测的是相邻绘制/Present 返回的时间，不是 DWM 最终上屏时间或输入延迟。** 200 Hz 高刷新率自适应、GPU/DWM 开销及实际掉帧仍需 PresentMon/ETW 验证。也不能用模拟音频场景推算真实 WASAPI 采集后的占用。

每轮句柄起止增加 6 个，私有内存变化约 -0.004～+0.35 MiB；尚不能据此证明长时间无泄漏。

原始数据：[采样 CSV](performance/native-frame-timing/music/performance.csv)、[汇总](performance/native-frame-timing/music/performance-summary.json)。

## 静止与生命周期回归

- 加入键盘控件后的静止版本采样 60 秒：CPU 0.0064%，私有内存 55.19 MiB，工作集 41.20 MiB。整个 67.40 秒生命周期绘制 3 次，未持续绘制；诊断退出检查仍使用 1 秒窗口定时器，不触发绘制。
- 30 次独立进程/窗口启动与退出：每次返回并收起后，页面实例为 0、定时器间隔为 0、退出码为 0，实际字体均为 MiSans。此项证明这些关闭路径完成，不替代单进程长时间堆和句柄平台期测试。
- 时间页收到 `WM_TIMECHANGE` 后仍使用整分钟边界定时器，没有连续动画采样。
- 最终代码通过 14 项单元测试、严格 Clippy、格式检查、Release 构建、8 种布局窗口路由检查和音乐→音量→返回→收起烟雾检查。
- 最终二进制 SHA-256：`3480E8214F7C73337AC086414CDC5CC953ECB160B2030FB99BF94EC516E7ECFE`。键盘与溢出判断改动后的 15 秒复测，预热后 589 个间隔样本均值 16.97 ms、P95 17.90 ms，约 58.9 次/秒；这次短回归不替代上表三轮采样。[最终复测](performance/native-frame-timing/checks/final-frame-check.json)
- Windows 减少动画偏好的读取/消息处理已实现；未更改用户全局设置进行自动测试，跨 DPI/休眠/设备丢失仍未完成实机验收。

## 剩余阶段门槛

继续完成混合 DPI 与小工作区约束、窗口隐藏/恢复、实际硬件点击和拖动、tooltip/UI Automation、旧版弹簧轨迹校准、辅助窗口释放以及隔离旧版同功能对照。P1 尚未整体通过，不提前宣称完整迁移或替换正式版。
