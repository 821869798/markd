# mkd

`mkd` 是一个跨平台目录书签工具。它使用 Rust TUI 管理、搜索和选择书签，并通过 Shell 函数让选择结果改变当前 Shell 的工作目录。

## 安装

需要已安装 Rust 工具链。进入源码目录后运行：

```console
cargo install --path .
```

确认 Cargo 的二进制目录（通常是 `~/.cargo/bin`，Windows 通常是 `%USERPROFILE%\.cargo\bin`）已经加入 `PATH`。

## Shell 注册

安装完成后，注册与当前 Shell 对应的函数：

```console
mkd setup
```

省略 Shell 时，`mkd` 会尝试自动检测。也可以显式指定 Shell，并使用以下选项预览、免确认写入或移除配置：

```console
mkd setup bash --dry-run
mkd setup powershell --yes
mkd setup --remove zsh
```

`--dry-run` 只显示计划和托管块，不写入文件。默认写入前会要求确认；`--yes` 跳过确认。重复执行 setup 会更新同一个托管块，不会重复追加。`--remove` 只删除 mkd 的完整托管块。

各 Shell 的自动注册目标和重载方式如下：

| Shell | 自动注册目标 | 在当前会话重载 |
| --- | --- | --- |
| Bash | `~/.bashrc` | `source ~/.bashrc` |
| Zsh | `~/.zshrc` | `source ~/.zshrc` |
| Fish | `~/.config/fish/config.fish` | `source ~/.config/fish/config.fish` |
| PowerShell | 运行时查询 `$PROFILE.CurrentUserCurrentHost` | `. $PROFILE.CurrentUserCurrentHost` |

也可以关闭并重新打开 Shell。修改已存在的 profile 前，mkd 会在同目录创建 `<profile>.mkd-backup`；例如 `.bashrc.mkd-backup`。如果 profile 中的 mkd 标记残缺、重复或嵌套，setup 会停止且不改动原文件。

自动注册无法定位 profile 时，可以手动加载 `init` 输出。Bash 当前会话的命令是：

```bash
eval "$(mkd init bash)"
```

Zsh 可使用相同形式并将参数改为 `zsh`；Fish 使用 `mkd init fish | source`；PowerShell 使用 `mkd init powershell | Out-String | Invoke-Expression`。需要永久生效时，把对应命令加入该 Shell 的 profile。

## 日常使用

添加当前目录书签并指定名称和分类：

```console
mkd add . --name markd --category work
```

直接运行 `mkd` 打开 TUI：

```console
mkd
```

常用操作：

| 操作 | 按键或鼠标 |
| --- | --- |
| 上下选择 | `j` / `k` 或方向键下 / 上 |
| 切换分类与书签区域 | `Tab` |
| 开始模糊搜索 | `/`，然后输入搜索文本 |
| 跳转到选中目录 | `Enter`，或鼠标左键双击书签 |
| 取消当前搜索、编辑或确认；无待处理操作时退出 | `Esc` |
| 删除书签 | `d`，再次按 `d` 确认 |
| 重命名书签 | `e` |
| 新建分类 | `c` |
| 重命名分类 | `r` |
| 删除分类 | `D`，再次按 `D` 确认 |
| 移动书签到分类 | `m` |

鼠标单击可以选择分类或书签。失效目录会在列表中标记，不能跳转，但仍可删除。

非交互管理命令可通过 `mkd --help` 和各子命令的 `--help` 查看。例如 `mkd list`、`mkd remove <BOOKMARK>`、`mkd rename <BOOKMARK> <NAME>` 以及 `mkd category add|list|remove|rename`。

## 数据文件

默认数据文件是平台 `data_local_dir` 下的 `bookmarks.json`：

| 平台 | 默认位置 |
| --- | --- |
| Linux | `$XDG_DATA_HOME/mkd/bookmarks.json`；未设置时为 `~/.local/share/mkd/bookmarks.json` |
| macOS | `~/Library/Application Support/mkd/bookmarks.json` |
| Windows | `%LOCALAPPDATA%\mkd\data\bookmarks.json` |

可以用 `MKD_DATA_FILE` 指向其他文件。该变量必须在调用 mkd 的 Shell 环境中设置，例如：

```bash
export MKD_DATA_FILE="$HOME/.mkd-bookmarks.json"
```

PowerShell 示例：

```powershell
$env:MKD_DATA_FILE = "$HOME\mkd-bookmarks.json"
```

数据以 JSON 保存，写入时使用同目录临时文件替换。若 JSON 损坏，mkd 会报告文件路径并退出，不会自动覆盖损坏文件。恢复时先在 mkd 外部备份原文件，再修复 JSON 或用已知正常的备份替换；确认恢复完成前不要删除原文件。

## 第一版限制

- 不提供云同步、团队共享、跨设备合并、跨机器导入导出或插件机制。
- 不提供后台守护进程、目录内容索引或 Shell 补全。
- 数据文件没有多进程锁协议；避免多个 mkd 进程同时修改同一数据文件。
- TUI 需要支持原始模式和鼠标事件的交互式终端；在非终端 stdin/stderr 环境中不能选择目录。
- 书签路径必须是添加时存在的目录。为保证 Shell 输出始终是一行安全路径，路径中含 CR (`\r`) 或 LF (`\n`) 时会被拒绝。
