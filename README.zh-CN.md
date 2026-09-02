# mkd

[![Crates.io](https://img.shields.io/badge/crates.io-mkd-orange)](https://crates.io/crates/mkd)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](./LICENSE)
[![Rust](https://img.shields.io/badge/Rust-1.85%2B-black)](https://www.rust-lang.org)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-lightgrey)](#)

**[中文](#中文) | [English](./README.md)**

---

## 中文

`mkd` 是一个跨平台目录书签工具。它使用 Rust TUI 管理、搜索和选择书签，并通过 Shell 函数让选择结果改变当前 Shell 的工作目录——像 `zoxide` 一样注册，但书签由你手动收藏，支持分组管理。

### 安装

**方式一：包管理器**

```console
# Homebrew（macOS / Linux ARM64）
brew install 821869798/mkd/mkd

# Scoop（Windows）
scoop bucket add 821869798 https://github.com/821869798/scoop-bucket
scoop install mkd

# asdf
asdf plugin add mkd https://github.com/821869798/asdf-mkd.git
asdf install mkd latest

# mise：registry 合并前用 asdf 插件全名
mise use -g asdf:821869798/asdf-mkd
# registry PR 合并后短名可用：
# mise use -g mkd@latest
```

**方式二：一键安装脚本**

```bash
# Linux / macOS
curl -fsSL https://raw.githubusercontent.com/821869798/markd/master/packaging/install.sh | sh
```

```powershell
# Windows PowerShell
irm https://raw.githubusercontent.com/821869798/markd/master/packaging/install.ps1 | iex
```

默认安装到 `~/.local/bin`（可用 `DEST` / `$env:DEST` 覆盖）。

**方式三：下载预编译二进制**

从 [Releases 页面](https://github.com/821869798/markd/releases) 下载对应平台的压缩包，解压后把 `mkd`（或 `mkd.exe`）放进 `PATH` 里的任意目录：

| 平台 | 压缩包 |
| --- | --- |
| Windows x64 | `mkd-x86_64-pc-windows-msvc.zip` |
| Windows ARM64 | `mkd-aarch64-pc-windows-msvc.zip` |
| macOS ARM64（Apple 芯片） | `mkd-aarch64-apple-darwin.tar.gz` |
| Linux x64 | `mkd-x86_64-unknown-linux-gnu.tar.gz` |
| Linux ARM64 | `mkd-aarch64-unknown-linux-gnu.tar.gz` |

示例（Linux/macOS）：

```bash
VERSION=v0.1.0
curl -LO https://github.com/821869798/markd/releases/download/$VERSION/mkd-aarch64-apple-darwin.tar.gz
# Linux x64 对应 mkd-x86_64-unknown-linux-gnu.tar.gz
tar -xzf mkd-*.tar.gz
sudo mv mkd-*/mkd /usr/local/bin/
mkd --version
```

**方式四：源码编译安装**

需要已安装 Rust 工具链。进入源码目录后运行：

```console
cargo install --path .
```

确认 Cargo 的二进制目录（通常是 `~/.cargo/bin`，Windows 通常是 `%USERPROFILE%\.cargo\bin`）已经加入 `PATH`。

### 快速上手

1. **注册 Shell 函数**（每个 Shell 只需一次）：

```console
mkd setup          # 自动检测当前 Shell，确认后写入配置
```

然后**新开一个终端窗口**（或重载配置），验证函数已生效——PowerShell 里 `Get-Command mkd` 应显示 `Function` 而不是 `Application`。

2. **添加第一批书签**：

```console
mkd add .                          # 收藏当前目录
mkd add ~/projects/api --name api --category work
```

3. **跳转**：

```console
mkd          # 打开 TUI
```

`j/k` 或方向键移动，`/` 模糊搜索，`Enter`（或鼠标双击）跳转。访问越多的目录自动排越前。

4. **分组管理**——TUI 里按 `c` 新建分组，选中书签按 `a` 在弹窗里上下选分组（`↑↓` + 回车）。也可以用命令行：

```console
mkd category add work
mkd list --category work
```

5. **不再需要时卸载注册**：

```console
mkd setup --remove
```

### Shell 注册

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

| Shell | 自动注册目标 | 在当前会话重载 |
| --- | --- | --- |
| Bash | `~/.bashrc` | `source ~/.bashrc` |
| Zsh | `~/.zshrc` | `source ~/.zshrc` |
| Fish | `~/.config/fish/config.fish` | `source ~/.config/fish/config.fish` |
| PowerShell | 运行时查询 `$PROFILE.CurrentUserCurrentHost` | `. $PROFILE.CurrentUserCurrentHost` |

修改已存在的 profile 前，mkd 会在同目录创建 `<profile>.mkd-backup`。如果 profile 中的 mkd 标记残缺、重复或嵌套，setup 会停止且不改动原文件。

自动注册无法定位 profile 时，可以手动加载 `init` 输出：

```bash
eval "$(mkd init bash)"        # Bash
eval "$(mkd init zsh)"         # Zsh
mkd init fish | source          # Fish
mkd init powershell | Out-String | Invoke-Expression   # PowerShell
```

### 日常使用

添加当前目录书签并指定名称和分组：

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
| 上下选择 | `j` / `k` 或方向键 |
| 切换分组与书签区域 | `Tab`，或鼠标单击对应区域 |
| 模糊搜索 | `/`，或鼠标单击顶部搜索框 |
| 跳转到选中目录 | `Enter`，或鼠标左键双击书签 |
| 归组：为选中书签选择分组 | `a`，弹窗中 ↑↓ 选分组，回车确认 |
| 新建分组 | `c`（输入名字） |
| 重命名分组 / 删除分组 | `r` / `D`（`D` 需再按一次确认） |
| 重命名书签 / 删除书签 | `e` / `d`（`d` 需再按一次确认） |
| 复制路径到剪贴板 | `y` |
| 帮助弹窗 | `h` |
| 取消 / 退出 | `Esc` |

按 `h` 可随时查看完整快捷键列表。失效目录会在列表中标记，不能跳转，但仍可删除。

#### 分组（分类）工作流

1. 按 `c` 新建分组（这是唯一需要输入名字的地方），或直接跳过这步用已有分组
2. 选中一个书签（右侧列表 `j/k` 或鼠标）
3. 按 `a` → 弹出分组选择窗 → `↑↓` 选分组 → 回车确认
4. 弹窗里最后一项"新建分组…"可以直接建新组

删除分组（`D`）后组内书签自动归回 default；`d` 删除书签是从所有分组彻底移除。

命令行等价操作：

```console
mkd category add work
mkd add . --name markd --category work
mkd list --category work
mkd category rename work jobs
mkd category remove jobs
```

### 数据文件

默认数据文件是平台 `data_local_dir` 下的 `bookmarks.json`：

| 平台 | 默认位置 |
| --- | --- |
| Linux | `~/.local/share/mkd/bookmarks.json` |
| macOS | `~/Library/Application Support/mkd/bookmarks.json` |
| Windows | `%LOCALAPPDATA%\mkd\data\bookmarks.json` |

可用 `MKD_DATA_FILE` 环境变量指向其他文件。数据以 JSON 保存，写入时使用同目录临时文件原子替换；JSON 损坏时 mkd 会报告路径并退出，不会覆盖损坏文件。

### 第一版限制

- 不提供云同步、团队共享、跨设备合并或插件机制
- 数据文件没有多进程锁；避免多个 mkd 进程同时修改同一文件
- TUI 需要支持原始模式和鼠标事件的交互式终端
- 书签路径必须是添加时存在的目录；含 CR/LF 的路径会被拒绝（保证 Shell 输出始终是单行安全路径）

## 许可证

[MIT](./LICENSE)
