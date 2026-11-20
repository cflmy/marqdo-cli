# Marqdo CLI：CI 收敛、语言边界回归与稳定发行实施规范

> **文档性质：** 可直接入库的执行规范 / Release Convergence Plan。供未阅读任何历史聊天的维护者、贡献者与 DeepSeek Harness 使用。
>
> **仓库：** `https://github.com/cflmy/marqdo-cli`；开发者的本地 Git 远端可能是 Gitee，不应混淆远端提交、GitHub 镜像与真实 CI 状态。
>
> **事实基线：** 2026-11-20，GitHub `master` `c9f01aa8fb127a2dcc125103ef794832984774ad`。最新公开 CI：[run 44692692318](https://github.com/cflmy/marqdo-cli/actions/runs/44692692318)。工作区当前版本 `0.1.0`；检索时 GitHub Releases 为空。**这不是已发布稳定版。**
>
> **实施原则：** 以仓库的 `spec/` 和已接受 ADR 为语义权威，以真实 CI/本地运行日志为实现证据。此文档只定义剩余收尾工作和验收顺序，不重开已解决的架构议题，不授予自动发布权限。后续每次变更先更新 `docs/STATUS.md` 事实基线。若阅读时 HEAD 已变化，重新取得该 SHA 的 CI 证据，并在本规范记录差异，不能机械照搬旧故障。

> **执行更新（2026-11-20，本地收敛）：** 远程 `master` 仍为 `c9f01aa8`，本机新增收敛提交 `d05f36f` → `f075c96`，但**未推送**（本检出只有 Gitee 远端且无 GitHub 凭据）。`f075c96` 上：Linux 本地 `fmt`/`clippy`/`cargo test --workspace --locked` PASS；专用 Ubuntu 26.04 VM 普通用户 `dev` 从 `git archive` 重跑 G-C lane **7/7 + 正向 CLI PASS**（见 `docs/evidence/2026-11-20-strict-gc-f075c96.md`）；Linux RC 已打包并通过外部干净安装与 checksum 校验。Windows/macOS 与同 SHA 的 GitHub CI、G-C 自托管 CI URL 仍为 **UNVERIFIED**，因此总体决定仍是 **DO NOT RELEASE**。下一动作是把 `f075c96` 或仅补文档的后继提交推送到 GitHub 并重跑全平台 CI 与安全 lane。

---

## 0. 一页决策摘要

目前 **不是重新开发 Marqdo CLI** 的阶段。Rust 语言实现、Markdown profile、表达式解析、lock v3、Analysis v2、Trace v2、Tool v1、Foreign 固定环境、Linux strict L1 已有实现；Linux strict L1 曾在普通用户运行的专用 Ubuntu VM 完成 **7/7** 安全测试，但其证据对应提交 `b6fc182`，**尚无同一发布 SHA 的远程安全 CI URL**。

最新 `c9f01aa8` 的 GitHub CI 状态：

| CI job | 观察到的结果 | 具体证据 | 发布影响 |
|---|---|---|---|
| `fmt + clippy + test (Linux x86_64)` | **FAIL** | `strict_mode_enforces_isolation_or_refuses_closed`: Bubblewrap 启动返回 `Resource temporarily unavailable`，其余已执行的 17 项 CLI 集成测试通过 | **阻断 G-B** |
| `runtime contract (macos-latest)` | **FAIL** | `marqdo-sandbox/src/lib.rs:115` 对 `linux::probe()` 的非 Linux 引用导致 `E0433` | **阻断 macOS 运行契约** |
| `runtime contract (windows-latest)` | **FAIL** | 同样 `E0433` | **阻断 Windows 运行契约** |
| `schema instance validation + consumers` | **PASS** | 真实 Analysis/Trace 实例、Tool manifest 的 Schema 验证及消费者通过 | 公共协议该 job 有有效证据 |
| `release candidate artifact` | **SKIPPED** | 上游 CI 不通过；且该 job 仅按 tag/手动触发运行 | 尚无合格发行产物证据 |
| Linux strict G-C 独立安全 lane | **无同 SHA 的 CI 记录** | 本地 VM 7/7 的历史证据在 `docs/evidence/2026-11-20-strict-gc.md` | 若发行宣称 Linux strict L1，**必须补同 SHA 安全 CI** |

**不要将“CI 通过 1/4”解释为“功能完成 25%”。** 四个工作流 job 是离散的质量门禁，不是均匀的功能权重；代码已经较完整，但当前可发布判定为 **DO NOT RELEASE**。也不要猜测“几天内发版”；只有逐项 PASS 才能发布。

### 0.1 实施范围与拒绝范围

本轮必须：修复三平台 CI，补充 Markdown–表达式跨层金样与语义测试，证明核心现有包/协议/Tool/ABI/Foreign 的支持范围，补充独立扩展消费者，完成发行候选与安全证据，更新文档和版本。

本轮不应引入：新的顶层命令、第二套 Markdown 解析器、改写 `**` / `` `...` `` 的正式职责、知识编译器、Lean 证明器、View 网页、自动 uv/npm 生态管理、通用容器平台、JIT、公共包注册表。除非**现有承诺的稳定路径存在阻断级缺陷**，否则这些是独立项目或后续版本工作。

## 1. 项目架构及必须保持的边界

### 1.1 目标与计算路径

Marqdo 是独立 Rust 实现的 Markdown 原生语言，**不是**把 Markdown 翻译成 Python 运行。程序计算过程是：

```text
.mq.md UTF-8 source
    │
    ▼
marqdo-markdown：固定 Comrak/CommonMark/GFM/Math profile（唯一 Markdown 解析）
    │ Canonical MdNode + byte Span
    ▼
marqdo-frontend：Heading 作用域/导入/参数推断/Markdown AST→IR
    │                条件头、分支、循环、调用、引用、源码位置
    ▼
marqdo-ir + marqdo-expression：可执行 AST/强类型表达式语法
    │
    ▼
marqdo-runtime：Rust 原生树遍历执行 + 数值/控制流
    │
    ▼
marqdo-host / marqdo-sandbox：能力授权、预算、可选外部 Kernel/ABI
    │
    └── 通过 marqdo CLI 输出结果、诊断和受控 Trace
```

扩展架构保持正交：

```text
marqdo tool install <通用第三方 Tool bundle>
      │
      └── marqdo <工具名> ... 由外部进程承担，不在核心硬编码工具名单

公共消费协议：Analysis v2 / Trace v2（由 check/run 产生，Tool 可按需消费）
运行时扩展：Plugin ABI v1（独立于外部 CLI Tool）
Foreign：显式 Kernel Provider 与已固定环境（独立于 Tool 和 ABI）
```

**不准**为了测试方便把第三方工具、数学扩展或安全子系统直接塞进解释器的 `match` 链；**不准**将 Markdown 解析换成基于正则的近似语法；**不准**让 `check` 执行副作用。

### 1.2 规范、状态与历史的权威顺序

- `spec/markdown-profile.md`、`spec/mapping.md`、`spec/expression.ebnf`、`spec/evaluation.md`：语言语义。
- `spec/package-format.md`、`spec/analysis-v2.md`、`spec/trace-v2.md`、`spec/tool-v1.md`、`spec/plugin-abi.md`、`spec/foreign.md`、`spec/security.md`：工程契约。
- ADR-0016～0021：lock v3、Worker/Host、Foreign 环境绑定、Linux strict L1 的有效决策。
- `AGENTS.md`、`docs/STABLE_RELEASE_IMPLEMENTATION.md`、`docs/contributor-guide.md`：既有开发和稳定化规程；**本文细化其当前 CI 收尾工作，不推翻其范围约束**。
- `docs/STATUS.md`、`COMPATIBILITY.md`、`docs/security-gate.md`：必须逐 SHA 更新事实证据。旧状态、旧本地日志、旧 CI 不能自动转成新 SHA 的 PASS。

### 1.3 版本与安全概念

- **G-A**：核心运行与语义可用（开发阶段）。
- **G-B / Core Stable**：承诺的语言/工程功能在所有支持平台的 CI 通过；契约、工具与发行产物可复现。
- **G-C / Linux strict L1**：独立安全 lane 在普通用户、真实 OS namespace 环境中通过逃逸矩阵，并记录同一 SHA 的 CI URL。L1 仅含纯 Marqdo、console 和原生 Math；Foreign、Worker、Native ABI 等 **L2 仍不支持**，必须拒绝。
- 稳定版本号可以仍为预 1.0 版本；“stable”指受支持功能通过公布的验收门槛，**不等于**所有未来设想均已完成。

## 2. 当前失败的精确根因与首轮修复

### P0-1：Windows / macOS 的 `E0433` 条件编译错误

**已观察到：** 现有 `crates/marqdo-sandbox/src/lib.rs` 中的 `linux` 模块由 `#[cfg(target_os = "linux")]` 条件编译，但 `PlatformCapabilities::detect()` 使用：

```rust
let strict = if cfg!(target_os = "linux") {
    match linux::probe() { /* ... */ }
} else { /* ... */ };
```

`cfg!(...)` 产生布尔常量，**不移除代码分支的类型检查**。macOS/Windows 仍需解析 `linux::probe()`，因而报 `E0433`。这是确定的源代码缺陷，而不是 CI 环境错误。

**最小正确修复：** 把平台专用操作分配给带 `#[cfg]` 的实现函数/模块：

```rust
#[cfg(target_os = "linux")]
fn detect_strict() -> StrictIsolation {
    match linux::probe() {
        Ok(mechanism) => StrictIsolation::Available { mechanism },
        Err(reason) => StrictIsolation::Unavailable { reason },
    }
}

#[cfg(not(target_os = "linux"))]
fn detect_strict() -> StrictIsolation {
    StrictIsolation::Unavailable {
        reason: format!("no verified strict OS adapter for {}", std::env::consts::OS),
    }
}
```

然后 `PlatformCapabilities::detect()` 只调用 `detect_strict()`。这段是**示意修改**，开发者必须依仓库现有 API 实际实现，并消除非 Linux 专属的 `libc` 依赖警告。不要将 `mod linux;` 变成无条件导入，那会使 Linux 特有依赖污染其他平台。

**必须添加的验证：**

1. Ubuntu 本地 `cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`。
2. Windows/macOS **真实本机 CI** `cargo test --workspace`，不能仅 `cargo check`。
3. 在非 Linux 上 `marqdo inspect --platform --json` 报 `strict=unavailable`；`marqdo run ... --mode strict` 必须拒绝，未产生程序副作用。
4. 再次运行 plugin host-target fixture；确认先前的 `.dylib`/CRLF 问题没有在恢复编译后重新显现。

**完成定义：** 两平台的 `E0433` 消失；各自完整测试 job 绿灯；严格模式仍 fail-closed；没有更改公开语义。

### P0-2：Linux CI 的 strict probe 与运行时 `EAGAIN`

**已观察到：** `crates/cli/tests/integration_cli.rs::strict_mode_enforces_isolation_or_refuses_closed` 首先通过 `inspect --platform` 判断 strict 是否 available；如果 available，它就要求另一进程 `run --mode strict` 必须退出 0。最新 CI stderr：

```text
bwrap: Creating new namespace failed: Resource temporarily unavailable
```

因此日志支持的是**运行时受到瞬时或配额类环境限制**，尚不能证明具体是 `user.max_user_namespaces`、用户进程限制、宿主工作负载、Runner 策略中的哪一项；更不能断言 Linux strict L1 安全边界失效。

当前 `crates/marqdo-sandbox/src/linux.rs` `probe()` 会运行 Bubblewrap 烟测；`run()` 内部又调用 `probe()`，之后才真正启动带资源限制和项目挂载的监督进程。探测成功不代表后续启动一定成功，两次启动之间的系统资源与权限可改变；这属于典型 TOCTOU / 资源可用性问题。

**不允许的“修复”：** 删除测试、在 strict 失败时降级为 project、把 `EAGAIN` 误标为成功执行、在普通 CI 下断言所有 Linux 主机支持 strict、用 sudo/root 运行测试代替普通用户验证。

**推荐分离两项不同契约：**

1. **普通 CI：安全失败契约。** 在任意 CI 机器上都能运行。若本次实际隔离启动成功，则程序输出必须正确；若由于真实系统资源或权限不足启动失败，则必须退出非零、给出明确诊断、**项目代码从未在不受限环境执行**，不得隐式降级。`inspect` 仅是当时环境快照，不能拿它当“今后一定成功”的永久承诺。
2. **专用 G-C 设备：正向安全契约。** `MARQDO_REQUIRE_STRICT_SECURITY=1`、普通用户、可重建的 Ubuntu VM/self-hosted runner。此处 **不允许 skip/允许拒绝**；必须成功完成 run/call 和全部 7 项对抗测试，否则 G-C FAIL。

**实现建议：**

- 在受控测试中注入可预测的 `probe succeeds → launch returns EAGAIN` 情况（通过隔离的 adapter abstraction/test seam，而非改写全局 PATH 造成工具替换风险），断言 `LaunchFailed/StrictUnavailable` 被诊断且没有无沙箱执行。
- 取消普通 CI 的“第一次 probe 为 available，第二次执行就必须成功”的不可靠断言；仍要验证成功路径和失败路径，而不是简单忽略失败。
- 如果要减少多次 probe，只在确保**实际启动**使用同等/更强参数且执行时失败仍 fail-closed 的前提下实施；不要用纯文件存在或 bwrap `--version` 代替真实 probe。
- 记录 `EAGAIN` 时使用的 uid、namespace/sysctl、RLIMIT、bwrap 版本和进程负载作为诊断信息；不得将这类失败归结为不存在的语义错误。
- bounded retry 只能作为可选可靠性工程，不能吞没最终错误、不能绕过安全约束；默认先保证 fail-closed。
- 将错误码和机器可读错误分类稳定化（安全不可用、启动失败、超时/资源耗尽），不要把下游 bwrap stderr 当作任意调用成功。

**完成定义：** 普通 Linux CI 与受控 G-C 安全 lane 同时成立；在宿主不可用时 100% fail-closed；在专用 VM 中 7/7 且 run/call 正常。保存当前目标 SHA 的两条 URL。

### P0-3：修复后必须重新确认 CI，不得“预判绿灯”

当前 macOS/Windows 在编译阶段中止，所以不能据此声称以前的 native plugin、AST CRLF、Tool、Foreign 以及跨模块测试全部通过。恢复编译后有可能暴露后续错误。

具体动作：

- 从同一 SHA 打开完整 CI run，逐 job 导出失败步骤、`thread ... panicked`、错误分类和目标平台。
- 不要只看 GitHub Workflow 总结；对每个失败 job 下载原始日志。
- 不论上一条失败多简单，修复后必须以**新的 HEAD CI** 为依据重新评估所有平台。
- 不对 macOS/Windows 的 Unix-only ABI/Worker 测试作“通过”宣称；它们若没有运行，应在 `COMPATIBILITY.md` 明确 UNSUPPORTED，而不是掩饰为 PASS。

## 3. Markdown 与乘方：从疑虑收缩为正式回归测试

### 3.1 已确定的语义，不得擅自改变

Markdown 源码经过冻结 Comrak profile。`##` 定义函数；普通函数段落中的 `Strong` 是可执行语句；`Emph` 是返回表达式；有序列表是分支，无序列表是循环；列表项第一段的**首个 CodeSpan** 是条件表达式；后续普通文字可解释目的；BlockQuote 是显式调用。完整计算表达式由独立 Marqdo 表达式解析器解释。

**用户本地实测**已经说明：适当空格使 Markdown 按预期生成 AST，乘方可以正确运行。故这一轮**不应以“修复乘方”为由重新设计强/斜体语法**。Markdown 标记本身具有分隔符词法规则：紧凑写法造成不同 AST 属于需要准确文档化的语言边界，而不是一律归咎解释器。

```markdown
## main

**n = 2**

**`n = n ** 2`**

> print(n)
```

以上是建议的、对 `**` 乘方运算最稳妥的可读写法：外围 Strong 定义执行语句，内部 CodeSpan 避免 Markdown 把计算运算符看作强调分隔符。对于本机已经验证能形成正确 AST 的空格写法，也应以 AST 金样记录，不应禁止。

**正式判断标准：** `marqdo inspect --ast` 的 Comrak AST 是否对应规范结构；其后 `check`/`run` 是否按该结构执行。不得为“猜测用户想写什么”而增加原始 Markdown 正则修复器，也不得因源码像表达式就跳过 Markdown AST。

### 3.2 乘方在各层的当前证据

- `marqdo-expression/src/lexer.rs`：`TokenKind::StarStar`。
- `marqdo-expression/src/parser.rs`：`parse_power()`；测试 `power_is_right_associative_and_binds_tighter_than_unary_minus`。
- `marqdo-runtime/src/value_ops.rs`：`BinaryOp::Pow`；整数精确幂、负整数指数类型错误、指数最大 100000、估算结果位数上限 1000000；浮点由 `powf()` 处理。
- Runtime 层测试 `text_concatenation_and_power` 覆盖 `2 ** 10 = 1024` 与负指数拒绝。
- 当前 36 组 `tests/conformance/semantics/` 的 `.mq.md` 金样未单独测试 **Marqdo 原生** `**`；`36-foreign-call-python.mq.md` 的 `r ** 2` 属于 Python Foreign，不等价于 Marqdo 乘方。

### 3.3 新增的正向语义 fixtures（至少）

遵循 `crates/cli/tests/conformance_semantics.rs` 已有的 `.mq.md` + `.expected.json` 机制。不要用 `UPDATE_SEMANTICS_GOLDENS=1` 把错误期望直接覆盖；先手工计算期望。

| 编号建议 | 源码表达 | 预期 | 证明什么 |
|---|---|---|---|
| 37 |  `*` + CodeSpan(`2 ** 3 ** 2`) + `*` | `Int 512` | Markdown→右结合解析→求值 |
| 38 | `*` + CodeSpan(`-2 ** 2`) + `*` | `Int -4` | 一元负号优先级 |
| 39 | `*` + CodeSpan(`(-2) ** 2`) + `*` | `Int 4` | 括号与优先级 |
| 40 | `*` + CodeSpan(`2.0 ** -1`) + `*` | `Float 0.5` | 浮点负指数 |
| 41 | `**` + CodeSpan(`n = 2 ** 3`) + `**`，然后返回 `n` | `Int 8` | Strong + CodeSpan 嵌套路径 |
| 42 | 有序列表条件 `` `n ** 2 == 4` `` + 叙述 + 引用调用 | 只调用真分支 | 条件 Markdown / 段落 / 控制流 |
| 43 | 无序列表条件 `` `n ** 2 < limit` `` + 叙述 + 递增 | 预期迭代和返回 | 循环、条件重新求值、乘方 |

例如，给 42 建立下述源文件；相应 expected 的 `stdout` 应为 `乘方正确\n`，返回值应为 `Unit`，实际 JSON 具体类型按已存在 wire-v1 金样约定构造：

```markdown
## main

**n = 2**

1. `n ** 2 == 4`，正确分支的自然语言说明。

   > print("乘方正确")

2. `n ** 2 != 4`，另一分支的说明。

   > print("乘方错误")
```

对 41：

```markdown
## main

**`n = 2 ** 3`**

*n*
```

### 3.4 必须新增的负向/边界测试

| 输入或场景 | 必须验证的行为 | 测试层 |
|---|---|---|
| `2 ** -1`，纯 Int | 类型错误；不是神奇地得到 `0.5` | Runtime error fixture |
| `2 ** 100001` | `resource.exhausted`（当前指数限额） | Runtime error fixture |
| `2 **` | 解析器诊断非法表达式，正确源码 span | 负向 fixture |
| 列表头 `` `n ** 2 ==` `` | 条件表达式语法错误，定位到原 Markdown 源码中缺失操作数的位置 | Frontend / CLI error |
| 多反引号 CodeSpan 包含 `**` | 仍能正确映射完整表达式与字节 Span | AST + frontend |
| `**n=n**2**` 等缺少空格的 Markdown 边界 | 固定 Comrak 真实 AST，不推断用户意图，不自动更正 | AST golden + 文档例子 |
| `**n = n ** 2**`（有空格） | 在实际 profile 中确认 AST，再据此固定接受/拒绝结果 | AST + CLI |
| 普通叙述段落出现 `` `n ** 2` `` | 不执行计算、不被作为分支条件 | 结构/副作用性质测试 |
| 代码围栏中出现 `**` | 保持惰性，不会触发运算 | AST + inert fixture |
| 纯说明段落的修改 | 可执行 IR/实际结果不变 | property test |
| 强行嵌套 Markdown 强调导致非法 payload | 给出诊断，不静默换另一计算规则 | negative test |

**条件头被吞掉的可执行节点：** 当前 `map.rs::condition_of()` 使用 `&item.children[1..]` 形成分支体，首段里第一个 CodeSpan 后若出现 `Strong` / `Emph`，这些可执行标记可能被整体忽略。建议在原有“首个 CodeSpan + 说明文本”语法上增加一个严格检查：首段剩余结构仅允许惰性叙述（按已冻结 profile 判定），保留其原始 AST 与 SourceSpan；若出现其他可执行角色，报 `markdown.invalid_construct`，不能无声丢弃。先补复现 fixture，再制定必要的精确规则；**无需更换 CodeSpan 条件头的正式语法**。

**CodeSpan 源码位置：** `condition_of()` 当前使用 `PayloadSegments::single(&literal, condition_node.span)`，须回归验证反引号是否计入字节跨度；若诊断偏移，修正 span 映射而不是改 Markdown AST。要求单/双反引号、中文前缀、换行和多字节 UTF-8 的准确来源测试。

### 3.5 正确的测试组织与执行命令

```bash
cargo test -p marqdo-expression power_is_right_associative_and_binds_tighter_than_unary_minus
cargo test -p marqdo-runtime text_concatenation_and_power
cargo test -p marqdo-cli --test conformance_ast
cargo test -p marqdo-cli --test conformance_semantics
cargo test -p marqdo-cli --test negative
cargo test -p marqdo-cli --test properties
```

逐个检查 `*.expected.json` 的类型标签、stdout、退出码和诊断分类；新增 AST goldens 先独立检查真实树，再写语义期望，不允许一键刷新旧金样掩盖回归。CI 应直接运行上述测试（或被 `cargo test --workspace` 包含），并在单独的 job step 报告 fixture 覆盖，使 Markdown 语义边界成为可发现的门禁。

## 4. 第二轮：核心生态、协议与跨平台验证（不能以 Linux 绿灯替代）

### P1-1：lock v3、环境与可复现性

既有 `marqdo.lock v3`、传递包实例图、Git cache、`sync --locked` 和环境原子物化需要继续验证，**不需要重写包管理器**。

- `cargo test -p marqdo-cli --test package_env`；增加/核实传递依赖固定、多版本别名、路径/Git 内容篡改、锁文件缺失/过时、`run`/`check` 不下载不改锁、原子安装失败可回滚。
- Windows 与 macOS 的实际原生测试必须覆盖承诺支持的路径与 Git 功能；Unix-only 设计要作为 UNSUPPORTED 写清。
- 校验包内容哈希策略对无关构建目录与特殊文件的一致处理，不能因 checkout 位置变化造成无端哈希漂移。

### P1-2：Analysis v2 / Trace v2

- 运行 `cargo test -p marqdo-cli --test analysis_trace`，及 `tests/consumers/validate_protocols.py`、`analysis_consumer.py`、`trace_consumer.py` 的真实实例验证。
- 验证跨项目移动后的逻辑身份、资源定位、SourceSpan、跨模块追踪、Trace 与 Analysis 字节摘要绑定、错误/截断/过期证明；不得把缺少某字段的 JSON 视为 Schema 检查成功。
- 新增乘方/控制流 fixture 后，证明 Analysis 的语义投影一致，Trace 只记录实际命中分支；阅读工具不执行源码。

### P1-3：通用 Tool Protocol v1 的独立消费者

目前已实现未知名字 `benchmark` 的 Tool fixture，尚缺用于验证三类真实消费者的独立样例。新增**三个小型、彼此独立、可跨平台运行**的模拟 Tool：`sample-view`、`sample-knowledge`、`sample-proof`。名字仅是测试 fixture，不是 CLI 硬编码白名单，也不是在本仓库实现实际 View/Knowledge/Proof。

各消费者从公共 `Analysis v2` 或 `Trace v2` 中读取极小字段、产生确定输出；按 Tool v1 manifest 安装、调用和卸载。至少验证：

- 安装 `marqdo tool install <manifest>` 后 `marqdo <自定义名称>` 正确分发；Tool 不在 CLI 源码内特殊注册。
- 传递原样 argv/cwd/stdin/stdout/stderr 与子进程退出码；打断、卸载、恶意 bundle 路径与 whole-bundle digest 篡改严格拒绝。
- Analysis/Trace 兼容版本不匹配时在调用前给出清晰诊断。
- CI 使用临时 `MARQDO_TOOL_HOME`；不触碰个人工具目录，不借由用户 PATH 自动运行未注册程序。

### P1-4：ABI/Worker/Foreign 的真实支持矩阵

- Linux/macOS Native ABI：使用各自主机目标构建 C fixture；成功调用、多导出、ABI mismatch、损坏 bundle、Worker 超时/崩溃。
- Windows：如当前没有承诺 Windows Native/Worker，明确 UNSUPPORTED；不得通过“测试跳过”声称可用。仍应验证 Windows 的语言/Tool/包/分析契约。
- Host Broker：只宣称已有明确实现的 `fs.read` 等能力；其他 `fs.write`、`net.request` 等返回 `capability.unavailable` 不算正常功能支持。
- Foreign：验证真正启动的可执行文件来自已 pin 的环境，环境内容 digest 与版本漂移检查，非法 wire、stdout/stderr 限额、超时；未实现 uv/npm/venv 自动生成，应在支持矩阵写清，**不作为稳定基础 CLI 的必需特性**。
- `check` 不执行 Foreign、Plugin 或用户程序；恶意输入下也必须保持静态。

## 5. G-C 专用安全验证与准入

### 5.1 已完成的事实与不可推断部分

ADR-0021 已实现 Bubblewrap supervisor，Linux strict L1 在专用 Ubuntu 26.04 VM、普通用户 `dev` 下通过 7 个对抗测试，包括身份/权限、主机读写、网络、进程与资源。日志位于 `docs/evidence/2026-11-20-strict-gc.log`，对应执行的代码提交为 `b6fc182`。

**不能据此推断：** 任意 Ubuntu 都支持 strict；所有 Linux 版本均通过；macOS/Windows 已具备 strict；Worker/Foreign/Native 插件能在 strict 中运行；已经获得 G-C 远程 CI 认证。

### 5.2 同 SHA 的安全 CI

- `.github/workflows/security-linux.yml` 已设定 `[self-hosted, linux, marqdo-security]`，必须由**专用、可销毁 VM** 执行。
- runner 主机需实际支持非 root Bubblewrap user/mount/PID/net namespace、正确 AppArmor profile；不可通过全局关闭保护或 sudo 模拟非特权用户。
- `MARQDO_REQUIRE_STRICT_SECURITY=1`：runner 缺机制、跳过测试、7 项中任一失败均视为 FAIL。
- 记录完整 run URL、被测 SHA、VM image/kernel、bwrap 版本、uid、源码与 artifact 摘要、7/7 结果；只有同 SHA 证据可作为正式发行审核依据。
- 严格证明“源码从未在降级模式运行”；启动/挂载/namespace 失败时退出非零并且没有副作用。
- L2（Worker/Foreign）明确拒绝；不应为赶发版扩展 strict 范围，除非另立 ADR 与攻击测试。

### 5.3 可选发行范围决策

**方案 A（推荐）：** 发行声称 Linux strict L1，前提是同 SHA 专用安全 CI PASS。该方案最符合当前 `STATUS.md` 的发布门槛。

**方案 B（仅当项目负责人明确改变发行范围）：** 发布仅 G-B 的普通 CLI，正式关闭/不宣称 strict L1，文档、功能开关、支持矩阵和测试必须一致，确保用户不会误以为安全运行已认证。不能一边保留可用的 L1 声明一边省略 G-C 证据。

## 6. 发行产物与工作流纠偏

当前 `release-candidate` job 依赖 Linux、macOS、Windows 核心 CI；在前置失败时被 skip 合理。它只对 release tag 或 `workflow_dispatch` 执行；`docs/releasing.md` 要求源代码与 CI 的 SHA 一致、干净环境安装、checksum、spec/examples、许可证、兼容矩阵与回滚说明。

### 6.1 发版前的工作流检查

- 修复 P0-1、P0-2 并触发**同一 SHA** 的全平台 CI，所有 jobs 必须 PASS。
- 单独跑 G-C 真实 VM（若宣称 Linux strict L1）；记录 URL 与环境。
- 校验 Release job 输出的 OS/arch/target 与 `rustc -vV` 一致，不可以把实际 host 二进制错误标注成 `aarch64-apple-darwin`。Windows 未配置发行包就不要宣称提供 Windows 发行资产。
- Windows/macOS/Linux 原生 smoke（当前支持的功能）必须发生在提取后的干净目录中，不能只在源代码树里 `cargo run`。
- `git describe --tags`、`Cargo.toml` 版本、二进制 `--version`、归档名、`CHANGELOG.md`、compatibility、Release Notes 保持一致；拒绝不同 SHA 的证据拼接。
- 发布前先在 RC 上执行 `sha256sum -c`（macOS 对应 `shasum -a 256`），再安装、运行、通过 Python consumers 验证 Analysis/Trace。
- 发版 tag/release 是不可逆的对外动作，需项目负责人显式批准；Harness 只可预备 RC 和证据，不能因完成文档而自行发 tag。

### 6.2 CI 环境可靠性要求

- 固定必要工具版本/lock，所有主机对同份 `.mq.md` 拥有一致字节行为（`.gitattributes` 与 CRLF 金样）。
- 影响平台的改动强制在受支持的平台编译和完整测试；仅 Ubuntu 本地通过不能合并发布。
- 对 GitHub-hosted runner 资源不稳定场景采取安全失败检测，不能把所有 `strict` 失败一概当可用；对应专用安全 lane 必须跑真成功路径。
- 避免把工具链安装、网络下载、外部 Python/Node 可用性放进与语言语义直接耦合的金样，从而避免无关环境波动把语言回归误判。

## 7. 严格的发布门禁清单（唯一签署表）

每项只允许 `PASS(证据URL/日志/SHA)`、`FAIL(原因)`、`UNVERIFIED(待执行)`、`EXPLICITLY UNSUPPORTED(范围说明)`。**核心必需项不允许标成 UNSUPPORTED；可选能力明确不发布时才可以。**

| 门禁 | 当前 c9f01aa8 | 证据/下一动作 |
|---|---|---|
| G0 同 SHA、版本与变更集明确 | PASS（基线记录） | SHA c9f01aa8；0.1.0；无 release |
| G1 Linux fmt/clippy/workspace/smoke | FAIL | strict CI false assumption/EAGAIN |
| G2 Windows workspace + portable smoke | FAIL | cfg! 下 linux::probe E0433 |
| G3 macOS workspace + portable smoke | FAIL | 同 E0433 |
| G4 公共 Schema + 消费者 | PASS（当前 job） | 当前 GitHub CI schema job |
| G5 Markdown AST/乘方新增语义回归 | UNVERIFIED / 待新增 | 第 3 章 test matrix |
| G6 全套负向/源位置/惰性属性测试 | UNVERIFIED（同 SHA 完整 job 未绿） | 现有 `negative`/`properties` 加新增条件头测试 |
| G7 lock v3 与包环境受支持平台 | UNVERIFIED（跨平台未完成） | `package_env`、Git / lock 反例 |
| G8 Tool、ABI、Worker、Foreign 受支持矩阵 | PARTIAL | 新消费者 fixture，macOS Native 实测；Windows 明确范围 |
| G9 Linux strict L1 安全真实测试 | VM PASS（旧 SHA），同 SHA CI UNVERIFIED | self-hosted security lane |
| G10 RC 构建/校验和/跨平台干净安装 | UNVERIFIED | release-candidate job 尚未执行 |
| G11 文档与版本/许可证/兼容矩阵 | PARTIAL | `STATUS.md` 落后最新 CI，需修订 |
| G12 负责人批准 tag + Release | NOT STARTED | 前面门禁 PASS 后审批 |

**当前总体发布状态：`DO NOT RELEASE`。** 不给虚假的百分比；以余下硬门禁的通过证据量化进度。建议每次提交后在 `STATUS.md` 维护上述表，至少填 commit SHA、各 CI URL 和失败测试。

### 7.1 关闭条件：最短可衡量路径

- **第一个工作包完成：** G1～G3 全绿（解决旧 CI 三处失败）；不再存在 E0433 / 环境依赖的错误断言。
- **第二个工作包完成：** G5/G6 全绿（乘方、Strong/CodeSpan、列表叙述、负向边界、源 Span）；语言语义不需重新设计。
- **第三个工作包完成：** G7/G8 全绿或清楚地标为非承诺子能力；独立 Tool 消费无硬编码。
- **第四个工作包完成：** G9 由专用 VM 自托管 CI 在同 SHA PASS，或经负责人明确决定仅发行 G-B 并真正关闭 G-C 声明。
- **第五个工作包完成：** G10/G11 PASS、Release Candidate 清洁安装，再经批准完成 G12。

## 8. 分阶段执行与 PR 编排（供 Harness 持续推进）

| PR/工作包 | 优先级 | 改动范围 | 先写失败测试 | 所需 CI | Done 定义 |
|---|---|---|---|---|---|
| `RC-01 cross-platform cfg` | P0 | `marqdo-sandbox` 平台检测与依赖 cfg | 非 Linux compile + strict refusal | Linux/Windows/macOS | 编译、测试、拒绝契约绿 |
| `RC-02 strict CI robustness` | P0 | `linux.rs`、`integration_cli.rs`、安全适配层 seam | probe-success/launch-EAGAIN | 普通 Linux + VM | 两种环境均有正确契约 |
| `RC-03 Markdown power fixtures` | P0/P1 | AST/conformance/negative/property/spec 示例 | 新乘方 goldens、错误 spans | 全平台 workspace tests | 结果与语法精确一致 |
| `RC-04 consumers & platform coverage` | P1 | Tool fixture、包/ABI/Foreign CI | 独立名称+恶意 bundle+ABI | Linux/macOS/Windows 支持矩阵 | 无硬编码，边界清楚 |
| `RC-05 G-C security CI` | P1（若宣称 L1 则必需） | self-hosted VM lane 与证据绑定 | 真逃逸负例，不允许 skip | 安全 runner | 同 SHA 7/7 PASS |
| `RC-06 artifact and release` | 发布门槛 | 版本、文档、工作流、checksums | 干净安装的 smoke | 全平台/安全/RC | 可签署发行记录 |

**PR 原则：** 每 PR 对应一个可以独立关闭的错误类，绝不把跨平台修复、控制流语义重定义、外部产品开发打包成大重构；在新 CI 上出现额外故障则新增最小复现并纳入同类工作包。每个 PR 的提交信息至少包含：`WHY / WHAT / TESTS / CI_URL / BREAKING_CHANGE / SECURITY_SCOPE`。

## 9. Ubuntu 开发工作流与 DeepSeek Harness 权限

日常执行以普通用户进行：

```bash
pwd
git status --short --branch
git remote -v
git rev-parse HEAD
git fetch --all --prune
rustc -vV
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
cargo build --release --locked -p marqdo-cli
```

**重要：** 本地 `origin` 可能是 Gitee，而 GitHub 才运行 CI。Push 到 Gitee 的输出只是 Gitee 更新证据，不能证明 GitHub 镜像/Actions 已经检出了同一 SHA。先对比 GitHub `master` SHA，再读取 Actions 结果，不能把两个远端混为一谈。

可在专用、可销毁 Ubuntu VM 中给予 Harness 有审计的 sudo，安装测试依赖与配置隔离环境；但**实际 G-C 安全用例必须以普通用户运行**。若当前机器不能创建 namespace，则开发普通语言/CI 工作包可继续，G-C 转到已验证 VM，不能通过关闭 AppArmor 全局规则来凑成功。

日常建议按以下顺序：

```bash
# 快速复现核心行为
cargo test -p marqdo-cli --test integration_cli
cargo test -p marqdo-cli --test conformance_ast
cargo test -p marqdo-cli --test conformance_semantics
cargo test -p marqdo-cli --test negative
cargo test -p marqdo-cli --test analysis_trace
cargo test -p marqdo-cli --test tool_protocol
cargo test -p marqdo-cli --test package_env

# 安全专用测试：仅在能实际提供 OS namespace 的 VM 中强制执行
MARQDO_REQUIRE_STRICT_SECURITY=1 ./tests/security/run-lane.sh
```

每修完一个问题应先本地跑最小用例，再跑 workspace，再等待远端支持平台 CI；只有被 CI 验证的 SHA 才能进入下一门禁。不要跳过失败测试，也不要把 `cargo check` 的通过当作 `cargo test` 的通过。

## 10. 发行验收报告模板

在仓库中维护 `docs/evidence/RELEASE_CANDIDATE_<version>_<shortsha>.md`（文件名在创建时替换），填入下面模板，并将 CI 链接明确绑定 SHA：

```text
Marqdo CLI release candidate
Source SHA:
Workspace version:
Intended release target/platforms:
Linux fmt/clippy/workspace/smoke URL / PASS?:
Windows native runtime tests URL / PASS?:
macOS native runtime tests URL / PASS?:
Schema + independent consumers URL / PASS?:
Markdown/power AST, semantic, negative fixture results:
Package/Tool/ABI/Foreign supported-matrix results:
Linux strict L1 claimed? (yes/no):
If yes, G-C self-hosted security URL + 7/7 evidence SHA:
Unsupported optional features:
Release archive names, targets, SHA-256:
Clean-install smoke (external directory) outcome:
Changelog / COMPATIBILITY / STATUS links and version:
Known limitations and rollback approach:
Gate summary G0..G12:
FINAL DECISION: RELEASE / DO NOT RELEASE
Approved by / time:
```

必须通过：`G1~G8`、`G10`、`G11`，以及如果承诺 strict L1 则 `G9`；`G12` 是最后的人工批准。报告附全部原始日志或 URL。不得将未运行的 Windows/Worker/Foreign 自动环境功能写成已通过。

## 11. 可直接发送给 DeepSeek Harness 的完整任务指令

> 下面是执行指令，不是已完成工作的声明。用户允许在专用 Ubuntu 开发 VM 中按需 sudo，不代表允许绕过独立安全验证或未经许可发布 tag。

```text
你现在负责 cflmy/marqdo-cli 的稳定版发版收尾。工作以
`docs/NEXT_RELEASE_CI_CONVERGENCE.md` 为唯一当前 CI 收敛计划；
语言规范以 spec/、已接受 ADR、AGENTS.md 为准。

先获取最新的本地 HEAD、Gitee 和 GitHub master SHA、最近一次 GitHub CI。
若仓库已有新提交，必须重新定位失败，不许盲目修复文档中基线 SHA 的旧故障。

目标不是扩写 CLI，而是在不改变冻结语言语义的前提下完成：
(1) Windows/macOS 的 Linux-only cfg 编译修复；
(2) Ubuntu 普通 CI 下 strict probe 与实际启动竞态/资源不足的 fail-closed
    契约，并在独立安全 VM 保证严格隔离正向执行与 7/7 攻击矩阵；
(3) 增加完整 Markdown AST、乘方、分支/循环条件、源 Span、惰性与
    错误边界的黄金测试和端到端语义测试，不擅自改 CodeSpan 条件语法；
(4) 验证 lock v3、Analysis/Trace、完全通用的 Tool、ABI/Worker/Foreign
    的支持矩阵和独立消费者；
(5) 完成全平台 CI、同 SHA 的 G-C 证据、Release Candidate 干净安装、
    checksums、COMPATIBILITY、STATUS、CHANGELOG 与发版审批材料。

开发纪律：
- 修复前先写最小失败复现，修复后运行其正反测试与 workspace tests。
- 不删除/跳过失败测试；不改金样来掩盖未知变化；不降级安全模式。
- 不写死官方 Tool 名称，不引入第二解释器，不扩大顶层命令数。
- 对于未承诺的 Windows ABI、strict L2、uv/npm 自动生成环境，
  明确 UNSUPPORTED，不为了“全部完成”强行加入 CLI。
- 在专用 VM 内可为环境配置使用 sudo；G-C 执行者必须是普通用户。
- 每个 PR 提供源 SHA、问题根因、代码位置、运行测试、GitHub CI URL、
  对平台和安全承诺的影响。任何 CI 失败均视为门禁未通过。
- 最新仓库若出现额外阻断，增补最小再现测试及 STATUS 事实，继续
  将问题关闭到 release gate PASS；不要反复只提交开发计划。
- 版本 tag / 正式 Release 需要项目负责人明确审批，不可自动发布。

最终交付：G0..G12 门禁矩阵、完整运行日志/CI 链接、可信安全范围、
可安装和验证 checksum 的 RC 产物、完整变更说明、可复现发行流程。
如果任一必要门禁没有真实 PASS，明确给出 DO NOT RELEASE 和下一步动作。
```

---

## 12. 外部事实与代码审查索引

- 最新审查提交：`https://github.com/cflmy/marqdo-cli/commit/c9f01aa8fb127a2dcc125103ef794832984774ad`
- 最新 CI：`https://github.com/cflmy/marqdo-cli/actions/runs/44692692318`
- 工作区状态：`docs/STATUS.md`、`COMPATIBILITY.md`
- 安全机制与证据：`docs/adr/0021-linux-strict-isolation.md`、`docs/security-gate.md`、`docs/evidence/2026-11-20-strict-gc.md`
- CLI 测试：`crates/cli/tests/integration_cli.rs`、`conformance_ast.rs`、`conformance_semantics.rs`、`negative.rs`、`properties.rs`
- Markdown/表达式：`crates/marqdo-markdown/src/profile.rs`、`crates/marqdo-frontend/src/map.rs`、`crates/marqdo-expression/src/parser.rs`、`crates/marqdo-runtime/src/value_ops.rs`
- 发行流程：`.github/workflows/ci.yml`、`.github/workflows/security-linux.yml`、`docs/releasing.md`

**证据边界说明：** 这里的 GitHub CI 结论来自该次真实 Actions 运行与原始任务日志；本文件编写期间没有在作者环境重新运行 Cargo/OS 隔离测试，也没有修改 GitHub 仓库。新增代码与测试为实施要求，不应被读成已完成。后续维护者必须将所有 `UNVERIFIED` 转为含原始来源的结果，才能签署稳定版本。
