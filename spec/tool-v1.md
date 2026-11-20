# Marqdo CLI Tool Protocol v1 — 通用外部工具协议

> 状态：Proposed，配套 `docs/NEXT_PHASE_IMPLEMENTATION.md` §5 修订 v2。任何官方/第三方工具一视同仁；该协议**不是** C ABI 插件协议。本文不声称现有 CLI 已实现此协议。

## 1. 范围与不变量

- Core 不含具体 Tool `command` 名的注册、适配、白名单、下载地址；仅维护自身原生命令集合与用户安装的工具注册记录。
- 任何语言写成的**独立可执行工具**均可安装；CLI 不链接其 Rust crate、不解释其业务子命令。
- 最小实现只支持 `marqdo tool install /path/to/tool.json`、`marqdo tool list`、`marqdo tool remove <command>`，以及未知顶层命令的已注册工具回退。
- 无索引服务、无网络时一样能够本地安装；`marqdo tool install <name>` 的远程名称解析是以后独立的可选索引能力，不要求 core 自带官方/社区工具列表。
- 工具仅能调用公开的 `marqdo` 命令/Analysis/Trace 文件协议；不得依赖私有 `marqdo-ir` 序列化、宿主内存指针或“已知官方工具名”。

## 2. 安装清单

`tool.json` 使用 UTF-8 JSON，v1 的最小规范清单示例：

```json
{
  "schema": "marqdo.cli-tool",
  "schema_version": 1,
  "name": "benchmark",
  "version": "0.1.0",
  "command": "benchmark",
  "entrypoint": "bin/marqdo-benchmark",
  "compatibility": {
    "marqdo": ">=0.1,<1",
    "analysis": [1]
  }
}
```

类型：`schema` 固定字符串，`schema_version` 正整数（首版仅 1），`name`/`version`/`command`/`entrypoint` 非空字符串，`compatibility.marqdo` 是可解析版本范围，`analysis`、`trace` 为可选的正整数数组；仅当工具使用该数据协议时声明。未来可以增加可选字段，但不得改变现有字段含义。

`command`：建议 `^[a-z][a-z0-9-]*$`，不得与任何内置命令或保留参数冲突；同一注册表中命令唯一，名字冲突拒绝且不覆盖。`entrypoint`：包内受控相对路径，不能是绝对路径、`.`/`..` 跳出根目录、符号链接逃逸或可直接解释的任意 shell 片段。真实文件应可执行并与目标平台匹配；若扩展名或平台格式与 OS 不兼容应显式失败。

## 3. 注册与验证

本地安装流程：读取用户指定的 `tool.json` → 校验 schema 与平台、版本 → 校验 entrypoint 不越界 → 检查内置/已注册命令冲突 → 用户确认/明确请求的信任决策 → 将工具与清单以临时目录方式拷入受控安装根 → 计算可执行文件真实 SHA-256 → 原子发布注册信息。失败时回滚且不留下可执行入口。

注册记录最少包含 `command`、`name`、`version`、`entrypoint_absolute_path`、`sha256`、`source`、`schema_version`、`compatibility`。注册不是来源认证；SHA 校验只能检查文件未被改变。对互联网下载必须额外有可信索引给出的摘要/签名；禁止在用户未同意前自动从网络下载。

运行时必须校验记录的版本/平台/摘要；任何不一致拒绝而非尝试降级。实现优先使用正确的跨平台配置目录 API，不要把 `~` 当成可移植的用户目录。多个用户/多个版本并存属于独立目录隔离；同一个命令只能解析到一个注册项，除非未来另有显式版本选择规范。

## 4. 分发算法

输入 `marqdo <first> <args...>`，先检查内置命令，内置优先；否则从已安装的只读注册映射查找 `<first>`。找到则验证 Tool Protocol 与程序文件，使用 `Command::new(entrypoint).args(args)` **直接启动**，从调用者原样保留 `cwd`、`stdin/stdout/stderr` 和可保留的环境（敏感环境变量策略另行安全审计），原样传播退出状态；绝不调用 `sh -c`、不拼接命令字符串。

工具负责自己的命令层级与帮助，如 `marqdo benchmark run .`、`marqdo benchmark --help`。CLI 不读取或修改工具业务选项。Rust `clap` 可通过单一 `external_subcommand` 捕获未知子命令，但必须保留 `OsString` 原样传参；不能让工具要求 CLI 添加 `Command::View` 等专门枚举项。不要求工具在处理 `--help` 时调用核心。未知命令时报错；即使 PATH 中存在 `marqdo-<first>`，**没有显式注册也不能执行它**。

可选辅助环境：`MARQDO_BIN`（当前核心 CLI 的绝对路径），`MARQDO_TOOL_PROTOCOL=1`。工具通过此路径运行 `marqdo check ... --emit ...` 和 `marqdo run ... --trace-file ...`，不拥有额外宿主内部 API。不得自动将密钥、项目能力授予记录、ABI 插件加载句柄或用户外联会话传给工具。

## 5. 三个独立安全域

1. **Tool CLI 进程**：用户主动安装并运行的原生程序，按当前用户身份执行。它**不受** Marqdo 的 `pure/project/strict` `.mq.md` 语言沙箱自动约束；工具执行不是安装后自动执行。
2. **Marqdo 程序**：通过 `marqdo run/call` 和 `CapabilityHost` 执行，严格隔离需受支持的 OS adapter。
3. **运行时 ABI 插件**：扩展可调用函数，必须有自己版本化 C ABI、显式信任或 isolated worker，和 Tool 注册表严格区分。

## 6. 向后兼容

`schema_version` 不支持或 `marqdo` 版本范围不匹配应以稳定错误码拒绝执行；可忽略的未知扩展字段允许透传，但不能被视为隐含授权。公共 Analysis/Trace 协议单独版本管理；有工具声明 `[1]` 才参与对应版本检查，没有声明不强迫工具使用任何数据协议。

原生命令集合是保留命名空间；未来新增与社区工具冲突的新核心顶层命令属于破坏性变化，必须经过 ADR、迁移期诊断和工具冲突统计，不能静默覆盖已有已注册工具。

## 7. 正反向黑盒测试（完成标准）

| 用例 | 期望 |
|---|---|
| `tool install` 本地清单 `benchmark` 后调用 `marqdo benchmark run .` | 无需 CLI 源码改动，可原样传参并传播退出码 |
| 将工具名改成 `lintx`，重建外部程序但不重建 CLI | 能直接注册和调用 |
| 工具没有 `analysis/trace` 兼容字段 | 可以独立使用；不强制创建分析或运行记录 |
| 工具声明不支持 Analysis v1 或核心版本 | 在需要该协议时拒绝，不回退猜测 |
| 工具注册名冲突 `run` 或另一已安装工具 | 拒绝，不覆盖 |
| PATH 中存在同名 `marqdo-benchmark` 但未注册 | 未知命令错误，不执行 PATH 程序 |
| 篡改已安装二进制的一个字节 | 拒绝启动，指明摘要不匹配 |
| `entrypoint` 是 `../outside`、绝对路径或越界符号链接 | 安装失败，无残留注册 |
| 安装时被杀/写盘失败/并发安装 | 不留下半安装或不一致的注册 |
| `marqdo tool remove benchmark` | 注册与受控安装制品删除；其他工具正常 |
| `--help`、复杂 Unicode 参数和 stdin/stdout | 工具获得原样参数/流，核心不解释私有业务语义 |

验收同时要求 `marqdo-cli` **不包含** `view/knowledge/prove/benchmark` 的特殊分发匹配、下载 URL、业务参数类型及平台分支。把测试工具迁移至新的仓库，并在不同机器上单独编译安装后仍可通过本协议。

## 8. 参考

- Cargo External Tools: https://doc.rust-lang.org/cargo/reference/external-tools.html —— 可扩展子命令而无需修改核心。
- kubectl plugins: https://kubernetes.io/docs/tasks/extend-kubectl/kubectl-plugins/ —— 独立可执行文件、自动分发的经验；其默认 PATH 信任模型不直接沿用。
