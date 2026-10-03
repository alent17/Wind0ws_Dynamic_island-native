# main 原生 UI 切换：1.0.11

main 的默认 `dev`、`preview`、`build`、`check`、`test`、`bundle:windows` 命令已指向 `native/`。旧版 Tauri 通过 `web:*` 命令继续构建，Playwright 也使用显式 Web 开发服务器。

正常启动原生 EXE 默认连接真实 Windows 媒体、系统音量和频谱。`--demo` 与隔离诊断保留测试数据；`--open-floating` 可在正常运行时打开原生悬浮播放器。

新增纯原生 NSIS 安装器和便携 ZIP 打包脚本。安装目录内的 `isle.exe` 是原生构建的直接副本；Windows 版本资源为 1.0.11，描述为 Isle Native，携带应用图标。

## 已执行验证

- `npm run check`：原生 workspace 通过。
- `npm test`：69 项通过，1 项联网测试按原有设计忽略。
- `cargo fmt --manifest-path Cargo.toml --all -- --check`：通过。
- `npm run bundle:windows`：原生 NSIS 安装包与 ZIP 成功生成。
- `python scripts/verify-package.py dist/native-1.0.11/isle.exe`：隔离配置、副屏，主岛和悬浮播放器 HWND 均可见，悬浮背景完成绘制，没有子运行时进程。
- 本机 `E:\Isle\isle.exe` 已安装为 Isle Native 1.0.11，与原生 release EXE 的 SHA256 相同；启动原生悬浮播放器，确认运行进程没有子进程。

旧安装版 EXE 和卸载程序备份位于本机 `dist/backup/webview-1.0.10/`，不提交个人安装文件。

托盘、MV 视频和部分捕获保护仍未迁移。本次切换不提供新的性能达标声明；原有性能记录保留。
