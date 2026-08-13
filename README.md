# 每日任务监测系统 1.3 独立版

[![CI](https://github.com/liqingmandual/daily-task-monitor-1.3/actions/workflows/ci.yml/badge.svg)](https://github.com/liqingmandual/daily-task-monitor-1.3/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/liqingmandual/daily-task-monitor-1.3)](https://github.com/liqingmandual/daily-task-monitor-1.3/releases)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

一个面向 Windows 10/11 的本地优先时间监测与任务分析工具。它记录应用使用、活跃与不活跃时间，将学习、搜索和开发活动整理成趋势与工作流，并提供可选的 AI 分析。

1.3 独立版拥有独立的安装标识、数据目录、凭据空间和进程锁，可以与其他版本并行安装，不会覆盖 TimeWheel 或每日任务监测系统的其他版本。

## 主要功能

- **今日**：查看活跃时间、学习时间、最长专注和应用切换频率；时间构成可在「全部」和「学习」之间切换。
- **趋势**：按连续日期或离散日期查看总时长、全部选中日期的日均时长、活动构成、应用构成与基准比较。
- **工作流**：自动识别开发、搜索调研和学习活动，归入项目与任务；游戏和休闲娱乐不会进入自动工作流。
- **AI 审核**：集中查看活动分类、工作流分配和执行异常，并保留人工修改入口。
- **目标辅助**：今日目标会作为分类、工作流识别和 AI 建议的辅助语义，但不会覆盖人工分类。
- **本地优先**：SQLite 数据保存在本机；API Key 保存到 Windows Credential Manager。

「学习」采用统一口径，只包含：

- 文字信息输入
- 创作开发
- 搜索/调研
- 明确为学习用途的视频

未知用途视频与普通待分类活动统一显示为「未分类」。休闲视频、游戏、社交、文件整理、不活跃和未分类不计入学习。

## 下载与安装

前往 [Releases](https://github.com/liqingmandual/daily-task-monitor-1.3/releases) 下载 `DailyTaskMonitor-1.3.0-x64-setup.exe`，双击并按安装向导操作。

安装包目前未进行商业代码签名，Windows SmartScreen 可能显示提醒。请从本仓库 Release 下载，并核对 Release 页面提供的 SHA-256。

更完整的安装步骤见 [安装说明](docs/INSTALL.md)，页面和功能说明见 [使用指南](docs/USER_GUIDE.md)。

## 数据与隐私

独立版数据库默认位于：

```text
%LOCALAPPDATA%\DailyTaskMonitorIndependent13\data\monitor.db
```

程序不会保存按键内容、鼠标轨迹、剪贴板、密码、Cookie、屏幕截图、麦克风或系统音频。启用云 AI 前，请在设置中确认发送范围。完整说明见 [隐私说明](PRIVACY.md)。

## 本地开发

需要：

- Windows 10/11
- Node.js 22
- pnpm 10
- Rust stable
- Microsoft C++ Build Tools 与 Windows SDK
- WebView2 Runtime

安装依赖并运行测试：

```powershell
pnpm install --frozen-lockfile
pnpm test
cargo test --manifest-path src-tauri/Cargo.toml --all-targets
```

启动开发环境：

```powershell
pnpm tauri dev
```

构建具有独立身份的 1.3 NSIS 安装包：

```powershell
pnpm run tauri:build:independent-1.3
```

安装包输出到：

```text
src-tauri\target\release\bundle\nsis\
```

不要使用普通的 `pnpm tauri build` 制作公开的 1.3 独立安装包；独立构建脚本会设置专用数据目录、凭据服务和进程锁。

## 技术栈

- Tauri 2
- Rust
- React 19 + TypeScript
- SQLite
- ECharts
- Vitest

## 反馈

如果遇到分类、监测连续性、安装或界面问题，请在 [Issues](https://github.com/liqingmandual/daily-task-monitor-1.3/issues) 中说明系统版本、应用版本、复现步骤和预期结果。请勿上传数据库、API Key 或包含隐私信息的窗口标题。

## License

[MIT](LICENSE)
