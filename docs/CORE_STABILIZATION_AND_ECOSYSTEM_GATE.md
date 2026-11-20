# Marqdo CLI 核心稳定化、协议冻结与独立生态准入实施规范

> **Historical review baseline (`0adebe4`, 2026-11-20).** The source review
> and CI facts here are a snapshot, not current state. The current overview is
> [`architecture/CURRENT.md`](architecture/CURRENT.md); executed status is in
> [`STATUS.md`](STATUS.md).


> **文档状态：Proposed / 可直接用作下一阶段工程实施基线**  
> **源码审查基线：** [`cflmy/marqdo-cli@0adebe42f14d72a2a275564d03ff9558f197766e`](https://github.com/cflmy/marqdo-cli/commit/0adebe42f14d72a2a275564d03ff9558f197766e)，`master`，2026-11-20。  
> **验证来源：** 对上述提交的 GitHub 源码、规范、测试、CI workflow 和 [CI run #44671025763](https://github.com/cflmy/marqdo-cli/actions/runs/44671025763) 日志的静态审阅；**未在本文写作环境重新本地编译、运行测试或验证 OS 沙箱**。  
> **仓库位置：** `docs/CORE_STABILIZATION_AND_ECOSYSTEM_GATE.md`。  
> **适用范围：** 只针对全新 Rust 仓库 `cflmy/marqdo-cli` 的后续开发。**不涉及从历史仓库 `cflmy/marqdo` 迁移代码，也不继承其 ABI 或隐式语义。**  
> **读者：** 第一次接触本项目的 Rust 开发者、测试开发者、插件作者、独立 Knowledge / Proof / View 工具作者及 AI 编程助手。

---

## 0. 执行摘要：先做什么，不做什么

当前 CLI 不是待重写的空壳。它已有 Markdown AST 驱动的原生 Rust 解释器、模块/函数、Math/Foreign 基础能力、C ABI v1、可运行的外部工具分发器，以及 Analysis v1 和 Trace v1。新阶段的正确任务是：**收敛已经形成的架构缺口，冻结能够被独立工具长期依赖的最小协议**。不要再设计大型 CLI 命令体系，更不要把知识编译、Lean 证明、Web View 放入本仓库。

本阶段执行三条主线：

1. **H0/H1 — 正确性与可复现性**：修复当前 CI，完成全依赖图锁定、包内容标准化、`sync` 一致性、跨模块错误定位。
2. **H2 — 外部协议可以真正被消费**：修正 Analysis 身份/资源投影，完成 Tool 版本协商、安装包完整性和跨仓黑盒消费测试；冻结公开 Analysis/Trace/Tool 的兼容规则。
3. **H3 — 有条件的可信执行**：补全 ABI Host Broker 的实际受控操作、Foreign 环境及权限边界；在至少一个 OS 上实现可验证的 strict 隔离，其他平台必须 fail closed。**H3 不阻塞 View/Knowledge/Proof 的早期独立开发，但阻塞“安全执行不可信代码”的宣传。**

**禁止新增顶层 CLI 命令。** 继续使用 `run`、`call`、`check`、`add`、`sync`、`tool` 作为推荐的主要入口，允许维护当前既有 `init`、`lock`、`remove`、`inspect`、`fmt`、`plugin inspect` 的兼容性，但不因本阶段需求扩大它们的产品边界。`marqdo knowledge`、`marqdo prove`、`marqdo view` 均由**独立仓库提供、通过 `marqdo tool` 注册**。

### 0.1 三道发布门槛，不能混为一谈

| 门槛 | 解锁事项 | 必须通过 | 不要求 |
| --- | --- | --- | --- |
| **G-A：外部工具原型可用** | 独立开发 View、Knowledge、Proof 原型 | 当前 CI 修复；Tool 分发黑盒测试；可读的 Analysis/Trace 基础消费；稳定错误码 | 完整 strict 沙箱、公共注册表、完整符号证明 |
| **G-B：Core Stable / 公共协议冻结** | 正式稳定依赖 CLI，生产可复现构建 | H0+H1+H2；跨平台协议测试；锁文件闭包；资源/模块身份；所有公测协议被 Schema 和外部消费者测试保护 | Lean、知识索引、CAS、JIT、全部平台 strict |
| **G-C：Strict Trust / 不可信执行** | 宣称支持某个平台上的不可信代码/原生插件受限执行 | 在相应平台上通过 OS 隔离、逃逸负例、资源限制和真实 Broker 测试 | 其他平台自动具备同等隔离能力 |

任何人声称“已经完成”，必须明确说的是 G-A、G-B 还是 G-C，且提供该提交上运行通过的 CI 地址和测试结果。

## 1. 项目定位、语言语义和仓库边界

### 1.1 Marqdo CLI 是什么

Marqdo 是以 Markdown 语法树为正式结构、具有确定执行语义的编程语言。其基础原则为“代码即文档，文档即知识库，知识即可执行”。它不是将 Markdown 翻译成 Python 后执行的工具。普通文档内容保持 Markdown 可读性，具有执行语义的结构由固定 Markdown profile 和 language mapping 解释，再由独立 Rust 解释器运行。

语言规范权威入口：`spec/markdown-profile.md`、`spec/mapping.md`、`spec/expression.ebnf`、`spec/evaluation.md`、`spec/references.md`。修改这些文件规定的语义属于**语言版本变更**，必须单独提出 ADR 和金样测试；本文件不会擅自改变它们。

语言核心应独立完成 Markdown AST 解析、函数和参数推断、表达式与控制流、模块/显式导入、静态检查、确定的值及错误语义、Rust 原生执行、能力检查、依赖环境。编译/运行 **不需要** Knowledge、Proof 或 View。

### 1.2 四个仓库如何合作

```text
                       用户终端
                           │
                           ▼
                        marqdo
                           │
              ┌────────────┴────────────┐
              ▼                         ▼
       内置语言/工程命令              Generic Tool Dispatcher
       run/call/check/add/...             │
              │                   tool.json + 已安装记录
              ▼                         │
    Markdown → Semantic → Rust Runtime    ├── marqdo knowledge ...
              │                         ├── marqdo prove ...
              ├── Analysis              ├── marqdo view ...
              └── Trace                 └── marqdo <任意第三方工具> ...
                    ▲                     ▲
                    └── 版本化文件协议 ────┘
```

- `marqdo-cli`：提供且仅提供语言权威、项目与执行环境、开放 CLI 工具发现分发、必要的公共分析/运行事实。
- `marqdo-knowledge`：独立仓库中的知识编译、索引、知识制品与交换；不得要求 CLI 内建知识图数据库。
- `marqdo-proof`：独立仓库中的契约、可证明语义子集、Lean 适配与证书；不得修改解释器以替代证明。
- `marqdo-view`：独立仓库中的 Markdown 阅读、追踪、导航与可视化；不得重新实现另一套语义解释器。
- 未被 CLI 预定义的第三方工具应与官方工具**完全等价接入**。官方工具名不是内置白名单。

### 1.3 三种“插件”必须严格区分

| 类别 | 做什么 | 接入方式 | 权限身份 |
| --- | --- | --- | --- |
| **CLI Tool** | 扩展 `marqdo xxx` 用户命令 | `tool.json` + 独立可执行程序，由 `marqdo tool install` 注册 | 用户主动启动的本地程序，不自动受 `.mq.md` 沙箱保护 |
| **语言运行时 ABI Plugin** | 给 `.mq.md` 增加可调用函数 | `plugin.yaml` + ABI v1（受信 Native 或 Worker） | 通过 Host、插件身份和能力授权约束；C ABI ≠ 沙箱 |
| **Foreign Kernel Provider** | 执行显式调用的外语 Code 值 | `kernel.json` + Foreign wire 协议 | 单独进程，`foreign` 授权 + OS 级边界，不是 CLI Tool |

不要为了复用文件名而合并三者注册表、安装命令或信任关系。

## 2. 当前仓库的真实基线与证据强度

### 2.1 代码已经存在的能力

| 领域 | 已在 HEAD 可定位的实现 | 当前结论 |
| --- | --- | --- |
| Markdown 解析/固定 profile | `crates/marqdo-markdown/`，AST goldens | 存在且已有测试；保持稳定 |
| 前端、签名和解释器 | `marqdo-expression`、`marqdo-frontend`、`marqdo-ir`、`marqdo-runtime` | Rust 原生执行路径已建立 |
| 通用可调用符号描述 | `crates/marqdo-ir/src/callable.rs`，ADR-0009 | 已解决此前多处手动注册的主要问题 |
| 包实例身份/解析 | `crates/marqdo-package/src/lib.rs`，ADR-0008 | path/Git、传递导入已出现，但锁定闭包未完成 |
| Tool 扩展 | `crates/cli/src/tool.rs`，`spec/tool-v1.md` | 本地任意工具名可分发，仍缺完整兼容协商与包完整性 |
| 公开 Analysis/Trace | `crates/cli/src/{analysis,trace}.rs`，`spec/{analysis,trace}-v1.md` | 已可导出；身份、资源、跨模块追踪仍有缺口 |
| ABI/Worker | `crates/marqdo-host/src/plugin_worker.rs`、`crates/marqdo-plugin-abi/` | 已有崩溃隔离；不是 strict OS 沙箱 |
| Foreign Provider | `crates/marqdo-foreign/src/lib.rs` | `KernelRegistry` 已支持外部 provider；环境管理未完成 |
| Host capability / handles | `crates/marqdo-host/src/{capability,handles}.rs` | 已有权限检查与 owner/run/generation 表；真正能力代理尚不完整 |

### 2.2 CI 的事实，不得“按文档标记 Done”替代验证

最新 HEAD 的 [CI run #44671025763](https://github.com/cflmy/marqdo-cli/actions/runs/44671025763) 为 **failure**：Linux 上 fmt 和 Clippy 成功，工作区测试失败于 `crates/cli/tests/properties.rs:328`：

```text
assertion failed: repo_root().join("tests").join("properties").is_dir()
```

原因是 Git 不跟踪空目录，仓库检出后的 `tests/properties/` 不存在。该失败**不是**已发现的语言语义不变性错误。Windows/macOS job 为 `cargo check` 成功，不能推论其运行测试通过。Linux 在测试失败后跳过了后续公共 Schema 和示例 smoke 验证。必须先修复这一基线才有资格说最新代码测试通过。

### 2.3 证据分级

- **源码确认**：本文件明确指出的函数或逻辑在基线提交中实际存在。
- **运行记录确认**：有可核对的 GitHub Actions 日志支持。
- **由代码推出的逻辑风险**：能够从条件分支和数据结构发现，但还应写复现测试。
- **设计建议**：推荐但尚未实现的方案，不得描述为现状。

每次后续更新文档时，先刷新 `HEAD SHA`、CI 证据，并为变更项记录实际测试，而不能机械继承本文件的状态判断。

## 3. 稳定内核的工程约束

1. **最小语义核心**。Markdown AST 映射、表达式、函数作用域、值类型和授权的最终裁决属于核心，不能让第三方 Tool 或任意 ABI 插件覆盖。
2. **不增加新的顶层命令**。新的功能优先通过原命令参数、现有 `tool` 协议或独立仓库实现；不要为了每个协议新增 `analyze`、`graph`、`env`、`kernel` 等新一级命令。
3. **无二次解析与双运行时**。Knowledge、Proof、View 只消费公开的标准分析/追踪或源码，不链接和序列化私有 Rust `marqdo-ir`，不重新决定何为执行节点。
4. **工具开放不代表语言可任意扩语法**。ABI/Kernel/Tool 的名称和实现由外部声明注册，但 Markdown 结构语义仍由核心规范控制。
5. **构建和检查不执行**。`check` 与 Analysis 导出不得运行文档、加载动态库或启动 Foreign；依赖安装/锁定不得执行安装脚本。
6. **来源声明不授予权限**。`marqdo.yaml`/`plugin.yaml`/`kernel.json` 的请求不是用户授权；实际能力在每次使用时受 Host 或 OS 限制。
7. **无隐式降级**。缺少 strict 隔离、缺少已锁版本、缺少契约支持时要用稳定错误码拒绝，不能悄悄恢复为不安全/不确定模式。
8. **允许破坏性内部重构，但对外协议必须显式版本化**。目前暂无正式稳定发布，推荐在首次 G-B 冻结前修正身份和锁文件格式；采用 ADR、迁移 fixture、Schema 版本和清楚的诊断，不要靠旧设计包袱维持错误。

## 4. 目标架构：改职责，不制造更多 crate

保留仓库现有 Rust workspace，以有限模块重构职责，而不是增加一串微型 crate。建议职责如下：

```text
crates/
  marqdo-markdown/         # CommonMark/GFM/Math profile, AST and SourceSpan
  marqdo-expression/       # expression lexer/parser/AST
  marqdo-frontend/         # language mapping, inference, static resolution
  marqdo-ir/               # private executable IR and CallableRegistry
  marqdo-runtime/          # independent Rust evaluator (no Python transpilation)
  marqdo-value/            # typed values, wire values, diagnostic/error
  marqdo-span/             # per-module source spans and locations
  marqdo-package/          # package graph, hashes, lock, env, cache
  marqdo-host/             # capability checks, effect calls, traces, workers
  marqdo-sandbox/          # per-platform isolation capability and enforcement
  marqdo-plugin-abi/       # stable C ABI, manifest, wire, worker framing
  marqdo-plugin-sdk/       # independent third-party Rust API
  marqdo-foreign/          # explicit kernel registry and process boundary
  marqdo-math/             # documented native mathematical subset
  cli/                     # command composition, project loading, public exports,
                           # Tool Protocol (no Knowledge/Proof/View implementations)
spec/
  package-format.md
  tool-v1.md
  analysis-v1.md             # if breaking fields: new v2 document + transition
  trace-v1.md
  schemas/
docs/
  adr/
  CORE_STABILIZATION_AND_ECOSYSTEM_GATE.md
crates/cli/tests/
tests/consumers/
```

### 4.1 关键依赖方向

- `cli -> package/frontend/runtime/host/foreign/plugin-abi`，只在 CLI 中装配；`package` 不调用 CLI；`runtime` 不知道 Tool 和 UI；`markdown` 不依赖 Host。
- 外部 Tool 使用发布的 **CLI 子进程接口 + 公开 JSON/文件协议**，不依赖 `crates/cli/src/analysis.rs` 的 Rust 私有函数。
- `marqdo-proof` 对公开投影的**受支持语义子集**建立模型；不向解释器注入 Lean 代码，也不让 Lean 成为运行时依赖。
- 公开协议只能含稳定的 `ModuleId`/`FunctionId`/资源身份、语义投影、来源哈希、运行事件；不能暴露内部 Host 指针、插件 handle 的实际权限、`Box`/`Rc` 或私有优化 IR。

### 4.2 设计取舍：为什么不把一切都做成通用插件

语言运行正确性依赖一套确定的求值规则，安全依赖可审计的 Host 权限边界。盲目把 AST 节点语义、内置操作、权限裁决全部开放会使 `check` 与 `run` 对同一程序给出不一致结果。应将 **可调用实现、Foreign Provider、CLI Tool、包来源扩展**设为开放注册点；将 **语法、运行值、名称解析规则和权限最终判决**固定在核心。开放的是有明确协议的边界，而不是开放任意劫持入口。

## 5. 问题清单与优先级（截至基线提交）

| ID | 优先级 | 确认/风险 | 问题 | 直接影响 | 主负责落点 |
| --- | --- | --- | --- | --- | --- |
| B-01 | P0 | CI 确认 | 属性测试依赖 Git 不保留的空目录，HEAD 的 Linux CI 红灯 | 无可信发布基线 | `crates/cli/tests/properties.rs`，`tests/` |
| B-02 | P0 | 源码确认 | 锁文件生成只遍历根直接依赖，传递闭包不完整 | 多包不能充分可复现 | `cli/src/commands.rs::build_lockfile`、`marqdo-package` |
| B-03 | P0 | 源码确认 | `check_lock` 找不到锁条目就成功返回 | 未锁传递依赖可能被静默接受 | `marqdo-package::PackageResolver` |
| B-04 | P0 | 源码确认 | `sync` 使用计算出的新锁结果物化环境，却未见相应写回锁文件 | manifest/lock/env 可以分歧 | `cli/src/commands.rs::sync` |
| B-05 | P0 | 源码确认 | `hash_tree` 包含生成目录且吞掉 WalkDir 错误；`copy_tree` 未形成统一文件集策略 | 哈希不稳定、不可重现或复制错误 | `marqdo-package/src/lib.rs` |
| B-06 | P0 | 源码确认 | Analysis `project.root_id` = 绝对物理路径哈希 | 换机器/目录分析和 Trace 失配 | `cli/src/{analysis,trace}.rs` |
| B-07 | P0 | 源码确认 | Analysis 仅 `resources: <数量>` | 公式/代码资源无法由外部独立编译 | `cli/src/analysis.rs::build` |
| B-08 | P0 | 源码确认 | 运行时错误用入口文件解释跨模块 Span | 报错行列与原模块不符 | `marqdo-value` / `runtime` / `cli` |
| B-09 | P1 | 源码确认 | Tool 安装校验 Marqdo 版本，但未对 Analysis/Trace 协议完整协商 | 外部工具可能不兼容 | `cli/src/tool.rs` |
| B-10 | P1 | 源码确认 | 仅校验 Tool 入口文件哈希，不校验完整包 | 辅助脚本/资源被替换后仍可运行 | `cli/src/tool.rs` |
| B-11 | P1 | 源码+文档确认 | Worker 能隔离崩溃但未提供 OS 沙箱，CapabilityBroker 主要做授权判定 | 原生扩展不能安全宣称不可信执行 | `marqdo-host` / `plugin-abi` / `sandbox` |
| B-12 | P1 | 源码/功能缺口 | Foreign Provider 缺少锁定的环境/内核依赖闭包 | Python/Node/R 等环境不能完整重现 | `marqdo-foreign` / `marqdo-package` |
| B-13 | P1 | 源码确认 | Schema CI 只解析 schema JSON，未用 schema 验证真实产物 | 公共协议回归保护不足 | `spec/schemas` / CI / `tests/consumers` |
| B-14 | P1 | 源码确认 | Win/macOS CI 只执行 `cargo check` | 原生 Tool/Worker/路径行为没有跨平台运行证据 | `.github/workflows/ci.yml` |
| B-15 | P1 | 设计缺口 | 插件重复导出名尚无正式的按包限定别名机制 | 多供应商同名插件不方便共存 | `CallableRegistry`、frontend、包 manifest |
| B-16 | P1 | 源码确认 | Trace 无事件数量预算、Worker 单帧上限高且失败路径需硬化 | 大循环或恶意插件可消耗过多内存/磁盘 | `cli/src/trace.rs`、`plugin-abi/src/worker.rs` |
| B-17 | P2 | 已知缺口 | 跨模块 LinkCall 语法不支持，`ref` 选择资源有限 | 远期知识可寻址体验受限 | `spec/references.md` + 单独 ADR |
| B-18 | P2 | 已知缺口 | Math CAS、优化字节码、包 Registry、通用 REPL 等未实现 | 不影响首轮独立生态闭环 | 不纳入本阶段 |

**优先级说明：** P0 阻止 G-B；P1 按门槛区分：Tool/Schema/跨平台项阻止 G-B，OS strict 项阻止 G-C；P2 不阻止本阶段。每个条目必须用新增正反测试关闭，而非仅修改 `STATUS.md` 的文字。

---

## 6. H0：立即恢复可信 CI 基线

### 6.1 原因与定位

`crates/cli/tests/properties.rs::pure_prose_edits_do_not_change_executable_ir` 在正式运行测试逻辑前断言一个空目录存在。Git 不追踪空目录，因而新检出失败。**不允许删掉“纯叙述编辑不改变执行语义”的测试。**

### 6.2 实施方案

首选将该测试变成**不依赖无内容目录**的内存 fixture 测试；测试数据已存在代码内时，不需要断言空目录。若确需外部 fixture，则在 `tests/properties/` 放入实际有意义的版本化 fixture 文件，并让测试逐一读取，不能仅用空 `.gitkeep` 掩盖依赖不成立的设计。

修改之后固定基线：

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test -p marqdo-cli --test properties
cargo test -p marqdo-cli --test tool_protocol
cargo test -p marqdo-cli --test analysis_trace
```

若 CLI 或脚本因环境缺少 Python、Node、C 编译器而跳过可选测试，必须在 CI 中增加装有相应依赖的专用 job；**跳过不能算通过第三方扩展协议验收**。收集通过的 workflow run 链接并写入 `docs/STATUS.md` 的本阶段基线栏。

### 6.3 验收（H0-GATE）

- [ ] 全新 `git clone` 下不需要手动创建目录，`cargo test --workspace` 全绿。
- [ ] 叙述插入/删除的等价性测试没有被禁用或削弱。
- [ ] CI 的 Schema/Smoke job 不再因前置测试失败而跳过。
- [ ] 文档记录实际 SHA、job URL、平台和工具链版本，不依赖开发者本机状态。

## 7. H1-A：包图、锁文件及可复现依赖体系

### 7.1 为什么必须破坏性修改当前 lock 结构

当前 `LockFile` 是平铺的 `{name, instance_id, source, sha256}` 列表，`build_lockfile` 只遍历根 manifest 的直接 alias，而运行时 `resolve_alias_from()` 支持从依赖包自己的 manifest 访问后续依赖。这两条语义必须对齐，否则“可执行传递依赖”并不意味着“已锁定传递依赖”。另外 `check_lock()` 对缺失项返回 `Ok(())`，导致本应拒绝的未锁状态可能被当成有效。为了保证可重复构建，**锁定对象应是包实例及其依赖关系组成的闭包，而不是根项目的一组别名**。

### 7.2 推荐的 lock v3 逻辑模型

以下仅定义**逻辑结构**；YAML 字段名称、顺序在实施 ADR 中最终冻结。应采用有约束的最小模型，避免提前开发复杂依赖求解器：

```yaml
lock_version: 3
root:
  package: root:quantum@0.1.0
  manifest_digest: sha256:... 
packages:
  - id: dep:math@1.0.0#<完整摘要或足够长的消歧部分>
    name: math
    version: 1.0.0
    source: {kind: git, url: https://example.invalid/math.git, rev: <固定提交>}
    content_digest: sha256:...
    dependencies:
      helper: dep:helper@2.0.0#...
  - id: dep:helper@2.0.0#...
    name: helper
    version: 2.0.0
    source: {kind: path, path: packages/helper}
    content_digest: sha256:...
    dependencies: {}
root_dependencies:
  math: dep:math@1.0.0#...
```

**关键约束：**

1. `id` 唯一代表一个确定包实例；根名、`name`、`version` 不能作为唯一 ID，同名多版本可以并存。ID 的摘要不能仅靠未经碰撞检测的短前缀作为安全唯一性依据。若人类可读短名则必须有完整摘要消歧校验。
2. 依赖边由“包实例 + alias → 目标实例”表示，相同 alias 在不同包的含义不必相同。
3. 每个包实例有来源、固定 Git commit 或项目相对路径、规范化内容摘要、自己的依赖边。无锁包一律拒绝，除非处于显式生成锁定图的操作中。
4. 锁文件记录 root manifest digest；`run/check` 默认不从网络补充依赖，也不静默改写锁。
5. 本阶段只支持**path + pinned Git**；不要求 Registry、SemVer 求解器、依赖脚本、自动更新或签名服务。

### 7.3 解析流程（伪代码）

```rust
fn resolve_lock_graph(root_manifest: Manifest) -> Result<LockGraph> {
    // 1. 检查根清单，装入每个显式 alias。
    // 2. 对每个来源按其固定路径 / Git 提交读取包清单。
    // 3. 计算规范化包内容摘要，导出 PackageInstanceId。
    // 4. 以 PackageInstanceId 去重；检查同 ID 内容相异冲突。
    // 5. 遍历包清单的所有依赖，记录从包实例出发的 alias 边。
    // 6. 对传递图做 cycle/缺失/冲突诊断，而非无限递归。
    // 7. 输出按 ID、alias 确定性排序的完整闭包及根边。
}

fn check_locked(graph: &LockGraph, wanted: &ResolvedGraph) -> Result<()> {
    // 不存在的包实例、边、摘要、来源、固定提交 => 明确拒绝。
    // 所有已锁实例都要满足其锁摘要。
}
```

具体模块建议：

- `marqdo-package/src/lib.rs`：提取 `LockGraph`、`PackageInstance`、`LockedDependencyEdge`、包闭包构建/校验逻辑。无需增加 `marqdo-lock` 独立 crate。
- `cli/src/commands.rs::build_lockfile`：从“遍历直接依赖”改为调用包层 `resolve_lock_graph`。
- `PackageResolver::check_lock`：必须用当前包实例 + alias 来查锁边，而不是全局 alias 名。锁不存在时拒绝，禁止静默 `Ok(())`。
- `cli/src/loader.rs`：只消费解析完成的 `ResolvedModule`，不得自创另一套依赖匹配逻辑。

### 7.4 统一的文件集与哈希规范

`hash_tree` 与 `copy_tree` 必须使用同一个**规范化文件选择策略**。不要在 hash 和复制时使用不同的排除规则，否则 hash 指纹无法可靠代表被执行环境。

最小规则：

- 纳入 `.mq.md`、`marqdo.yaml`、包内实际需要的资源文件以及明确声明的可执行资产；默认忽略 `.git/`、`.marqdo/`、构建缓存、临时 staging 等生成目录。
- 规范化包内相对路径为 UTF-8、`/` 分隔、无 `..`、无绝对路径；不接受同一规范化路径映射到两个实际文件。
- 目录遍历**不允许** `.filter_map(Result::ok)` 静默吞掉错误；遇到权限不足、不可读文件必须终止构建。
- 明确 symlink 策略：第一版推荐拒绝包含 symlink 的可执行包源；避免 hash 时跟随、复制时拒绝引起不一致。
- 采用域分隔和文件名+文件长度+文件内容的确定性编码；包括 `content_profile_version`，将来更换忽略规则须改变 profile 版本。
- Git checkout 必须校验指定 revision 精确对应的提交对象，缓存命中仍要验证源及内容摘要。缓存不可信，不能仅凭目录存在就跳过验证。
- 对源包设置文件数、单文件字节、总字节、深度预算，避免恶意包或异常目录树耗尽内存/磁盘。

示例策略（规范性测试期待，不是已实现 API）：

```text
source package:  packages/math/
  index.mq.md           INCLUDE
  marqdo.yaml           INCLUDE
  assets/formula.json   INCLUDE
  .git/                 EXCLUDE
  .marqdo/env/          EXCLUDE
  target/               EXCLUDE (仅在规范明确为生成目录时)
  symlink -> /etc/passwd REJECT
```

### 7.5 `sync` 的事务语义

存在两种可行方案，必须选定一种并在 CLI 帮助/规范中公开。**推荐方案 A，减少隐式状态变更：**

- `marqdo lock`：计算并写入完整 lock v3（已存在的命令）；可以显式访问网络获取 pinned Git。
- `marqdo sync --locked`：只读取现有锁并校验、物化，不更改锁；缺失/漂移直接失败。
- `marqdo sync`：如果沿用旧行为要自动重锁，则必须把**新 lock + 新 env**作为同一事务（同 staging、fsync/回滚策略）发布；否则改为拒绝过期锁并指导用户运行 `marqdo lock`。**禁止只把新 env 生成了却留下旧 lock。**

为了减少状态机复杂度，更建议 `sync` 首版默认严格执行锁，更新仅由 `lock` 明确触发；这是一个有意允许的行为改变，须写 ADR 和迁移/错误提示。

### 7.6 H1-A 测试矩阵

| 测试 | 输入/操作 | 应当得到 |
| --- | --- | --- |
| 一级传递 | A → B → C | lock 包含 B/C，含 B→C 边，sync 可全部物化 |
| 多版本并存 | A→B@1 / A→C→B@2 | 两个 B 实例身份和实际导入互不干扰 |
| 不存在锁条目 | 删除 C 的锁节点或 B→C 边 | `sync --locked` / `check` 按契约失败，不静默接受 |
| 依赖漂移 | 修改 B 自己的 manifest，使其 C 改变 | 锁失配错误，禁止自动网络修复 |
| Git 迁移目录 | 同一固定 Git commit 在两台不同目录恢复 | 包实例 ID、导入解析保持相同 |
| 缓存污染 | 修改 `.marqdo/cache/git/...` 已缓存包内容 | 不能以“目录存在”接受污染内容 |
| 生成目录 | 增加 `.git` / `.marqdo` / target | 包内容指纹不因非分发内容改变 |
| 遍历失败 | 把包内某文件变为不可读 | 明确报错，不产生“部分哈希” |
| 恶意路径 | `../`、绝对、symlink/循环 | 拒绝且不离开包根 |
| 事务失败 | 物化环境中途断电模拟或注入 rename 失败 | 保留旧一致状态，无半成品锁/env |

**验收：** `marqdo.lock` 足以表达完整已支持的包闭包，在清洁机器有源/缓存可恢复；不锁定、不匹配、不完整一律失败。成功时锁、环境和实际执行模块一致。

## 8. H1-B：跨模块诊断与源码身份

### 8.1 当前错误

`cli/src/commands.rs::runtime_error_json` / `render_runtime_error` 用 `project.entry_file().source` 的行索引解释 `error.span`。如果运行错误发生在导入模块，`Span` 起止位置可能是那个模块的局部偏移，不能映射到入口文件。

### 8.2 目标模型

将源码位置当成不可分割的二元组：

```rust
struct SourceLocation {
    module_id: ModuleId,  // 包实例 + 相对路径，逻辑身份
    span: Span,            // 该文件 UTF-8 字节偏移
}
```

调用栈可以额外保存 `function_id` 和调用点 `SourceLocation`，并由 CLI 根据 `module_id → LoadedFile` 查找真实路径及 `LineIndex`。不要让 `Span` 单独跨文件传递后再猜是哪份源码。

设计建议：

- 优先扩展 `RuntimeError` 的**源模块**或定位结构，而不是只在 CLI 做字符串补丁。
- 所有执行节点/Host 效应可以携带已有 `ExecutionContext.module`；错误产生时附上该来源，转成 JSON 时保持 `module_id` 和 `span`，UI 字符行列只是展示层派生信息。
- Plugin/Foreign 产生的错误若只有调用点位置，应标记 `origin: callsite` 而非捏造外语内部栈；可选附加内核自己的错误信息。
- 错误 JSON 与 Trace v2（若需要）使用同一逻辑模块身份，禁止混用绝对文件路径作为持久地址。

测试：根入口 `index.mq.md` 调用 `deps/physics/index.mq.md`，在后者第 8 行故意除零；应报告后者的模块和第 8 行，而不是 `index.mq.md` 第 8 行。再测试多版本包两个同名相对路径的错误都能区分。

**验收：** 所有跨模块运行异常、调用栈及导出的 SourceSpan 都可以唯一映射至真实模块，不因同名文件、不同包实例或缓存位置产生歧义。

---

## 9. H2-A：重新冻结公开 Analysis 协议

### 9.1 为什么现在不能直接将 Analysis v1 永久冻结

现有 `cli/src/analysis.rs::build` 将项目 `root_id` 设为项目绝对路径的 SHA-256；同一项目复制到不同目录会变更 `root_id`，进而变更整个分析 JSON 摘要和 Trace 的 `analysis_digest`。与此同时，每模块的 `resources` 只有一个数量整数，无法为知识编译建立资源级的稳定寻址与来源关联。

这是**协议模型问题**，不是 Knowledge 项目该做的补丁。如果让 Knowledge 自己另建一套身份和 Markdown 解析规则，就会重新导致仓库耦合。

### 9.2 三种身份必须独立

| 概念 | 用途 | 修改时是否变化 |
| --- | --- | --- |
| `ProjectIdentity` / `ModuleId` | 逻辑归属、符号寻址 | 项目名称或符号/模块身份变更时变化；不因存储路径变化 |
| `SourceDigest` / `PackageContentDigest` | 内容变化检测与缓存 | 源码或真实分发内容变化时必须变化 |
| `AnalysisDigest` | 对整份语义投影及指定编译配置建立快照 | 语义投影、profile、锁图、编译器语义版本改变时变化 |

**不要用 `hash(absolute_path)` 充当项目身份。** 可以根据根 manifest `project.name + version` 和显式持久项目标识形成逻辑归属。无 manifest 的单文件场景需要独立定义 `workspace_snapshot_id`（可以使用来源内容摘要以区分无名项目），并说明其没有跨重命名持久身份承诺。

`AnalysisDigest` 必须使用规范化序列化的内容、必要的语义版本和规范化 lock；不能包含本机路径、临时文件名、当前时间、进程 PID 等与语义无关的数据。内容变更后 digest 应变化，纯位置迁移不应变化。普通叙述变更是否影响 AnalysisDigest 必须明确：因为源码源映射 `span` 会变化，所以**完整 Analysis snapshot digest 可以变化**，但同一函数的独立语义摘要可以不变；不要混淆二者。

### 9.3 资源级投影：知识编译不可或缺的最小字段

建议将原 `resources: <integer>` 改为资源数组，必要时另设 `resource_count`。资源目录内容读取仍通过源码路径/偏移而不是强行将所有文档内容复制到 Analysis。示例仅展示结构：

```json
{
  "id": "resource:root:quantum@0.1.0::index.mq.md#formula-1",
  "module_id": "root:quantum@0.1.0::index.mq.md",
  "unit_id": "<所属章节的 UnitId>",
  "kind": "formula",
  "span": {"start": 112, "end": 144},
  "content_digest": "sha256:<实际资源内容摘要>",
  "language": null
}
```

应覆盖公式（`Formula`）、代码围栏（`Code`）、可静态提取的 GFM 表格等实际由 `marqdo-ir::Resource`/AST 支持的资源。**先精确定义当前存在的资源种类，不得虚构尚未实现的 AST 节点。** 资源身份可由 `ModuleId + UnitId + 资源标识` 确定；如果同章节存在多个无显式锚点资源，须制定确定的、可检测重排和歧义的寻址规则。

SourceSpan 应始终指向具体**资源所属模块**，导出模块内规范化 `relative_path` 和 `source_digest`。View/Knowledge 可通过模块内容+span 获取资源，不得在 Analysis 中泄漏环境变量或执行时秘密。

### 9.4 分离可复用语义和原始语法信息

- `functions[]`：提供名称、签名、可观察的归一化函数体节点、来源定位、保守效应摘要。
- `imports[]`：显式别名、解析状态、目标 ModuleId；缺失绝不默默变“external”。
- `references[]`：Markdown 链接关系，独立于 `calls[]`，**引用不意味着调用**。
- `calls[]`：源函数、调用点、解析状态、绑定目标及必要参数信息；由与执行一致的符号解析机制生成。
- `resources[]`：可定位资源，保持原始 Markdown/公式/围栏文档源映射；静态投影不调用 Foreign 或 Math。
- `effects[]` 或 `effects`：`unknown` 是未知，不能表示已证明纯函数。模型仅提供保守静态摘要。

### 9.5 v1/v2 与兼容策略

在首次正式对外稳定发布前，推荐创建 `spec/analysis-v2.md` 和 `spec/schemas/analysis-v2.schema.json`，显式把 `schema_version: 2` 用于上述破坏性 `root_id`、资源结构变化。是否保留 `--emit-version 1` 为兼容选项需另行决定，**不要凭空增加新的顶层命令**。由于本仓库尚无正式发布，允许直接明确宣布 v1 是实验协议，不强制长期维护两份写入器；但必须修改文档、所有消费者 fixtures 和错误提示。

如果维护者选择不升级 schema_version，也必须通过 ADR 证明字段变化符合现有 v1 合同；考虑 `resources` 从整数变数组，这通常不符合。**推荐版本升级更安全。**

### 9.6 H2-A 必需测试

1. 相同项目检出到 `/tmp/projA` 与 `/tmp/projB`，导出的**规范化 Analysis 字节**相同（需将本机诊断路径从协议中剥离或独立标注），各 ModuleId 一致。
2. 改一行真正的执行表达式，source digest、function body、analysis snapshot digest 按规则变化。
3. 只改普通叙述，不改变程序执行结果；函数语义节点应按明确规范保持语义等价，SourceSpan 可以变化。
4. 一个章节下两个公式和一个代码围栏，分别拥有唯一资源记录，`Span` 和摘要可反查源内容。
5. 两个不同包实例里存在相同 `src/index.mq.md`，模块和资源身份不能冲突。
6. 缺失/未支持的节点不能被投影器“假装支持”：显式 `unsupported` 或稳定诊断。
7. 导出过程中发生错误时不留下看似成功的新分析文件；已存在旧产物按现有规范正确清理。
8. `check --emit` 过程中没有 Python/Node 进程、ABI 动态库加载和 `run` 副作用。

## 10. H2-B：Trace 的身份、源映射与容量边界

现有 Trace v1 已记录 `call.enter` / `call.exit`、`branch.eval`、`loop.iteration`、`return` 和 Host effect，默认不记录值和秘密，这个隐私立场应保留。需要修正：

1. **根身份与 Analysis 一致**：Trace 应绑定公开 Analysis 的逻辑身份和规范化摘要，不再依赖物理目录路径哈希。
2. **事件来源完整**：`effect` 已有 `module`；其他语义事件的源位置必须可以通过 `function_id` 或显式 `module_id` 解析，跨模块不能误读入口文件；如果语义事件不能唯一定位，增加稳定来源字段。
3. **有限预算**：加入可复用的事件数量和字节预算，超限时要有明确 `trace.truncated` / `resource.exhausted` 结尾；不能在无限循环里无限增长 Vec 和输出 JSON。
4. **状态确定**：运行失败仍应记录错误终态；外部工具不能把旧 Trace 误认成新执行。
5. **机密性**：坚持不记录参数值、文件内容、密钥、句柄；未来 View 想要局部变量时应通过显式、受权限约束的调试 API 单独设计，不能悄悄扩大 Trace 默认数据集。

`run/call --trace-file` 入口不变，若修改必需字段则采用 `trace-v2`，保持与 Analysis 协议独立版本。测试包含 10 万次循环触发上限、外部插件异常、跨模块事件关联、移动路径之后的源匹配及 secret 红action（搜索全量输出不得含测试密钥）。

## 11. H2-C：通用 CLI Tool v1 完整性与兼容性

### 11.1 继续坚持完全通用的工具名称

核心**不能**在代码中出现 `knowledge` / `prove` / `view` 的特殊命令分发。已有 `crates/cli/src/tool.rs` 使用未知命令回退与 `tool.json` 注册，这是正确方向。保留最小：

```bash
marqdo tool install path/to/tool.json
marqdo tool list
marqdo tool remove benchmark
marqdo benchmark run .     # 测试工具名核心完全不认识
```

不能因为要管理三款官方工具就添加内置白名单或工具专用 Rust crate。

### 11.2 兼容性必须实际判断，而非仅记录字段

当前 `compatibility.marqdo` 已做版本校验；`compatibility.analysis` 与 `.trace` 虽可记录，但尚需在**安装和分发前**对用户请求的协议版本作严格协商。建议：

- 工具声明支持 Analysis/Trace 的版本列表；**只有工具声明使用某协议时才校验该协议**。
- CLI 的 `SupportedProtocols {tool:1, analysis:[...], trace:[...]}` 是内核对外事实，不由官方工具决定。
- 安装或启动时交集为空 → 明确错误，不得尝试执行后靠 JSON 解析失败；没有声明则不强制工具使用该协议。
- 工具版本采用确定的版本语义；现有简化解析可能接受不完整表达式，要测试预发布号、边界和不合法范围。可引入小型可靠版本约束库，**不需要新管理命令**。

### 11.3 全工具包不可变完整性

当前安装记录主要保存入口文件 SHA-256。工具可能包含动态库、脚本和资产；只校验入口文件无法检测其余依赖资源变化。

最小安全可行方案：安装时计算包目录内的**规范化文件清单**（排除 record/staging/不随包发布资源），为每个文件记录相对路径、大小、SHA-256；分发前至少验证完整包摘要或每个条目。采用不可变用户工具目录，安装/卸载基于 staging + atomic rename，禁止 symlink、路径逃逸和重复覆盖。若确需可写运行数据，应放到独立 state/cache 目录而非安装包内部。

注意：这保护的是**安装后完整性**，不证明工具作者可信。来自网络的来源认证应由未来可信索引或签名解决，不应借本阶段强行添加内置官方工具市场。

### 11.4 Tool 进程安全说明

工具运行在用户权限下，不是 `.mq.md` 的 `pure/project/strict` 沙箱。继承环境变量可能泄漏调用者的密钥，默认应在规范里明确说明；若未来支持受限工具运行，则需新的显式权限模型，不得把 Tool 当成已沙箱化。

### 11.5 Tool 黑盒验收

- 与 CLI 完全无关的 `benchmark`、`my-quantum-lab` 安装后正常转发 Unicode 参数、空格、stdin/stdout/stderr 与退出码。
- 工具只支持 Analysis v2，而 CLI 只支持 v1 → 安装/分发明确失败。
- 辅助脚本被篡改 → 拒绝启动，即使入口文件未变。
- 已安装注册文件中的 `entrypoint` 被手工改成 `../outside` → 拒绝。
- 与内置命令冲突 → 安装拒绝；不同外部工具重名 → 不覆盖已安装内容。
- 工具不需要 Analysis/Trace → 不应被强迫支持它们。
- 删除工具后同名命令回到 unknown；移除不能删除安装根以外文件。

## 12. H3-A：ABI Capability Broker 的真正执行闭环

### 12.1 当前真实边界

`CapabilityBroker::authorize()` 会通过 `decide()` 判断 `fs.read` / `fs.write` / `net.request` 等能力，并为审计日志记录结果；对成功授权主要返回空字节。**授权被允许**与**操作已经执行**是两回事。Worker 目前主要通过独立进程实现插件调用/崩溃隔离，并没有自动获得可验证的 OS 级权限限制。

### 12.2 目标接口：Request → Authorize → Execute → Reply → Audit

```text
Plugin process        Host Broker          Authorized Service
   request ─────────► 解析/身份核验
                         │
                         ├── deny ────────► 错误 + 审计
                         │
                         ▼
                    grant intersection
                         │
                         ▼
                     execute service
                         │
            reply ◄──────┘
```

- 请求必须携带与 Worker 会话绑定的真实插件身份（不可由插件文本单独决定）、本次调用身份、操作名和结构化参数。
- **有效授权 = Host 当前策略 ∩ 用户显式授权 ∩ 当前任务/插件上限**，manifest 只能请求不能授予。
- Broker 不只是返回 `Ok(())`；真正实现的核心操作必须产生明确结果与结构化错误。尚未支持的能力返回 `capability.unavailable`，不能用空字节冒充执行成功。
- Worker 内的 native 代码不能自动通过这种 Host API 获得 OS 沙箱；必须按 H3-B 的 OS 隔离要求同时限制其直接 syscall。
- Vendor capability 可以使用独立可信 Handler 注册，但不得通过名称猜测把未知能力提升为可用；其注册/调用均受权限和审计约束。

### 12.3 最小实现范围

建议先打通一个 `fs.read` 受限路径的端到端插件示例，返回受限测试文件内容。网络、任意进程、复杂设备操作暂不实现，留在 `unavailable`。不要求为了实现 Broker 而发明全部 Host API。

保留 `PluginHandleTable` 的 owner/run/generation 校验：任何从 wire JSON 重构的句柄不得自行成为授权凭据，跨运行、卸载后、跨 owner 的重放必须失败。插件 Worker IPC 应限制帧大小、请求速率、超时、总内存；崩溃后清理状态和子进程，不能残留可用句柄。

### 12.4 测试

1. 已获授权插件读取 `data/allowed.txt`：Host 完成读取并返回真实内容与审计记录。
2. 未授权读取 `private/secret.txt`：`permission.denied`，不返回任何内容。
3. 插件请求尚未实现的 `net.request`：明确 `capability.unavailable`（即使声明了名称）。
4. 伪造 `package_id`、handle、会话代次：拒绝，无其他用户资源泄漏。
5. Worker 崩溃、超时、发送超大 frame：受限失败、子进程清理、Host 正常存活。

## 13. H3-B：严格沙箱与 Foreign 环境

### 13.1 strict 的可执行定义

目前 `marqdo-sandbox` 在缺 OS 隔离实现时拒绝 strict，这是正确安全承诺。**不可**因为 Worker 已存在就把 `strict` 从 unavailable 改为 available。C ABI/Foreign 原生代码默认可使用其 OS 进程权限；一个子进程只能隔离地址空间和部分崩溃，不等于访问控制。

推荐先实现**一个经测试的平台**（优先 Linux），并清楚列出所需 OS 能力：文件系统可见范围、进程启动、网络访问、CPU/内存/文件数/进程数与墙钟预算、子进程树生命周期。优先基于成熟系统级机制（namespaces、seccomp、cgroups，或受支持的现成沙箱封装），不从零写容器平台。无法获得必要机制时拒绝 strict，并报告准确原因。macOS 和 Windows 没有等价适配前继续 fail closed。

### 13.2 路径与时间竞争

`ProjectFs` 的路径检查不能依靠一次 canonicalize 后再无限信任后续路径字符串。应使用支持目录句柄相对访问的安全路径方案，明确 symlink / rename TOCTOU 攻击的负面测试。尤其写操作在检查与创建之间需要维持目录边界，不允许通过切换链接写到项目外。

### 13.3 Foreign Kernel 运行环境

当前 `KernelRegistry` 已允许注册 `mocklang` 等外部执行器，不能为了 Python、Node、R 增加新的解释器分发 `match`。但 `python`/`node` 的包依赖、具体解释器版本与执行路径还未进入锁定环境模型。

**最小路线：** Kernel Provider descriptor 提供 `provider_id`、协议版本、入口二进制内容哈希、语言别名、可选 `environment_id` / 环境摘要；环境管理由 Foreign/Package 协作完成，不能在 CLI 中为每种语言写一套包管理器。Python 包可委托 uv、Node 依赖可委托成熟包管理器，但必须按固定 lock 和隔离目录运行，且**绝不能把 Marqdo 程序翻译成 Python**。本阶段仅须能将一个指定版本的 Foreign Provider 和它的依赖在两台机器一致恢复的测试用例闭环；不用自己开发 PyPI/npm 解析器。

对于扩展脚本、网络访问、进程树及资源耗尽，strict 要真正加 OS 限制；project 模式只能说明 Host 路径受到检查，不能宣称子进程的一切 syscall 都被能力 API 控制。

### 13.4 可信执行验收（G-C）

- Linux strict 下插件尝试读取未授权 `/etc/...`、写工作区外文件、建立网络连接、启动子进程，必须被真实 OS 策略拒绝。
- 异常退出、无限递归、海量输出、fork 子进程和超大内存分配受限，不留下孤儿进程。
- 没有必要 OS 特权或内核能力时，拒绝启动 strict，并输出稳定错误码而非静默退回 project。
- 另外两个平台如未实现，CI 明确测试 `strict unavailable` 且拒绝执行；不要将“Windows/macOS cargo check 通过”写成隔离支持。

---

## 14. 测试总策略：协议与负例优于堆叠功能

### 14.1 测试金字塔

| 层级 | 目标 | 位置与形式 |
| --- | --- | --- |
| 单元测试 | 解析、归一化路径、LockGraph、摘要、版本协商、Handle owner/generation | 各 crate 的 `#[cfg(test)]` 或 `/tests` |
| 属性测试 | 纯叙述变更不影响执行语义、顺序确定性、路径迁移稳定、可逆 wire roundtrip | `crates/cli/tests/properties.rs`、package / value tests |
| 集成测试 | CLI 原命令的真实 exit/status/stdout、包图物化、跨模块错误 | `crates/cli/tests/` |
| 协议契约测试 | 对公开 Schema 检查真实 Analysis/Trace/Tool，未知字段和版本拒绝 | `tests/consumers/`，Python / JS / Go 非 Rust 消费者 |
| 系统负例 | 未授权 FS/Network、Worker 崩溃、超时、沙箱逃逸、tool package tampering | 独立 Linux job，必要时专用沙箱 CI Runner |
| 跨平台 | Win/macOS/Linux 的编译 **和**可执行 API 合同 | GitHub Actions matrix + artifact smoke |
| 生态端到端 | 不修改 CLI，分别安装第三方 Tool、Plugin、Kernel，消费 Analysis/Trace | `examples/` + `crates/cli/tests/ecosystem_contract.rs` |

### 14.2 正反测试清单（最小发布集合）

| 类别 | 正例 | 负例 | 阻断门槛 |
| --- | --- | --- | --- |
| 文档 AST | 既有 150 AST 金样持续全绿 | 无效 Markdown 执行节点报结构化错误 | G-B |
| 执行语义 | 既有 36 语义样例持续全绿 | 叙述修改与非法函数调用负例 | G-B |
| 包图 | 多版本与传递 B→C 的 lock 闭包 | 未锁依赖、缓存污染、相异内容同 ID | G-B |
| 环境 | 同锁复现 identical logical graph | sync 中途失败事务回滚 | G-B |
| 源码定位 | 跨模块错误映射到正确文件与行号 | 两包同路径不混淆 | G-B |
| Analysis | 移动项目路径后规范化摘要一致、资源可定位 | 不支持协议/无效资源拒绝 | G-B |
| Trace | 正确语义事件序列、失败终态 | 已过期/溢出/秘密数据不泄漏 | G-B |
| Tool | 未知工具名、Unicode 参数、完整 bundle 验证 | 版本不兼容、资源篡改、冲突与路径逃逸 | G-B |
| Plugin | 多导出、不同包实例、合法 handle | 错误签名、跨 owner、Worker 崩溃 | G-B（基础）；G-C（不可信） |
| Kernel | 外部 mock provider 通过 wire 协议调用 | 不存在标签、协议不符、超时、越权 | G-B（基础）；G-C（不可信） |
| Host Broker | 授权 `fs.read` 返回真实结果 | 拒绝目录外及未知 vendor 能力 | G-C（如宣传完整受控扩展） |
| strict | Linux 限权执行在可用平台有效 | syscall 越权、孤儿进程及不可用平台拒绝 | G-C |

### 14.3 JSON Schema 验证必须是**实例验证**

目前 CI 有 `json.loads(schema)`，只能证明 Schema 文件本身是合法 JSON，不能证明导出结果符合它。应使用真实 JSON Schema 2020-12 validator（Python `jsonschema`、Rust 对应库或等效工具）在 CI 上完成：

```text
emit actual Analysis artifact
emit actual Trace artifact
read actual Tool manifest
validate each against the advertised matching schema
modify one required field to a wrong type -> validation must fail
modify schema_version to an unsupported value -> consumer must reject
```

此外，Schema 本身必须提供足够的约束；若只把 `functions` 写成 `{type: array}` 而不约束内部 `id/signature/span`，测试无法防止破坏性回归。需以**真实公开消费者需要的字段**为依据定义 `required`、`enum`、格式和 ID 关联检查，而不是机械复制私有 Rust 结构。跨记录一致性（资源 span 在源文件长度内、引用 target 存在等）需额外语义校验，Schema 无法替代。

### 14.4 完全独立的模拟消费者

至少维护三个与 CLI Rust workspace 无任何源码链接的消费脚本：

1. `tests/consumers/analysis_consumer.py`：从 Analysis 获得函数、签名、规范体、公式与代码资源及依赖，证明 Knowledge 不需内部 AST。
2. `tests/consumers/trace_consumer.py`：读 Analysis + Trace，验证 `analysis_digest`、按 ModuleId 定位运行事件，检测移动路径/旧数据。
3. `tests/consumers/tool_consumer.py` 或可执行 fixture：实现未知名称的 Tool，声明准确协议版本，直接调用 CLI 并解析结果，验证返回码和版本协商。

消费者不应包含对 `crates/marqdo-*` 的 `use` / `import`。应在 CI 中以真实二进制和文件运行，而非调用 CLI 内部 Rust 函数。

### 14.5 跨平台 CI 修改建议

当前 `.github/workflows/ci.yml` Linux 全量测试，Windows/macOS `cargo check`。G-B 前调整为：

```text
linux:   fmt + clippy + full tests + schemas + examples + C ABI + tool/kernel
windows: cargo test --workspace + tool/analysis/package portable smoke
macos:   cargo test --workspace + tool/analysis/package portable smoke
linux-security: strict/worker/fs security negative tests (具备相应隔离条件时)
```

不能把“当前跨平台检查通过”误写成“运行时跨平台支持充分测试”。Windows/macOS 遇到只能 Unix 执行的 shell fixture，应使用 Rust 构建的小工具或跨平台脚本替代，不应静默 skip 对应合同测试。

### 14.6 可执行的基本命令清单

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test -p marqdo-cli --test properties
cargo test -p marqdo-cli --test package_env
cargo test -p marqdo-cli --test analysis_trace
cargo test -p marqdo-cli --test tool_protocol
cargo test -p marqdo-cli --test plugin_worker
cargo build -p marqdo-cli
./target/debug/marqdo check examples/multi-module/index.mq.md --refs --emit ./analysis.json
./target/debug/marqdo run examples/hello.mq.md --trace-file ./trace.json
```

以上既有命令按当前仓库确实存在的接口写出。后续新增协议版本参数应在 ADR 中明确，不得以本文示例伪装成已实现选项。

## 15. 分阶段交付与明确停机点

### S0（立即）：审计和 CI 基线

**修改：** `properties.rs` fixture 依赖、CI、`STATUS.md`。

**完成条件：** 完整新 clone 在 CI 通过；保留现有 AST、semantic、negative、Tool、ABI、Kernel 回归。此阶段不改语言语义、包协议、Tool 协议。

### S1（G-B 前置）：可复现锁图与包文件集

**修改：** `marqdo-package`, `cli/src/commands.rs`, `spec/package-format.md`, lock 迁移 fixtures。

**实施顺序：** 先设计 lock v3 ADR 和 fixtures → 实现统一文件集哈希 → 完成包闭包图 → 严格校验所有锁边 → 一致性 sync → 跨机器与错误注入测试。

**完成条件：** path/Git 双来源、A→B→C、多版本 A→B1/B2、缓存污染、生成文件、事务失败全部通过；`run/check` 不自动联网或改锁。

### S2（G-B 前置）：源码身份与公开语义协议

**修改：** `marqdo-value`/`runtime` 的源码身份、`cli/src/{analysis,trace}.rs`，`spec/analysis-v2.md`、Schema，外部消费者测试。

**实施顺序：** 定义逻辑身份与 digest → 修跨模块 RuntimeError → 资源级投影 → 版本化 Analysis/Trace → 独立消费脚本 → 跨目录与跨机器复现测试。

**完成条件：** 同一项目位置无关，资源可定位，Trace 能对上分析与源码，任何未知协议/资源类型不会被伪装为支持。

### S3（G-B 前置）：Tool 兼容性与完整包验证

**修改：** `cli/src/tool.rs`、`spec/tool-v1.md`（如破坏性变化则 v2）、bundle fixtures、schema/契约测试。

**实施顺序：** 定义 SupportedProtocols → 实装版本交集检查 → 统一 bundle 内容清单 → 注册/执行前完整性验证 → 黑盒未知工具测试。

**完成条件：** 任意名字工具安装调用无需修改核心；不兼容或被篡改工具被可靠拒绝；工具不受 Knowledge/Proof/View 特殊待遇。

### S4（G-B 收尾）：质量门槛、协议冻结、独立仓库启动

**修改：** CI 跨平台运行测试，公开 Schemas 的真实验证，`STATUS.md`、Release notes/兼容性表。

**完成条件：** 通过 G-B；发布 `marqdo-cli` 与公开协议版本；Knowledge、Proof、View 可在独立仓库仅依靠发布二进制和标准文件继续开发。

### S5（G-C 可独立推进）：安全执行闭环

**修改：** Host Broker 实际受控服务、Worker IPC/预算、Foreign 锁定环境、`marqdo-sandbox` Linux adapter、安全负例与 per-platform policy。

**完成条件：** 只在通过真实 OS 隔离测试的平台宣称 strict 可用；不支持平台 fail closed。该阶段不得要求 Knowledge、Proof、View 修改核心语义。

**各阶段必须附有：** 一份可复现的失败测试（先红）、最小实现、通过证据、协议/安全影响说明、明确不实现范围。若某阶段试图增加大量 CLI 命令或业务功能，应拒绝并拆到独立仓库。

## 16. 如何与现有规范、ADR、状态表协同

建议阅读顺序：

1. `README.md`、`AGENTS.md`：项目定位和开发原则。
2. `spec/markdown-profile.md`、`spec/mapping.md`、`spec/evaluation.md`、`spec/references.md`：语言的权威行为。
3. `docs/adr/0008`—`0015`：已接受的包身份、Callable、ABI、Broker、Provider、Tool、Handle、Analysis/Trace 决策。
4. `spec/package-format.md`、`spec/tool-v1.md`、`spec/analysis-v1.md`、`spec/trace-v1.md`、`spec/security.md`：当前公开/候选协议。
5. **本文**：当前基线上的缺口、修复设计、实施顺序与验收。
6. `docs/STATUS.md`：随每次合并同步的实现状态，不能以旧状态覆盖最新证据。

注意：仓库部分 `spec/` 文件还带有旧阶段 `Proposed` 文案，与代码已实现状态冲突。每次规范变更都要做一次 **spec ↔ code ↔ tests ↔ STATUS ↔ README** 五方核对，不可让“完成”只出现在文档中。旧 `docs/NEXT_PHASE_IMPLEMENTATION.md` 是 M0.6 设计背景及前阶段实施计划，不能再将其中“尚未实现”作为新 HEAD 的现状。应由本文补充新的当前阶段权威基线，并在它们首页相互指向，避免维护两份冲突的“最新开发任务”清单。

### 16.1 PR 模板最低要求

```markdown
## Problem
- 问题 ID：B-xx / Sx
- 当前失败的公开可运行测试（链接）

## Scope
- 改哪些 crate 和协议
- 明确哪些语言语义/CLI 命令不改变

## Why this design
- 为什么不在 Tool/Proof/View 内实现
- 备选方案和舍弃原因

## Verification
- 新增正向、负向、属性、跨平台/跨仓测试
- cargo fmt / clippy / test / Schema / CI 链接

## Compatibility & security
- 格式版本、迁移方式、拒绝降级行为
- 权限和可信边界是否变化
```

任何会改变用户可见行为的更改必须有 ADR；仅内部错误修复可只增加测试和状态记录。`check --emit` / `run --trace-file` 是对外协议的一部分，其字段变化不能以“内部重构”掩盖。

## 17. 新开发者第一周执行清单

1. 克隆仓库、记录完整 HEAD 与 CI 状态；运行全工作区测试，**复现并修复 B-01**；不要先开发新功能。
2. 阅读语言语义规范和 ADR-0008—0015，确认外部 CLI Tool / ABI Plugin / Foreign Kernel 的不同边界。
3. 从 `build_lockfile`、`resolve_alias_from`、`check_lock` 和 `hash_tree` 画出已有包解析数据流，先写 A→B→C 失败 fixture，再提出 lock v3 ADR。
4. 在两份不同绝对路径下生成 Analysis v1，确认 root_id 的迁移不稳定问题；设计不依赖路径的逻辑身份和 source/analysis digest。
5. 从 `marqdo-ir::Resource` 和 `cli/src/analysis.rs::build` 对照实际资源种类，设计可定位资源数组；避免重新解析 Markdown 成执行语义。
6. 用两个不同模块创建真实运行错误，先写失败测试，再让 `RuntimeError` 携带模块身份。
7. 使用**陌生工具名**制作第三方 Tool，验证不修改任何核心源码即可接入；对 Tool、Analysis、Trace 的版本不兼容写黑盒负例。
8. 合并修正后更新对应 `spec`、`ADR`、`STATUS`、CI；每个版本门槛以真实 run URL 而非文字宣称为准。

## 18. 不在本阶段实现的内容（防止再次膨胀）

明确不作为 G-B 的阻断项：

- Marqdo 知识图谱构建、OKF 导入导出、向量检索、Agent 记忆（**marqdo-knowledge**）。
- Lean 证明义务、证明证书、数学/程序证明系统（**marqdo-proof**）。
- Markdown 前端、交互式编辑器、实时调试可视化、网站生成（**marqdo-view**）。
- bytecode VM、JIT、复杂 CAS、矩阵/微积分全套、完整 registry、自动更新器、包发布门户。
- 新的 `marqdo graph`、`marqdo prove` 内置子命令枚举、`marqdo view` 内置模块、`marqdo kernel`/`env` 管理命令等。外部工具通过已实现的通用 Tool 协议自行注册。
- 任何需要“核心先知道官方扩展名字”才能工作的接入机制。

**特别说明：** G-B 并不要求所有潜在外部用例都已经实现，而要求**公开数据模型没有已知错误，且能支持真实独立消费者**。Proof 的全语言形式化验证范围仍应由它自己的仓库定义，不得借“为未来 Proof 预留”要求 CLI 公开一套未经验证的超大 IR。

## 19. 最终验收与准入判定模板

```text
Commit: <full SHA>
Release/Profile: <core version> / <markdown profile>
CI Linux full test: <URL> PASS|FAIL
CI Windows runtime contract: <URL> PASS|FAIL
CI macOS runtime contract: <URL> PASS|FAIL
Lock graph tests: PASS|FAIL
Package integrity / transactional sync: PASS|FAIL
Cross-module diagnostic source: PASS|FAIL
Analysis identity / resources / digest: PASS|FAIL
Trace source / stale detection / quota: PASS|FAIL
Tool generic name / compat / bundle integrity: PASS|FAIL
Schema instance validation / external consumers: PASS|FAIL
G-A: PASS|FAIL
G-B: PASS|FAIL
Linux strict isolation negative tests: PASS|FAIL|UNAVAILABLE
macOS strict isolation: PASS|FAIL|UNAVAILABLE
Windows strict isolation: PASS|FAIL|UNAVAILABLE
G-C per-platform: <precise result>

Unresolved risks with owners: ...
```

G-B 的成功标准不是“功能菜单齐全”，而是 **Marqdo 在不依赖外围仓库的条件下正确运行，依赖可重建，第三方工具无须改核心即可接入，知识/证明/View 可稳定消费同一套公开事实**。G-C 才允许严格宣称“不可信代码受 OS 隔离执行”。若某项条件仍失败，应保持对应门槛未达成，不能用任务数量或测试通过百分比替代。

---

## 附录 A：本次审查源码入口（固定 HEAD）

- [GitHub 当前核心仓库](https://github.com/cflmy/marqdo-cli/tree/0adebe42f14d72a2a275564d03ff9558f197766e)
- [Linux CI 失败与 Windows/macOS cargo check 结果](https://github.com/cflmy/marqdo-cli/actions/runs/44671025763)
- [CLI 命令实现](https://github.com/cflmy/marqdo-cli/blob/0adebe42f14d72a2a275564d03ff9558f197766e/crates/cli/src/commands.rs)
- [PackageResolver 与 hash_tree](https://github.com/cflmy/marqdo-cli/blob/0adebe42f14d72a2a275564d03ff9558f197766e/crates/marqdo-package/src/lib.rs)
- [Analysis 导出](https://github.com/cflmy/marqdo-cli/blob/0adebe42f14d72a2a275564d03ff9558f197766e/crates/cli/src/analysis.rs)
- [Trace 导出](https://github.com/cflmy/marqdo-cli/blob/0adebe42f14d72a2a275564d03ff9558f197766e/crates/cli/src/trace.rs)
- [CLI Tool 协议实现](https://github.com/cflmy/marqdo-cli/blob/0adebe42f14d72a2a275564d03ff9558f197766e/crates/cli/src/tool.rs)
- [CapabilityBroker](https://github.com/cflmy/marqdo-cli/blob/0adebe42f14d72a2a275564d03ff9558f197766e/crates/marqdo-host/src/capability.rs)
- [Worker 子进程](https://github.com/cflmy/marqdo-cli/blob/0adebe42f14d72a2a275564d03ff9558f197766e/crates/marqdo-host/src/plugin_worker.rs)
- [现有公开 Analysis v1](https://github.com/cflmy/marqdo-cli/blob/0adebe42f14d72a2a275564d03ff9558f197766e/spec/analysis-v1.md)
- [现有 Tool Protocol v1](https://github.com/cflmy/marqdo-cli/blob/0adebe42f14d72a2a275564d03ff9558f197766e/spec/tool-v1.md)

## 附录 B：参考工程与使用边界

- **Cargo**：学习 lockfile 中 package instance、来源、完整依赖图与 CLI 外部子命令；不照搬 Cargo 的全部包发布和 registry 产品。
- **uv**：学习项目环境、依赖锁定、显式同步与可重复构建；只在需要 Python Foreign 环境时复用其能力，**绝不将 Marqdo 作为 Python 翻译器**。
- **CommonMark / GFM**：作为 Markdown AST 输入规范；不允许第三方随意重定义执行 AST 节点。
- **JSON Schema 2020-12**：外部消费者合同的结构验证，需搭配跨对象语义验证。
- **Language Server Protocol / Debug Adapter Protocol**：未来 View/IDE 的可选互操作参考；**不要求 Core 在本轮实现 IDE 服务器**。
- **Lean 4**：Proof 项目的可信证明内核；**不能把外部形式模型自动等同于 Marqdo 真实执行语义**。

**本文件是开发与验证规范，不是已经实现上述功能的声明。所有建议都须以新增测试、ADR 和真实 CI 证据落实。**

## 附录 C：修订包来源说明（原 `marqdo-cli-stabilization/README.md`）

以下保留整合前 `docs/marqdo-cli-stabilization/README.md` 的原文，作为来源与审查基线记录。该修订包现已合并入当前文档体系：主规范位于 `docs/CORE_STABILIZATION_AND_ECOSYSTEM_GATE.md`；原目录已删除。原文中的“放入仓库”表述保留为历史记录。

> # Marqdo CLI 下一阶段实施规范
>
> 请将 `docs/CORE_STABILIZATION_AND_ECOSYSTEM_GATE.md` 放入 `cflmy/marqdo-cli` 仓库相同相对路径。
>
> - 该文件是针对 `master@0adebe42f14d72a2a275564d03ff9558f197766e` 的下一阶段工程设计与验收规范。
> - 不迁移旧 `cflmy/marqdo` 仓库；不增加任何新的顶层命令；不将 Knowledge、Proof、View 的业务代码引入 CLI。
> - 实施前先刷新 HEAD/CI 基线，若仓库已发生改变，必须按最新源码修订问题状态。
> - 文中拟议的 Lock v3、Analysis v2 与严格沙箱均非当前已实现功能。实现前应补充 ADR 和失败测试。
