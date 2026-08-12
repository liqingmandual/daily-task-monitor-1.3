# 安装说明

## 系统要求

- Windows 10 或 Windows 11（64 位）
- WebView2 Runtime（多数 Windows 10/11 设备已自带）
- 约 200 MB 可用磁盘空间，长期使用还需要为本地记录预留空间

## 安装 1.3 独立版

1. 打开项目的 [Releases](https://github.com/liqingmandual/daily-task-monitor-1.3/releases)。
2. 下载 `Orbit-1.3.0-x64-setup.exe`。
3. 在 Release 页面复制安装包的 SHA-256；也可以用以下命令计算本地文件哈希：

   ```powershell
   Get-FileHash '.\Orbit-1.3.0-x64-setup.exe' -Algorithm SHA256
   ```

4. 双击安装包，按安装向导完成安装。
5. 从开始菜单打开「Orbit」。

安装包目前未进行商业代码签名，Windows SmartScreen 可能显示「Windows 已保护你的电脑」。确认文件来自本仓库且哈希一致后，可选择「更多信息」→「仍要运行」。

## 独立安装边界

本版本使用以下独立身份：

- 应用版本：`1.3.0`
- Tauri identifier：`com.dailytaskmonitor.independent.v13`
- 数据目录：`%LOCALAPPDATA%\DailyTaskMonitorIndependent13`
- 凭据服务：`DailyTaskMonitorIndependent13`
- 进程锁：`Local\DailyTaskMonitorDesktopIndependent13`

因此它可以与其他版本并行安装，不会覆盖 TimeWheel 或 Orbit 的其他数据目录。

## 第一次启动

1. 查看右上角状态是否为「监测中」。
2. 打开设置，确认不活跃阈值、浏览器来源和隐私排除项。
3. 返回「今日」页面，填写今日目标并保存。
4. 正常使用一段时间后，检查时间线中的应用和分类是否符合预期。
5. AI 是可选功能；不配置 AI 仍可使用监测、统计、人工分类和本地工作流功能。

## AI 设置

软件支持两种执行方式：

- **API Key**：在设置中选择兼容提供商，保存自己的密钥并执行连接测试。
- **本机 Codex**：本机已经安装并登录 Codex CLI 时，可以在设置中选择并测试。

API Key 保存在 Windows Credential Manager，不写入 SQLite 数据库或日志。开启自动分析前，请先阅读 [隐私说明](../PRIVACY.md)。

## 更新、备份与卸载

更新前建议备份：

```text
%LOCALAPPDATA%\DailyTaskMonitorIndependent13\data\monitor.db
```

程序运行时数据库可能带有 `-wal` 和 `-shm` 文件。进行手工备份前应先退出程序，或同时复制这三个文件。

可以在 Windows「设置」→「应用」→「已安装的应用」中卸载。卸载程序不应被视为数据备份；重要数据请先导出或复制数据库。

## 从源码构建

```powershell
pnpm install --frozen-lockfile
pnpm test
cargo test --manifest-path src-tauri/Cargo.toml --all-targets
pnpm run tauri:build:independent-1.3
```

公开分发 1.3 独立版时必须使用 `tauri:build:independent-1.3`，以保留独立数据目录、凭据空间和进程锁。
