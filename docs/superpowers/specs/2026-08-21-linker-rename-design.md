# Linker 全量重命名设计

日期：2026-08-21  
状态：已实施

## 目标

将当前产品完整重命名为 Linker，使公开命令、Rust crate、代码符号、运行时目录、安装包、发布配置和文档使用一致的新名称。

这是一次不兼容升级。新版不提供旧命令、旧环境变量或旧运行时路径的兼容入口，并将版本提升到 `0.2.0`。

本次只改变品牌和标识符，不改变双向同步算法、排除规则、冲突决策以及 `remove`、`delete` 的业务语义。

## 最终命名

| 类别 | 最终名称 |
|---|---|
| 产品与仓库 | `Linker`、`SleeplessCatty/Linker` |
| 主命令 | `linker` |
| 后台命令 | `linkerd` |
| Rust crate | `linker-cli`、`linker-core`、`linker-daemon` |
| Rust crate 引用 | `linker_core` |
| 公共错误类型 | `LinkerError` |
| Application Support | `~/Library/Application Support/Linker` |
| 测试覆盖环境变量 | `LINKER_APP_SUPPORT_DIR` |
| 忽略文件示例 | `.linkerignore` |
| 保留控制目录名 | `.linker` |
| 临时文件前缀 | `linker-tmp-*` |
| daemon lock | `linkerd.lock` |
| LaunchAgent | `com.linker.linkerd` |
| Homebrew Formula | `linker.rb`、`class Linker` |
| 发布版本 | `0.2.0` |

## 代码与目录结构

三个 crate 目录将分别命名为：

```text
crates/linker-cli
crates/linker-core
crates/linker-daemon
```

Workspace member、包依赖、Rust import、测试 binary lookup 和开发命令全部使用这些名称。

LaunchAgent 模板命名为：

```text
packaging/launchagent/com.linker.linkerd.plist.in
```

Homebrew Formula 命名为：

```text
packaging/homebrew/linker.rb
```

与品牌无关的内部模块名称，例如 `ops`、`state`、`rules` 和 `sync`，保持不变。

## 运行时状态

Linker 使用独立的新运行时目录：

```text
~/Library/Application Support/Linker/
├── state.sqlite
├── manifests/
├── rules/
├── logs/
├── tmp/
└── bin/
```

SQLite 表结构和 manifest schema 保持不变。由于旧运行时状态会被删除，本次不实现数据库迁移或旧路径 fallback。

运行时以 SQLite 为事实来源，manifest 和规则文件继续作为本地快照。同步引擎的数据流和文件状态决策保持现状。

## 旧版清理

新版安装器必须主动清理上一品牌留下的本地运行时状态。清理逻辑将被隔离为可测试的 Shell 函数，并由安装和卸载脚本复用。

安装顺序：

1. 校验用户、物理路径、链接目录和旧状态布局，并成功构建 `linker` 和 `linkerd`。
2. 安装 Linker 二进制、受管命令链接和原子写入的 LaunchAgent plist。
3. 启动 `linkerd`，并连续确认 LaunchAgent、daemon 二进制和进程锁均健康。
4. 停止旧 LaunchAgent，并验证服务确实消失。
5. 仅在新 daemon 启动成功后，删除旧 plist、受管命令链接和旧 Application Support 工件。

清理必须满足：

- 目标不存在时正常成功，可重复执行。
- 只删除指向旧安装目录的命令链接，不删除同名的其他程序。
- 不读取旧数据库来执行目录删除。
- 不删除任何源目录。
- 不删除任何同步目标目录。
- 清理后不自动迁移关联，用户需要重新运行 `linker add`。

为同时满足彻底清理和数据安全，旧数据库只用于删除否决：如果其中记录的源目录或目标目录等于或位于旧状态根目录内，安装立即停止，数据库中的路径绝不会成为删除目标。清理不使用递归删除，只移除能证明属于应用的精确文件；发现未知目录、symlink ancestry、损坏数据库或其他不确定内容时一律 fail-closed 并保留旧状态。

自定义命令链接目录必须已存在且由当前用户可写。`sudo` 只允许用于经过验证、无 symlink ancestry 的固定 `/usr/local/bin` 路径。LaunchAgent plist 通过同目录临时文件、XML escaping 和原子 rename 写入，不能跟随已有目标 symlink。

为了定位旧安装，旧标识符只允许存在于隔离的清理实现和对应测试中。其他代码、配置和文档不得继续使用旧标识符。

Linker 自身的卸载语义维持现状：停止服务并删除二进制，但默认保留 `~/Library/Application Support/Linker` 中的状态。

## CLI 与 daemon

主命令保留现有子命令和参数：

```text
linker add
linker rule
linker list
linker status
linker doctor
linker sync
linker remove
linker delete
```

所有帮助文本、错误提示和示例使用 `linker`。不安装旧命令别名。

daemon 命令为 `linkerd`，继续使用现有行为：

- 启动时执行一次同步；
- 递归监听源目录与目标目录；
- 文件事件使用 2 秒防抖；
- 每 300 秒重新加载关联并全量校准；
- 使用 `linkerd.lock` 保证 daemon 单实例运行。

## 安装与发布

远程安装脚本和文档指向：

```text
https://github.com/SleeplessCatty/Linker
```

安装脚本使用 `LINKER_REPO_URL` 和 `LINKER_REF` 覆盖仓库及 ref。

Homebrew tap 使用 `SleeplessCatty/linker`。在 `v0.2.0` 发布归档及 SHA-256 可用之前，Formula 只提供 HEAD 安装，不能包含旧发布包 SHA 或虚构的新 SHA。

Formula 仅用于全新二进制安装，不执行用户级不兼容升级、旧状态清理或 LaunchAgent 注册。已有 pre-0.2 安装必须先使用受保护的脚本安装器完成切换。

本次源码修改不执行以下操作：

- 不创建或移动 Git tag；
- 不推送提交；
- 不重写 Git 历史；
- 不修改本地 `origin`；
- 不在外部平台执行仓库改名。

外部 GitHub 仓库完成改名后，再单独更新本地远端地址并创建正式发布。

## 错误处理与安全边界

- 清理旧 LaunchAgent 时，服务不存在不视为错误；其他停止错误必须中止，且删除前必须验证服务不再运行。
- 清理链接前必须验证链接目标位于旧安装目录。
- 删除旧状态前必须验证目标是用户 Application Support 下的精确旧目录，拒绝空路径、HOME 根目录和宽泛目录。
- 构建必须在清理前完成，避免构建失败后提前破坏旧安装。
- 安装后启动新 LaunchAgent 或健康检查失败时保留已安装的 Linker 二进制、全部旧状态以及仍在运行的旧 daemon，允许用户修复环境后重试。
- 同步过程中的现有错误传播和 item 状态标记行为保持不变。

## 测试策略

实施采用测试先行：

1. 先把 CLI 和 daemon 集成测试改为期望新的二进制、环境变量、帮助文本和日志前缀，并确认旧实现无法通过。
2. 为旧版清理函数添加 Shell 测试，并确认缺少实现时测试失败。
3. 实施目录、包、符号、运行时和发布配置重命名。
4. 执行一次 `cargo clean` 清除旧 crate 和二进制构建产物。
5. 使用新名称重新构建，运行全部测试并执行残留扫描。

清理测试覆盖：

- 旧状态目录删除；
- 旧 LaunchAgent plist 删除；
- 指向旧安装目录的旧链接删除；
- 指向其他位置的同名链接保留；
- 模拟源目录和目标目录保留；
- 重复清理成功。
- 根路径别名、UID 0 和 symlink ancestry 被拒绝；
- 未知旧状态内容、旧状态根内的源/目标以及损坏数据库均 fail-closed；
- daemon 停止失败时 plist、状态和二进制保持不变；
- plist 目标 symlink 不会覆盖用户文件，XML 特殊字符正确转义；
- 远程安装对 branch、tag 和 commit SHA 均解析到精确提交。

最终验证命令包括：

```text
cargo fmt --all -- --check
cargo test --workspace --all-targets
cargo check --workspace --all-targets
cargo metadata --no-deps --format-version 1
bash -n scripts/install.sh scripts/uninstall.sh scripts/install-remote.sh scripts/lib/cleanup-legacy.sh scripts/tests/legacy-cleanup.sh scripts/tests/install-safety.sh scripts/tests/uninstall-safety.sh scripts/tests/remote-ref.sh scripts/tests/branding-residue.sh
bash scripts/tests/legacy-cleanup.sh
bash scripts/tests/install-safety.sh
bash scripts/tests/uninstall-safety.sh
bash scripts/tests/remote-ref.sh
bash scripts/tests/branding-residue.sh
test -x target/debug/linker
test -x target/debug/linkerd
```

`branding-residue.sh` 只允许旧标识符存在于清理实现、清理测试和该残留检查脚本本身。CI 同步增加 Shell 语法检查、清理测试和品牌残留测试。

## 验收标准

- 产品、crate、代码符号、运行时、安装包和文档统一使用 Linker 命名。
- `linker` 和 `linkerd` 构建成功，旧二进制不再由 workspace 生成。
- 新测试环境变量和 Application Support 路径生效。
- 新版安装器可安全、幂等地清理旧安装，同时保留所有源目录和目标目录。
- 除隔离的旧版清理实现与测试外，当前工作树没有旧品牌或旧标识符残留。
- Rust、Shell 和 CI 验证全部通过。

## 非目标

- 不修改同步算法或冲突策略。
- 不迁移旧 SQLite 数据或关联。
- 不提供旧命令别名。
- 不创建 `v0.2.0` 发布或计算发布包 SHA。
- 不重写包含旧名称的 Git 历史。
