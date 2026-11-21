# RFC-EXT-0001：Marqdo CLI 可扩展性闭环与第三方仓库彻底解耦实施规范

> **状态：Implemented（实现与证据已按本文件验收，见 §19）。** 本文同时保留为规范性任务书和验收记录；其中 `MUST` 项目已由 PR-00…PR-08 实现并测试。发布 tag 仍受仓库既有的 G3/G4/平台门禁与 Owner 批准约束，见 `docs/releasing.md`。
>
> **实现基线：** `6a7bb98`（`feat/extensible-ecosystem`，2026-11-22，Linux x86_64）。实现与测试证据：`docs/evidence/2026-11-22-ext-implementation.md`；冻结 CLI 外部证据：`docs/evidence/extension/6a7bb983c3b34fd0340afdcba9f197f7f50cba2a/manifest.json`（CLI sha256 `28e3776d…`，fixture repo `e4b7352c…`，全部必需结果 PASS）。
>
> **原始任务书文本保留如下。**文中标注 `MUST` 的项目是必须达成的验收要求；`SHOULD` 为建议，偏离须写 ADR；`MAY` 为可选。本文同时给出架构、接口、程序顺序、负例、操作命令、交付物和退出条件。未明确授权修改仓库；实施者应以 PR 形式完成并保留全部证据。
>
> **固定审查基线**：`cflmy/marqdo-cli`，`master@595279fe38048b52b96d9ff1e7d57a867ec5c4c6`（2026-11-21 查验）。原仓库：https://github.com/cflmy/marqdo-cli 。如果开发开始时 HEAD 已移动，先按 §2 的盘点模板重新核对事实，不能把本文基线判断无条件套用到新 SHA。
>
> **语言权威**：现行单一语义 RFC-LANG-0002 / ADR-0052；不得恢复 V1 兼容解释器，不得使用 `syntax:` / `marqdo_semantics:` 选择双语义。现有 Plugin ABI v1、Value Wire、Analysis v2、Trace v2 等公开协议不得静默改义；需要破坏时另立版本并给出明确不兼容错误。
>
> **本阶段目标**：新增第三方 `.mq.md` 函数库、Callable 插件、Foreign Kernel、Resource Provider、CLI Tool 或只读知识/证明消费者时，开发者**不修改 marqdo-cli 源代码、不增加内置分发分支、不重新编译既有 CLI**；通过显式声明、加载、授权、版本固定与协议测试即可使用。对基础语法、新基本类型、新 OS 原语的变更不作这项保证。

---

## 0. 执行摘要：哪些事必须做到，哪些坚决不做

### 0.1 完成定义（Definition of Done）

在**冻结的同一份 Marqdo CLI 二进制**上，另建 `marqdo-ext-fixtures`（仓库可命名为 `marqdo-extension-conformance`）并独立构建以下五类扩展：

1. 一个固定 Git SHA 的外部纯 `.mq.md` 库，具有跨模块函数，使用 `marqdo sync` + `run/check` 正确引用；
2. 一个导出多个不同前缀函数的外部 Callable Plugin（Worker 默认），静态签名和运行参数绑定一致；
3. 一个语言标签完全未知（例如 `mocklang`）的 Foreign Kernel，通过 `--kernel` 登记并执行；
4. 一个资源种类完全未知（例如 `demo.sensor`）的 Resource Provider，通过**普通 CLI**加载，完整实现 `open/read/write/close`，未授权时拒绝；
5. 一个独立 CLI Tool，通过已存在的 `tool.json` 安装并调用公开 Analysis/Trace 合同，不依赖私有 Rust IR。

每项均要求：无 marqdo-cli 文件改动；未重编译 CLI；合法调用通过；未知导出、错误参数、重名、权限拒绝、工件篡改或版本不符都在**可预期阶段**失败且有结构化诊断；`check` **从不加载或运行第三方二进制**。在全部通过前，不得在 README 中写“第三方生态已完全解耦”。

### 0.2 四条不可破坏的不变量

- **S1 单语义**：Markdown AST → `@exec/@knowledge` → H2 函数 → 语句载荷 → IR 由核心定义。`@exec` 是固定核心语言关键字不是需要通用注册的插件。插件只能扩展函数/资源/工具/分析消费者；不允许按项目随意重定义 Markdown 控制流。
- **S2 统一解析**：同一调用点的 `check` 与 `run` 必须绑定同一 `CallableId`；Runtime 禁止重新按名字猜 Builtin/Module/Plugin；任何扩展必须先提供可静态读取的描述符。
- **S3 权限不继承**：加载/依赖/导入/清单声明只表示可见或请求，**不是授权**。所有 I/O 都在真实执行位置进行权限检查和审计。`project`/Worker 不得被宣传为 OS 严格沙箱；现有 `strict L1` 继续拒绝第三方 Worker/Foreign/Provider。
- **S4 低层内核稳定**：普通算法、格式化、AI API、量子门组合、数据处理实现均使用 `.mq.md`、现有基础资源操作或独立 Provider；不得因一个新业务函数修改 `marqdo-ir::builtins::BUILTINS`、`builtins_impl.rs::dispatch`、`HostOp` 或 `resource_request()` 的字符串分支。

### 0.3 本阶段不做

不开发包注册中心、自动网络搜索插件、插件任意宏改写 Markdown、JIT、WASM 执行、跨设备共享裸指针、让未经 OS 约束的 Worker 支持 `strict`、将自定义业务对象不断加到 `Value` 枚举、拆出官方 `stdlib/` 仓库、发布独立的分析/证明引擎。第三方 `@agent/@proof` 只能作为**惰性文档元数据**处理，不成为新的执行语义。新增语言级 `if` 变体、异步调度、原子内存操作、量子计算后端指令须另立语言/核心 RFC，不能借“可扩展”绕过评审。

---

## 1. 架构目标与边界：谁拥有哪一层

### 1.1 分层图

```text
                 任何独立的第三方 Git 仓库
     ┌──────────────────────────────────────────────────────┐
     │ .mq.md 库 │ plugin.yaml │ kernel.json │ provider.json │
     │ tool.json │ 知识/证明/View 的 Analysis/Trace 消费者   │
     └───────────────────────┬──────────────────────────────┘
                             │ 版本化清单 + hash + 显式引用
 ┌───────────────────────────▼─────────────────────────────┐
 │ CLI Composition Root                                      │
 │ 收集 packages / plugin descriptors / kernel / providers   │
 │ 解析锁定身份；校验签名；授权策略；组装 Host                │
 └──────────┬───────────────────────┬───────────────────────┘
            │                       │
    ┌───────▼───────────────────────▼────────────────┐
    │ Markdown AST → Frontend → CallableRegistry      │
    │                         → BoundCallTable → IR   │
    │ 基础语法固定；静态绑定；副作用请求摘要           │
    └──────────────────┬─────────────────────────────┘
                       │ CallableId（不重新猜名字）
    ┌──────────────────▼─────────────────────────────┐
    │ Runtime：值 / 内存 / 控制流 / 执行调度          │
    │ 核心 Builtin | Marqdo 函数 | 已绑定 ABI export   │
    └──────────────────┬─────────────────────────────┘
                       │ 显式可审计的 Host API
    ┌──────────────────▼─────────────────────────────┐
    │ Host：Capability Broker / 资源句柄 / 预算 / Trace│
    │ Plugin Worker │ Foreign Kernel │ Resource Worker│
    └────────────────────────────────────────────────┘

  旁路：CLI Tool 由独立进程执行，仅消费公开 CLI/Analysis/Trace；
        Tool 不获得 Runtime 内存访问权，也不因安装而获得项目授权。
```

### 1.2 明确的职责与禁止依赖

| 层 | 负责 | 绝不能做 |
|---|---|---|
| `marqdo-markdown` | 保真 Markdown AST / Span / 原始标题文本 | 按插件改变 Markdown 语法 |
| `marqdo-expression` | 固定表达式与 `if/else/while/end/return/params` | 识别数据库/量子/AI 的业务函数 |
| `marqdo-frontend` | 语义单元、作用域、签名、静态调用绑定、Analysis | 加载插件二进制；查询 OS 权限；网络搜索库 |
| `marqdo-ir` | 公共内部 `CallableId/Descriptor`、可执行 IR、绑定表 | 硬编码新第三方导出名 |
| `marqdo-runtime` | 只按绑定身份调用、内存和值计算、Trace 语义事件 | 根据字符串 fallback 猜插件、直接拉网络 |
| `marqdo-host` | 最终授权、资源/进程适配、受控注册、审计 | 由未信任清单直接授予权限 |
| `marqdo-package` | 锁定包实例/来源、路径与固定 Git SHA | 根据运行时 `PATH` 猜测代码来源 |
| `marqdo-plugin-abi` / `marqdo-foreign` | 对外 ABI/外语 wire 版本边界 | 把第三方协议当成内部 Rust 对象 |
| `crates/cli` | 组合装载/声明式参数/输出，不含扩展业务逻辑 | 在 `run` 命令中写每个扩展的专用分支 |
| `stdlib/` | 官方 `.mq.md` 高层函数 | 代替权限 Broker 持有真实 OS 权限 |

**依赖方向**：第三方仓库只依赖公开协议、头文件/SDK、`.mq.md` 模块以及 CLI 合同，不得依赖 `marqdo-runtime`、`marqdo-frontend` 或 `marqdo-ir` 私有结构和 `include!()`/`path = ../marqdo-cli` 风格的私有源码。Host 嵌入者可以使用公开 Rust API，但这是另一种产品分发形式；“无需重新编译 CLI”验收专门以命令行黑盒为准。

### 1.3 五类扩展不得合并成一个万能机制

| 类别 | 静态发现 | 执行形式 | 授权 | 典型应用 |
|---|---|---|---|---|
| `.mq.md` Package | `marqdo.yaml` + `marqdo.lock` | Marqdo Runtime | 实际资源 I/O 时 | 数据处理、算法、Agent SKILL |
| Callable Plugin | `plugin.yaml` + 目标 ABI 工件 | 本机动态库或 Plugin Worker | `--allow-plugin` + 实际 Broker | 数值加速、设备 SDK 包装 |
| Foreign Kernel | `kernel.json` + pin 的可执行程序 | 独立子进程 | `--allow-foreign` | 兼容 Julia/R/Python 工具 |
| Resource Provider | `provider.json` + 工件 | Resource Worker / trusted C ABI | `--allow-provider` + resource rights | 新设备、数据库连接、量子设备流 |
| CLI Tool | `tool.json` 安装记录 | 运行独立 CLI | Tool 独立授信；无隐式项目权限 | View、Proof、Knowledge、IDE |

### 1.4 元数据扩展，不引入动态 `@exec`

`@exec`、`@knowledge` 以及 H2 可调用性仍由 `SemanticMode::{Exec, Knowledge}` 决定；第三方不得注册 `@agent` 作为第三种执行状态。第三方库使用现有 Markdown 结构表达元数据，例如：

````markdown
# @knowledge 量子实验

## @exec main

计算代码照常写在语句载荷中，调用 `quantum.run(...)`。

### @knowledge 证明与实验元数据

```json
{"schema":"vendor.quantum.proof","version":1,"obligation":"unit:#main"}
```
````

上例中的 JSON Fence 是**惰性代码资源**，在解析和渲染时不执行；第三方 Tool 通过公开 Analysis/源码读取并解释它。真正的 `@agent` 文本如需展示，可作为普通正文或章节名称，不使用目前会被拒绝的 `@agent` 标题模式。如果未来需要统一元数据 schema，另立 `marqdo.metadata` 描述协议，不修改 `SemanticMode`，也不自动赋予权限。

---

## 2. 当前源代码审计与刷新方式

### 2.1 基线确认的事实、影响、修复入口

| ID | 源码事实（595279f） | 影响 | 本 RFC 处理 |
|---|---|---|---|
| EXT-01 | `marqdo-frontend/src/units.rs` 的 `SemanticMode` 仅 Exec/Knowledge；未知 `@xxx` 报 `SEM_INVALID_MODE_MARKER` | 第三方不能加执行模式；但这是**有意**的语言闭包 | **保留**；仅惰性元数据扩展 |
| EXT-02 | `frontend/src/calls.rs` 的 Member 调用先查 Builtin 再 import；`runtime/src/interpreter.rs` 对同类调用先查 import 再 Builtin | 编译与运行可能命中不同目标 | P0 统一静态绑定 |
| EXT-03 | `ir/src/callable.rs` 已有 `CallableRegistry`，但调用链仍分别使用 `builtins::find` 与运行时 fallback | registry 未成为唯一真相；新 origin 需多处修补 | P0 解析与 dispatch 闭环 |
| EXT-04 | `interpreter.rs` 在无外部描述符时允许未知 `alias.name` 猜作插件调用 | 静态未知符号可能运行时才失败 | 删除 fallback；统一 `symbol.unknown` |
| EXT-05 | `cli/src/commands/run.rs` 从已加载导出补建缺失描述符时，签名曾使用 `params: Vec::new()`，特效请求使用泛化 `host` | 与真实 manifest 签名不一致；验证无法保证 | 禁止补录；静态与动态清单比对一致 |
| EXT-06 | `cli/src/loader.rs` 在加载主模块后才进行 `validate_program`；之后 CLI 再追加静态插件描述符并重复检查 | 错误时序、重复 diagnostics、`check/run` 分歧风险 | P0 把扩展描述符提前注入一次性 finalize |
| EXT-07 | `host/src/host/providers.rs` 支持 Rust/C/Worker Provider，`resource_ops.rs` 提供未识别 kind 的注册分发 | Host 可扩，普通 CLI 尚不能装载/授权 Provider | P0 增加 `--provider/--allow-provider` |
| EXT-08 | `host/src/capability.rs::execute` 目前仅 `fs.read` 具有实作处理，其余已识别能力走 unavailable | 注册了权限名不代表能实际执行 | P1 Handler table 与权限分离 |
| EXT-09 | `host/src/host.rs::resource_request` 有 `process.run` / `regex.*` 等逐字符串特例 | 每加新的非流式资源操作可能改核心 | P1 操作注册合同，现有核心操作保留 |
| EXT-10 | `cli/src/loader.rs` 的 `program_uses_print` 注入 `stdlib/io.mq.md`，Runtime 对 `print` 特判 | 标准库迁移若复制此模式将堆积硬编码 | P1 通用标准库别名绑定 |
| EXT-11 | `foreign/src/lib.rs::KernelRegistry` 已支持注册未知语言；旧 `normalize_language` 只认识 Python/Node | 新语言实际执行路径可扩，但公共帮助 API 可能误导 | 维持 registry 为唯一执行查询、标明 legacy 辅助函数 |
| EXT-12 | Plugin ABI v1 Native/Worker 已实现，Wasm 显式 unsupported；Tool Protocol 已有测试 | 不应重造插件/Tool 平台 | 保留并补签名与完整性契约 |
| EXT-13 | `package/src/lib.rs` 支持 path + pinned Git 及包实例；`stdlib/` 是已验证的内置 bundle | 第三方 `.mq.md` 已有真实源码解耦路径 | 保持；补跨仓库黑盒测试 |
| EXT-14 | 目前 `strict L1` 仅允许受限纯计算与 stdout，Worker/Foreign/Provider 等拒绝 | 项目模式与严格安全模式不能混称 | 保持 fail-closed；L2 另 RFC |

注意：`docs/EXTENSIBILITY_AUDIT.md` 是历史计划，部分曾经的问题已经解决；本 RFC 不可机械复制旧 E01–E16 状态。当前 `.mq.md` 语法参考 `docs/architecture/RFC-LANG-0002-unified-semantics.md` 与 `docs/adr/0052-unified-markdown-semantics.md`。

### 2.2 开发开始前必须运行的静态盘点（允许 HEAD 前移）

```bash
set -euo pipefail
git fetch origin
git checkout -b feat/extensible-ecosystem origin/master
git rev-parse HEAD
rg -n 'enum SemanticMode|heading_declaration' crates/marqdo-frontend/src
rg -n 'builtins::find|external_callable|PluginInvoke|print_override' crates
rg -n 'register_.*provider|allow_resource_provider|resource_request' crates
rg -n 'add_static_plugin_descriptors|validate_program|plugin_exports' crates/cli
rg -n 'KNOWN_CAPABILITIES|fn execute|capability.unavailable' crates/marqdo-host crates/marqdo-plugin-abi
rg -n 'SUPPORTED_LANGUAGES|KernelRegistry|normalize_language' crates/marqdo-foreign
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

填写 `docs/evidence/ext-baseline-<SHA>.md`：git SHA、Rust/Cargo 版本、OS/arch、每个命令退出码、已复现问题、未复现问题、现行 CI run URL。上述命令的成功**不等于**新功能完成，只是基线。若看到本文某项问题已被修复，应标为 `already_closed` 并引用对应测试，不允许重新实现一个平行系统。

---

## 3. 顶层开发顺序、每个 PR 的原子边界

**禁止“大爆炸一次性全部替换”。** 每个 PR 都应能在当前平台上独立构建、通过相关回归测试，且产出一份 evidence 文件。

| PR | 任务 | 主要改动 | 前置 | 可合并门禁 |
|---|---|---|---|---|
| PR-00 | 基线与测试脚手架 | 冲突 fixture、冻结 CLI 黑盒驱动、架构依赖门禁 | 无 | 新测试先红（或者证明已绿），不能仅新增文档 |
| PR-01 | Callable 唯一静态解析 | `ir/callable.rs`、`frontend/calls.rs`、CLI loader/finalize | PR-00 | 同一个 call site 唯一绑定，冲突矩阵全绿 |
| PR-02 | Runtime 只按已解析身份分发 | `runtime/interpreter.rs`、ABI 绑定、删除 fallback | PR-01 | `check == run`、参数/签名完全一致 |
| PR-03 | Resource Provider CLI 闭环 | CLI settings、Provider manifest v2、Worker assembly | PR-00、建议 PR-02 | 外部 Provider 在冻结二进制运行且拒绝越权 |
| PR-04 | Package/Extension 版本与装载归一 | CLI 配置组合根、解析/授权去重、完整性记录 | PR-02/03 | 多包/多插件、多版本、离线闭环 |
| PR-05 | Capability Handler 与操作扩展 | Host Broker + 可注册 operation handler | PR-03 | 未定义 handler 不执行，权限有审计 |
| PR-06 | 标准库别名去魔法化 | `loader.rs`、Builtin/stdlib 绑定层 | PR-02 | print 既有正负例等价，无按函数名扫描的新增特例 |
| PR-07 | 公共 Analysis/Trace 与第三方消费者 | spec/goldens、Tool e2e、外部仓库 CI | PR-01～06 | 外部工具只依赖公共 schema |
| PR-08 | 发布/安全/文档收敛 | README、STATUS、ADR、CI 矩阵、releasing | 所有前置 | §11 所有门禁通过才可 release |

**并行规则**：PR-03 可与 PR-01/02 并行；PR-05 不得与 PR-03 同时引入第二种不兼容 Provider schema；PR-06 必须在唯一绑定稳定之后进行。PR-07 的外部 fixture 框架可在 PR-00 建立，待功能就绪逐一解除 `expected_failure`，最后禁止剩余 `xfail`。

---

## 4. PR-01/PR-02：唯一 Callable 解析、绑定与运行分发（最高优先级）

### 4.1 需要修复的具体调用矛盾

当前 `Expr::Member(alias, name)` 在 `crates/marqdo-frontend/src/calls.rs` 的检查顺序与 `crates/marqdo-runtime/src/interpreter.rs` 的执行顺序不同；`Expr::Ident` 又有局部变量、打印重定向、Builtin、函数的独立优先级。更严重的是，Runtime 在 `program.external_callables` 为空时容忍未声明的 `alias.xxx` 并发往 `PluginInvoke`；以及 `run.rs` 为已加载但未静态描述的 export 合成空参数签名。会造成：静态通过但实际调用另一实现、扩展名冲突不稳定、错误延后到执行时、调用合同不一致。

**红灯测试 A（本阶段先写后修）**：用 `import: { math: "./math.mq.md" }`，在导入模块中实现 `eval`，同时主文件调用 `math.eval(...)`；静态解析和实际运行必须一致。建议的最终行为为**编译时报别名冲突**，而不是静态按内置检查、运行时执行导入函数。**红灯测试 B**：不传 `--plugin` 时调用 `solver.add(a=1,b=2)`，`check` 与 `run` 都必须在编译期返回 `symbol.unknown`；不得尝试 Host PluginInvoke。**红灯测试 C**：插件清单只有 `solver.add(a,b)` 而执行时调用不存在的 `solver.subtract`，必须静态拒绝，不受其他已加载插件数量影响。

### 4.2 新增/调整的数据模型（建议形状；公开 IR 之外的内部实现可微调）

在 `crates/marqdo-ir/src/callable.rs` 补齐只读 `CallableRegistry`；在 `marqdo-ir/src/lib.rs` 或独立 `call_binding.rs` 保存已经决议的调用点。**优先不增 crate**：

```rust
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct CallSiteKey {
    pub module: ModuleId,
    pub function: FunctionId,
    pub span_start: usize,
    pub span_end: usize,
}

#[derive(Clone, Debug)]
pub enum BoundTarget {
    Static(CallableId),
    // 局部变量持有 FunctionRef 等运行时值，此路径只允许语言值调用；
    // 不允许在此路径退化为未知字符串的 ABI 调用。
    DynamicValue,
}

#[derive(Clone, Debug, Default)]
pub struct CallBindingTable {
    pub sites: BTreeMap<CallSiteKey, BoundTarget>,
}

#[derive(Clone, Debug)]
pub struct BoundCallable {
    pub descriptor: CallableDescriptor,
    pub target: CallableTarget, // 内部闭包，不包含裸主机指针
}

pub enum CallableTarget {
    Builtin { canonical_name: String },
    Marqdo { function_id: FunctionId },
    AbiPlugin { package: String, export: String, digest: String },
}
```

这段 Rust 是**实现接口草案**，不是现有代码可直接复制编译的补丁。开发者必须沿用真实的 `FunctionId`、`ModuleId`、`Span` 构造方式，确保现行 `Analysis v2` 与 `Trace v2` 协议不被内部字段变化误改；新增内部绑定表不应默认暴露为公开 JSON。`CallableId` MUST 具有确定性，不依赖磁盘物理路径、注册顺序或随机 `HashMap` 遍历。第三方插件的身份至少包括插件包身份、完整导出名与已验证的工件摘要；包的多版本实例使用锁文件的包实例标识，不能只用显示名。

### 4.3 静态解析优先级：必须明确、可复现，无模糊试探

本阶段采用**显式命名冲突拒绝优先**，优于“恰好哪个先匹配”。

| 调用形式 | 规则 | 允许的例外 |
|---|---|---|
| `f(...)` | 优先本地变量（若为可调用 `FunctionRef`，运行时动态调用）；否则解析未占用的 Builtin 名，再按当前 namespace/可见模块函数查找 | 同名不同 namespace 且未显式限定，报 ambiguity |
| `alias.f(...)` | `alias` 若是显式 import，唯一绑定到该包内函数 | 未声明函数编译错误，不降级成插件/内置函数 |
| `core_alias.f(...)` | `alias` 若与核心内置函数的 dotted namespace 冲突，声明 import 时拒绝，并给出改名提示 | 不允许把内置 `math.eval` 在同一作用域隐式改成导入 `math.eval` |
| `vendor.f(...)` | 既不是 import 别名也不是本地动态值时，仅按静态插件完整导出名查找 | 未加载描述符直接 `symbol.unknown` |
| `some_map.f(...)` | 若 `some_map` 为局部值，在编译期标为 `DynamicValue`，运行时依语言 `Member`/`FunctionRef` 既有规则判定 | 不能作为任意 PluginInvoke 后门 |
| `print(...)` / `打印(...)` | 本阶段保留既有公开打印行为，PR-06 改为通用标准库 alias；先保持所有格式化/Trace goldens | 不因此新增另一打印专属调度路径 |

**名字合法性**：插件导出必须有明确 `namespace.function` 结构且不能使用核心保留前缀；静态表中存在同名 Builtin、模块导出、插件导出时，MUST 在装载/绑定时给出冲突诊断；禁止“last wins”“first wins”以及根据注册顺序遮蔽。可以保留合法局部变量对未限定函数名的覆盖，但这应作为语言变量作用域规则写入规范，不得影响 `alias.f` 的显式静态目标。

**包内导入命名空间**：在加载该模块的前置阶段即可汇总所有显式 import alias、内置 dotted prefix 和可见插件 prefix，冲突按本表拒绝。对于旧文档中恰好名为 `math` 等的 alias，提供错误解释和重命名建议；用户允许 0.x 破坏性语义修正，但无 `--legacy` 兼容开关。第三方实际导出名称不可因为冲突被自动加后缀。

### 4.4 统一装载流程：先描述符，后 `validate_program`

当前 `Loader::load()` 先完成跨模块检查，CLI 后续通过 `add_static_plugin_descriptors` 追加插件并再次调用 `validate_program`。必须改为一次 finalize，不再通过重复验证消除先后错误。

**建议的新代码组织**：

```text
crates/cli/src/
  loader.rs               # 只加载 Markdown / pinned dependencies，保留源位置信息
  extension_descriptors.rs # 读取 Plugin manifests / External descriptors，不执行二进制
  project_prepare.rs       # prepare(file, ExtensionSet) -> PreparedProgram
  commands/check.rs       # 调用 prepare；消费诊断，无 Host side effect
  commands/run.rs         # 调用同一 prepare；核验工件并组装 Host，执行已绑定目标
crates/marqdo-ir/src/
  callable.rs             # descriptor、registry、唯一命名查找
  call_binding.rs         # CallSiteKey/BoundTarget（或等价位置）
crates/marqdo-frontend/src/
  calls.rs                # 由 registry 产生 CallBindingTable + Diagnostic
```

统一流程伪代码：

```rust
fn prepare(file: &Path, extensions: &ExtensionSelection)
    -> Result<PreparedProgram, Vec<Diagnostic>>
{
    let mut loaded = load_module_graph(file)?;              // 只读源码、清单、锁文件
    let external = read_plugin_descriptors(extensions)?;   // 不 dlopen、不 spawn
    let registry = CallableRegistry::build(&loaded, &external)?;
    let bindings = validate_and_bind(&loaded, &registry)?;  // 仅此一次
    Ok(PreparedProgram { loaded, registry, bindings, external })
}

fn run(prepared: PreparedProgram, selection: &ExtensionSelection, grant: &GrantSet) {
    let mut host = assemble_host(grant)?;
    let runtime_exports = load_selected_plugins(&mut host, selection)?;
    reconcile_exact_exports(&prepared.external, &runtime_exports)?;
    execute_bound_program(prepared, host);
}
```

`prepare()` 的全部可执行边界是文件读取、清单解析、静态摘要/工件元数据验证；不得调用 `LoadedPlugin::open_with`、`WorkerPlugin::spawn`、`Foreign.run`、`ResourceProvider.open`、网络 fetch 或任意 shell。静态检查 **MUST** 复用同一份 `PreparedProgram` 绑定逻辑；`inspect` / Analysis emit 也消费它。`run()` 必须在准备成功后才能启动 Worker。`run` 的 Plugin manifest 指向缺失或不匹配工件，应在真实加载前失败且不执行主函数。

### 4.5 插件签名、默认值、参数位置与运行绑定（明确禁止空签名填充）

在 `cli/src/commands.rs::plugin_signature` 的现有签名读取基础上，让 Runtime 拿到**同一份** `CallableSignature`：

1. 静态加载 `plugin.yaml` 时，解析完整参数名称、顺序、必选性，拒绝重名、非法名字、不支持的默认表达式；返回注解如 `-> Int` 当前若仅展示，明确写为 `advisory`，不虚称运行时类型证明。
2. 静态检查时用 descriptor 执行 `validate_args`；除签名明确允许的参数外不得接受未知参数。对于 ABI v1 当前使用 map 传参的事实，支持位置参数时必须先将其确定性映射为签名参数名；若无法做到，必须在静态阶段明确拒绝位置参数，**禁止静态通过而运行时才拒绝**。
3. 推荐本阶段实现一致的位置参数绑定：例如 `solver.add(1, b=2)` 规范化为 `{a:1,b:2}`；任何重复、未知、缺失参数先失败。对于带缺省值的 ABI 参数，v1 首先可直接拒绝在清单中定义默认值，直到语义与 Wire 版本明确，不允许随便在运行时隐式注入。
4. Host `plugin_exports()` 返回的 `name/signature/digest/package` 必须与静态 manifest 集合**精确一一对应**。多出、少了、签名不同、Owner 不同、工件 SHA 不同，统一返回 `ext.plugin_binding_mismatch` 且禁止执行；不允许以 `params: Vec::new()` 合成描述符。
5. Runtime `dispatch` 不得再次 `builtins::find(qualified)` 或以 `external_callables.is_empty()` 作为 fallback 条件；根据绑定结果 `Builtin | Marqdo | AbiPlugin | DynamicValue` 分发。`DynamicValue` 仅对真正的语言可调用值有效。
6. 有多插件时按完整导出名注册，不以 `solver` 字符串前缀挑选“第一插件”；跨插件重名是确定性错误，Worker 与 Native 在签名绑定层无差别。

### 4.6 必须新增的精确测试

新增 `crates/cli/tests/callable_resolution.rs`（黑盒 CLI）与 `crates/marqdo-frontend` 对应单元测试，至少：

| 编号 | 构造 | 期待结果 |
|---|---|---|
| CALL-01 | import alias 与 `math.*` Builtin prefix 冲突 | `check` 确定性编译错误，`run` 不执行 |
| CALL-02 | `math.eval(...)` 无 import 且参数合法 | check 和 run 都绑定 Builtin |
| CALL-03 | `math` 重命名为 `mymath` 并调用 `mymath.eval()` | check 和 run 都调用模块函数 |
| CALL-04 | `solver.add` 无插件描述符 | `symbol.unknown`，无 Host fallback |
| CALL-05 | `solver.add` 在不同数量的无关插件下缺失 | 同一诊断、位置、状态码 |
| CALL-06 | 插件真实两个不同前缀 exports | 两个函数都可静态检查并运行 |
| CALL-07 | 两个插件同名完整 export | 准备阶段冲突；不启动其中任何一个 |
| CALL-08 | 错误参数名/少参数/重复参数/过多位置参数 | check 阶段失败，run 同诊断 |
| CALL-09 | 参数合法但运行时加载的 artifact SHA 改变 | 执行主函数前拒绝 |
| CALL-10 | 清单签名与运行时导出签名不一致 | `ext.plugin_binding_mismatch` |
| CALL-11 | 局部变量是真实 FunctionRef | 可运行，不能转为插件 fallback |
| CALL-12 | 无法静态确定的局部调用目标值是 Text | `not_callable`，不调用 Host |
| CALL-13 | 两个不同 package instance 有同名局部函数 | 按显式导入实例绑定，物理缓存位置无关 |
| CALL-14 | `check --plugin` 时放置不应被执行的插件启动副作用脚本 | 仅读取静态清单；该脚本未运行 |
| CALL-15 | 从 `check --emit` 到 `run` 的绑定身份一致 | 原因与来源可由公开诊断/测试投影核对 |
| CALL-16 | 原有 `打印/print` 标准库行为 | 输出字节、Error、Trace goldens 等价 |

**完成定义**：Runtime 所有静态 call site 都有绑定项；没有由 `qualified` 字符串推测插件的分支；`CallableRegistry::from_program` 不只是一个孤立的数据结构，实际 `check/run/inspect` 都消费同一解析结果；所有上述红灯测试变绿。

---

## 5. PR-03：普通 CLI 的第三方 Resource Provider 完整接入

### 5.1 现状与约束

`StdHost::register_resource_provider`、`register_worker_resource_provider`、`register_c_resource_provider` 和 `register_provider_package` 已存在；`resource.open(other)` 能在 `self.resource_providers` 中按新 kind 找到处理器并检查 `Capabilities::check_resource_provider(name,...)`。这是可复用的正确内核，**不能重新写一个资源运行时**。

但 `crates/cli/src/main.rs` 的 SandboxArgs、`commands/sandbox.rs` 的 `SandboxSettings` 和 `commands/run.rs` 不接受/装载/授权第三方 Provider，因此新仓库无法仅凭 `marqdo run` 使用。采用新增一个 CLI 组合根入口，不新增 `HostOp` 枚举分支、不新增语法关键字。

### 5.2 新 CLI 公共合同（本阶段必须实现）

```text
marqdo run FILE --mode project \
  --provider PATH/TO/provider.json \
  --allow-provider PROVIDER_NAME

marqdo call FILE FUNCTION [named arguments...] --mode project \
  --provider PATH/TO/provider.json \
  --allow-provider PROVIDER_NAME

marqdo check FILE                 # 检查 Marqdo 文档，不加载 Provider
```

- `--provider`：可重复；仅显式注册，无目录扫描；支持相对当前工作目录或绝对 manifest 路径，标准化后必须是普通文件；相对工件路径以 manifest 父目录为根，禁止 `..`/symlink 跳出已声明 bundle。
- `--allow-provider`：可重复；精确匹配经过清单声明和授权的 `name`；不允许 `*` 通配符；仅 `--provider` 不会授权；仅 `--allow-provider` 而无 Provider，不自动寻找或下载。
- `pure/strict`：带 `--provider`、`--allow-provider` 均在 preflight 明确拒绝，不能静默切成 project；`strict L2` 未实装时不放开。
- Provider 名称和 kind 重复、协议版本不支持、worker 程序缺失、库文件 hash 错误、工件目标不兼容，全部在运行主函数前 fail-closed；不得回退为 Native。
- `check` 默认不解析 Provider 作为新的函数语义来源；Provider kind 是 `resource.open` 的运行参数，不自动添加全局 Marqdo 函数。

### 5.3 Provider manifest：为新 CLI 入口定义 v2，保留 v1 显式历史边界

现有 ADR-0037 的 `marqdo.provider` **schema_version=1** 只有 `name/kinds/abi/library`，缺少可移植的目标工件摘要与版本字段。本阶段新增 **schema_version=2**（仍使用 `provider.json` 文件名），**CLI 扩展装载只接受 v2**；旧 v1 可以继续通过既有显式 `StdHost::register_provider_package` 供可信嵌入者使用。不得静默把 v1 当 v2，也不得伪造它有工件校验。新的 `spec/resource-provider-v2.md` 必须完整定义下面的字段和拒绝条件：

```json
{
  "schema": "marqdo.provider",
  "schema_version": 2,
  "name": "demo.sensor",
  "version": "0.1.0",
  "abi": "c-abi-1",
  "transport": "worker",
  "kinds": ["demo.sensor"],
  "artifacts": [
    {
      "target": "x86_64-unknown-linux-gnu",
      "path": "lib/libdemo_sensor.so",
      "sha256": "<64-lowercase-hex-sha256>"
    }
  ],
  "permissions_requested": []
}
```

上述 `sha256` 是**占位符**，构建时必须替换为真实二进制摘要；目标三元组须和 `marqdo_plugin_abi::HostTarget::current().triple()` 的结果一致。v2 解析规则：

1. `schema`/`schema_version` 必须恰好匹配；`name` 与每个 `kind` 必须非空、符合允许的分段命名语法（采用 Plugin 名称校验中的 ASCII 小写段 + `.`；若允许 Unicode 名称须另立 ADR）；保留 `stdio/file/tcp/tls/udp/tcp-listener` 等核心 kind，禁止第三方覆盖。
2. `version` 为严格 SemVer；`abi` v2 初期只支持 `c-abi-1`；`transport` v2 初期只支持 `worker`。未来新增 transport 必须明确增加枚举与测试，不自动解释未知字符串。
3. `artifacts` 必须非空，不能重复 target；选择当前目标后校验普通文件、bundle 内位置、大小上限、SHA-256 精确相符；目标缺失返回 `ext.no_matching_artifact`，不使用 `default` 的其他平台二进制替补。
4. `permissions_requested` 为**声明**，不授予权限；v2 `open/read/write/close` 只按 `--allow-provider` + rights 受控，不能因为声明了 `vendor.*` 就给 Host 新权限。
5. schema 版本内未知的安全影响字段拒绝；可扩展元数据统一放入 `metadata` 映射且不具有执行意义。manifest 自身保存为 UTF-8 JSON。相对路径解析前阻止 `..`、绝对路径、符号链接逃逸；绝不将内容拼接给 shell。
6. 加载前验证完整 manifest + artifact；若要强制工件真实性，交由发布签名/信任根或用户固定的期望摘要额外处理。SHA-256 是**完整性**检查，不是来源身份认证，文档不得混淆。

### 5.4 精确落地的代码修改

1. `crates/cli/src/main.rs`：为 `SandboxArgs` 增加 `provider: Vec<PathBuf>`（CLI 名 `--provider`）和 `allow_provider: Vec<String>`（CLI 名 `--allow-provider`）。两个选项同用于 `run/call`，不得在 `check` 阶段启动 Worker。
2. `crates/cli/src/commands/sandbox.rs`：为 `SandboxSettings` 加同名字段；`capabilities()` 的 `Project` 分支对每个名称执行 `allow_resource_provider(name)`；`Pure`/`Strict` 的拒绝条件均包含两个新字段；`strict_preflight()` 输出固定、结构化错误码。
3. 新建 `crates/cli/src/provider_manifest.rs`（而非新大 crate）：解析 v2；校验语法、artifact、sha256、target；产出 `VerifiedProvider { name, kinds, selected_artifact, transport, manifest_digest }`；不要与 Plugin `plugin.yaml` 共享错误的 ABI 解析器。
4. 新建或复用 `commands::load_resource_provider_into_host(...)`：先验 manifest 与工件，确认 `--allow-provider NAME` 存在；调用**已有** `StdHost::register_worker_resource_provider(name, kinds, worker_program, [artifact_path])`；Host 最终对 resource.open 再次授权；失败不得尝试注册受信 Native。
5. Worker 可执行程序解析：使用发行包中和 `marqdo` 同目标三元组的 `marqdo-provider-worker`（从当前 CLI 可执行文件的兄弟目录解析），拒绝 PATH 任意同名可执行文件回退；发行归档必须一起交付、记录摘要。若某平台尚未支持，返回 `ext.platform_unsupported`，不可伪装成功。
6. 在 `run/call` 的通用 `assemble_host` 中加载 kernels、Callable plugins、resource providers；为各类型使用相同的可重复、确定性错误输出规则，但保持不同执行协议。加载顺序严格：parse + compile → 校验扩展唯一性与工件摘要 → 验证授权 → 初始化必要的 Worker → 执行主函数；无语义副作用的检查绝不 spawn。
7. `StdHost` Provider 注册 API 的代码重复部分要复用 `register_provider_checked`；保障按**完整 kind**索引，重复 kind 跨 Provider 不得覆盖；单个 Provider 同时导出两个 kind 都必须可用。
8. 结束 Session 时关闭所有 Provider tokens 并终止对应 Worker；单次 Worker crash 要转结构化错误，不能让 CLI panic；禁止在资源使用中断后静默回退其他 Provider。

### 5.5 可复制的 Provider 调用 fixture（现有语法）

`index.mq.md`：

```markdown
# @knowledge 设备测试

## @exec main

打开独立仓库提供的资源；此标题仅决定文档语义，不涉及 Provider 注册。

`流 = resource.open("demo.sensor", "unit-1", ["read"])`

`内存 = memory.new(1, 1)`

`读取 = resource.read(流, 内存, 0, 5)`

`关闭 = resource.close(流)`

`if not 读取.ok`

`return 读取`

`else`

`if not 关闭.ok`

`return 关闭`

`else`

`if true`

`return 读取.value.count`

`end`

`end`

`end`
```

配套 fixture Worker `open` 返回独立 provider token；第一次 `read` 返回字节 `hello`，`eof=true`，返回 count=5；`close` 必须仅发生一次，运行结果为整数 5。注意 Resource 与 Memory 均为语言层不透明句柄，不可从 Wire 随意反序列化。有关 buffer 长度、权限及 Result 结构使用仓库真实 `stdlib/fs/file.mq.md` 现行调用约定，不应发明新的 `resource.read(bytes=...)` 形态。

### 5.6 Provider 测试矩阵

| ID | 输入 | 预期 |
|---|---|---|
| PROV-01 | 外部 v2 Worker + grant + read 5 bytes | 返回 5，stdout 不变，Host Trace 有 resource effect |
| PROV-02 | v2 Worker，无 grant | `permission.denied`；不得先启动未授权 Worker |
| PROV-03 | 只 grant，不提供 manifest | `resource.unsupported` 或装载诊断，不自动发现 |
| PROV-04 | 同 kind 不同 Provider | 准备阶段冲突，不能“后注册覆盖” |
| PROV-05 | 同一 Provider 有两个非保留 kind | 两个 kind 分别可 open/read/close |
| PROV-06 | `kind="stdio"` / `kind="file"` | 注册拒绝，不可覆盖核心资源 |
| PROV-07 | digest 不匹配、symlink 外逃、路径 `../` | spawn 前拒绝，安全错误稳定 |
| PROV-08 | 缺少当前 target | `ext.no_matching_artifact` |
| PROV-09 | v1 传入新 CLI | `ext.unsupported_provider_schema`；无 silent migration |
| PROV-10 | `pure` 与 `strict` 带 Provider 参数 | preflight 拒绝、无 Worker |
| PROV-11 | Worker 崩溃/超时/返回超大帧 | Host 结构化错误，CLI 不 crash，不退回 Native |
| PROV-12 | token 关闭后读、跨 run 重用、伪造值 | fail-closed，不能泄漏其他资源 |
| PROV-13 | 无 `marqdo-provider-worker` 同目录 helper | 清晰报缺失，不 PATH fallback |
| PROV-14 | 多次并发开启/退出同 Worker（以当前同步 API 测试） | 生命周期/句柄隔离，关闭幂等性按现有 API 规范 |
| PROV-15 | 外部仓库安装并在冻结 CLI 上跑 | 哈希证明 CLI 二进制未变，源码无改动 |

说明：当前 C ABI Worker 相关 fixture 可能仅覆盖 Unix 平台。新增 v2 Provider CLI 首先在 Linux x86_64 做强制门禁；macOS 如支持则同门禁；Windows 保持可编译且显式 unsupported，直到有独立 native fixture 和真实 C/Worker 证据才标 `supported`。**不得因为 Linux 通过就宣布全平台支持**。

---

## 6. PR-04：Package 与 Extension 组合根——独立仓库可以发布，使用端显式激活

### 6.1 为什么现有 Package 机制基本正确，但尚未完全闭合

`marqdo-package` 的 `Dependency::{path,git,rev}`、`ResolvedPackage`、`PackageInstanceId + relative_path`、lock v3 与 Git cache 已经形成普通 `.mq.md` 仓库之间的解耦基础。**不得为“插件系统”重新实现一个隐藏的 Marqdo 源码包管理器。** 当前的缺口是：项目 `marqdo.yaml` 里存在 Plugin 版本需求，但 CLI `--plugin` 路径、`--kernel`、计划中的 `--provider` 与 Package 锁身份未形成清晰、一致的装配事实表。当前只能靠开发者命令行重复传路径；若从其他独立仓库部署，容易出现 descriptor/执行工件不同步。

### 6.2 唯一 Project Extension Selection 结构

建议新增内部 `ExtensionSelection`（放在 CLI 组合根而非新 crate）：

```rust
pub struct ExtensionSelection {
    pub plugins: Vec<ResolvedPluginManifest>,
    pub kernels: Vec<ResolvedKernelDescriptor>,
    pub providers: Vec<ResolvedProviderManifest>,
}

pub struct ExtensionIdentity {
    pub kind: ExtensionKind,    // plugin | kernel | provider
    pub logical_owner: String,  // 来自当前已锁定包或当前项目
    pub name: String,           // 包/Provider 稳定声明名
    pub version: String,        // 若本协议支持版本
    pub manifest_digest: String,
    pub artifact_digest: String,
    pub selected_target: String,
}
```

每一种现有扩展都保留各自的解析器与 ABI；`ExtensionSelection` 只负责把“谁、版本、摘要、来源、用户启用”统一成可审计事实。相同完整身份可去重；同名不同 digest、一个 kind 被两个 Provider 声明、同名 Plugin Export 重复，都必须给出确定性冲突。**绝不能**按目录遍历顺序决定使用哪个实现。

### 6.3 后续 `marqdo.yaml` 可声明扩展激活源，但绝不自动授权

PR-04 把下列语法作为唯一新增的项目 manifest 结构（须更新 `crates/marqdo-package/src/lib.rs::Manifest`，写规范与 round-trip tests）。此前已有 `plugins: {name: {version: ...}}` 的需求字段不得被悄悄解释为可运行工件；若保留，就明确仅为“需求”，不用于隐式选择；建议后续一次性迁移到下面的显式扩展选择结构：

```yaml
project:
  name: research-demo
  version: 0.1.0

dependencies:
  quantum:
    git: "https://github.com/EXAMPLE/marqdo-quantum.git"
    rev: "<完整固定的 Git commit SHA>"

extensions:
  plugins:
    - from: "@quantum/adapter/plugin.yaml"
  kernels: []
  providers:
    - from: "@quantum/adapter/provider.json"
```

这是**新提议的 manifest 合同，基线版本尚不存在**。`from` 值仅允许两种形式：相对项目目录的 `./path`，以及显式 `@alias/relative/path`（alias 必须属于当前已锁定依赖闭包）。不得支持任意 HTTP URL、未固定 Git 分支、主机 PATH 中任意搜索、从 import 文本隐式发现扩展。每个 `from` 解析为包内普通文件，文件必须在所属包文件集/锁定的源码快照内；manifest/artifact 摘要都记录在新锁版本中，不能仅记录路径。

**锁文件升级**：当前 lock v3 是公开有序依赖图。若加入选定的扩展 manifest/artifact identity，升级为 **lock v4**，同时更新 lock spec、读写器、goldens、构建 fixture、离线验证和迁移文档；不允许 v3 中凭空塞入同名字段而仍声称 v3 兼容。对于不涉及扩展的旧 lock v3，可选择继续只读支持，或者给出确定的迁移命令；迁移结果必须显式写入新 v4，不实现隐藏的语言兼容模式。`marqdo sync` 可物化已锁定内容但**不自动授权或执行其中二进制**。

### 6.4 组合根配置优先级和安全规则

1. **声明**：`marqdo.yaml.extensions` 是项目明确启用的候选扩展来源；`--plugin/--kernel/--provider` 是本次运行临时追加来源；两个来源若相同逻辑身份且摘要一致则幂等，否则拒绝冲突。
2. **锁定**：`sync`/`lock` 负责把可复现的来源、包 SHA、manifest 内容 SHA、目标工件 SHA 记录到 lock；运行只消费已经锁定的物化源，不调用网络下载。测试中的临时显式 CLI 路径如果不在 project lock，可作为本机开发/调试形式使用，但输出 `unlocked_extension` 标记；发布模式不接受未锁定扩展。
3. **验证**：`check` 使用静态可读 descriptor + lock identity；`run` 在启动前重新核对所有实际工件，不能用 `check` 的旧哈希替代当前校验。
4. **授权**：`--allow-plugin`, `--allow-provider`, `--allow-foreign` 与文件/网络/进程等 Host grant 是独立权限；即便依赖包请求权限，Host 默认不授予。
5. **执行**：ABI Plugin、Kernel、Provider 使用各自的明确路径，不能让 `Tool` 充当偷偷运行库代码的 shell 执行后门；`Tool` 仍在独立 CLI 进程域。
6. **失败**：未解析 alias、未锁定依赖、摘要不匹配、无授权、缺当前平台工件均 fail-closed；不可自动退到最新版本或某个系统安装。
7. **可观测**：`inspect --extensions --json`（为现有 inspect 增加 flag，不新建顶层命令）列出来源、类型、完整身份、选中工件、验证状态、授权状态；**不泄漏密钥/环境变量**。

### 6.5 与多版本依赖、相对路径、SDK 的关系

- 同一第三方包 v1/v2 可以在依赖图中同时存在，因为 `ModuleId` 由 package instance + package-relative path 决定；但调用者导入必须显式选定 alias，不靠“最新”或搜索顺序。
- 插件的完整导出名在单个程序执行空间是全局唯一的；两个版本都导出 `solver.add` 时，除非显式定义不同的对外 alias，否则拒绝。扩展 alias 功能若未实现，就以冲突错误交付本阶段，不实现悄悄加后缀。
- 工件在物理 cache 中移动不应改变逻辑身份；同摘要不同来源的信任处理与 lock 身份是独立概念，不能单凭 hash 判定 publisher。
- 第三方 SDK 可在其仓库使用已发布的 ABI header、Plugin SDK、Wire schema；**不能**通过直接修改 `marqdo-cli/crates/...` 来提供扩展实现。
- 发行物必须声明支持平台矩阵；未知架构不是“试试 Linux 的 so”，而是明确拒绝。

### 6.6 PR-04 验收测试

`PKG-01` 同一项目通过固定 Git SHA 的 `.mq.md` 依赖调用成功；`PKG-02` 离线 cache 一致、无联网仍可运行；`PKG-03` cache 移位不改 logical ModuleId；`PKG-04` 同名不同版本在不同 alias 下调用不混淆；`PKG-05` 未固定 Git rev 不允许；`PKG-06` 锁定工件被修改必拒绝；`PKG-07` `extensions` 声明而无 grants 时不可执行 Provider/Plugin；`PKG-08` CLI 与项目双重选中同摘要幂等、不同摘要冲突；`PKG-09` 扩展依赖的 `@alias` 无法逃离所属包范围；`PKG-10` 恶意 `../`、symlink、非普通文件、超大 bundle 被拒绝；`PKG-11` lock v3/v4 版本切换有确定错误或显式迁移；`PKG-12` `check` 无插件初始化副作用。

---

## 7. PR-05：Capability Broker 与非流式资源操作的长期扩展边界

### 7.1 把“能声明能力”与“能执行能力”彻底分开

现有 `KNOWN_CAPABILITIES` 包含 `fs.read/fs.write/net.request/process/clock/random/...`，允许带命名空间的 `vendor.*`；但 Broker 实际 `execute()` 只实现了受控 `fs.read`。因此不得只在 `KNOWN_CAPABILITIES` 或 `CapabilityBroker::decide` 加一行就宣称新增服务已接通。

统一权限链必须满足：

```text
extension manifest requires capability
    └→ parser 只接收声明，校验标识合法
      └→ CLI / operator 明确 grant + 有效范围
        └→ CapabilityBroker 查受信任 Handler 是否安装
          └→ 实际调用再核对 owner / grant / 作用域 / budget
            └→ Handler 执行受控操作
              └→ Audit { identity, effect, decision, duration, trace_id }
```

核心预置能力是有限的基础系统原语；Vendor 能力是有命名空间的申请，**不能**跳过 Broker 直接变成特权的宿主调用。第三方项目只修改 manifest 不意味着可在 Host 注册特权 Handler。

### 7.2 处理器注册模型（Rust Host 内部可扩，不再增加长 match）

为 `marqdo-host` 的 Broker 增加显式处理器目录。最小建议接口：

```rust
pub struct CapabilityRequest<'a> {
    pub package_instance: &'a str,
    pub capability: &'a str,
    pub payload: &'a [u8],
    pub trace_id: &'a str,
}

pub trait CapabilityHandler: Send {
    fn name(&self) -> &str;
    fn execute(&mut self, request: &CapabilityRequest<'_>,
               policy: &EffectiveGrant,
               budget: &mut ResourceBudget)
        -> Result<Vec<u8>, CapabilityError>;
}

// Host/Broker 中注册，由受信任的组合根调用：
// register_capability_handler(Box<dyn CapabilityHandler>).
```

`EffectiveGrant` 与 `ResourceBudget` 在此为设计接口名，实施时应复用 `Capabilities`、现有时限/字节限制结构，而不是复制一套权限系统。Handler 注册必须拒绝同名、保留名前缀、未受信任来源以及声明协议不符。既有 `fs.read` 改为标准 Handler；其他核心能力可以逐步迁移，但**没有真实执行处理的能力继续返回 `capability.unavailable`**，不能只给空字节当“成功”。`decide` 不得把 vendor 能力字符串存在 allow-list 就当作 Handler 已安装；授权通过后仍需 Handler 存在才能执行。

### 7.3 第三方非流式 API 推荐怎样在 CLI 外完成？

**第一优先级：在独立 `.mq.md` 库中使用既有 `resource.open/read/write/close`。** 例如数据库查询、设备指令、量子线路提交可以将请求序列化成有限字节写入 Provider stream，再读回 Result；这是现阶段完全不用新增 `HostOp` 的扩展路径。请求/响应二进制协议由第三方库和 Provider 自己版本化，Host 只管 handle/budget/rights，不解释 SQL 或量子指令。

**第二优先级：针对确实不可映射为 stream 的系统原语**，可通过受控 `resource.request` 的 Operation Registry 追加可信操作。新增 `OperationDescriptor { public_name, args_schema, result_schema, capability, handler_id, version }` 与 `BTreeMap<String, Handler>`；核心 `process.run`、`fs.*`、`regex.*` 等可以逐步整理为内置 Handler，但这**不是让任意 `vendor.*` 清单直接注入 Rust 回调**。第三方仅用 CLI 的 Provider v2 stream 即可开发；需要直接注册 Operation Handler 的场景属于可信 Host embedder 能力，不在本阶段 `--provider` 对外暴露的 ABI 内。

**不得承诺**现有 Resource Provider ABI v1 已经能执行任意新 `resource.request("vendor.op", args)`；实际上它只保证 `open/read/write/close`。若未来把 Operation 暴露给 ABI Worker，应另立版本化 ResourceOperation ABI v2，明确 wire、预算、效果、支持平台和迁移测试；不能在 v1 的 Worker 消息里悄悄塞一个 `request` 并要求所有现有实现兼容。

### 7.4 安全检查点（必须覆盖）

- 受控 `fs.read` 依然受 `ProjectFs` 根路径限制、glob 和最大输出字节数限制；不能调用裸 `std::fs::read` 绕过策略。
- 时间、熵、进程、网络等能力分别有实际 Host 授权与执行路径时才可宣称支持；权限 Handler 的审计记录必须体现 `denied/unavailable/granted`，不能误把“允许声明”当“已执行”。
- Capability request payload 的 JSON/字节长度、嵌套深度、编码、错误码、响应上限受既有预算约束；无界缓冲、递归调用、异常退出必须可恢复。
- PluginHandle / ResourceHandle 由 Host/Runtime 签发，wire 输入不能凭 `id` 字段重建高权限 handle；跨运行、伪造、释放后重用必须拒绝。
- 每次调用都绑定 owner package、已选择工件 digest、运行会话/trace，不能信任插件自报的 package 名和 `capability` 字符串就越权。
- Worker 进程隔离并非 OS sandbox：恶意二进制仍可能直接访问其同用户 OS 权限。正式支持不可信 Provider 必须另行开发 strict L2，并完成真实系统调用负例后才开放；当前明确标 `trusted worker`。

### 7.5 必须有的 tests

`CAP-01` 仅声明 vendor 权限但未授权 → denied；`CAP-02` 授权但无 Handler → unavailable；`CAP-03` 有 Handler 但跨 package 冒名 → denied；`CAP-04` 注册同名 Handler → conflict；`CAP-05` Handler 读超范围文件 → denied；`CAP-06` 返回超过预算 → resource limit；`CAP-07` Worker 返回伪造 PluginHandle/ResourceHandle → rejected；`CAP-08` 卸载/关闭/新 run 后重放旧 handle → rejected；`CAP-09` 审计不含文件原文/密钥；`CAP-10` resource 请求低层 `open/read/write/close` 性能不劣化到不可接受；`CAP-11` 默认 `project` 不得被标记为 OS strict；`CAP-12` 没有 Handler 的核心能力始终 `unavailable` 而非 silent OK。

---

## 8. PR-06：官方标准库函数绑定去魔法化，而不是让每个新库都改内核

### 8.1 现有特殊行为的边界

`cli/src/loader.rs::program_uses_print` 遍历整个 AST 检测 `打印/print`，然后载入 `stdlib/io.mq.md`，把 `写值` 指定成 `print_override`；`runtime/interpreter.rs` 对 `打印/print` 特判。为了在迁移期维持完整的 Unicode/复杂值格式化，这种 Compat-C 是合理过渡；但将其复制到其他高级函数会破坏扩展性。

### 8.2 最小替代方案：官方标准库公开别名描述符

- 保留受核心保护的公开名字 `打印/print`；不允许第三方抢占。
- 在官方 stdlib bundle 的版本化元数据中定义 `PublicAlias { public_name, target_module, target_function, expected_signature }`，例如 `打印` 与 `print` 都指向 `std:io::写值`。此元数据与 `stdlib.json` 的内容摘要一起封装、校验，并与 `CallableRegistry` 的核心预置描述符合并。**不是**运行时根据 `if name == "print"` 重定向。
- `CallableRegistry` 解析到官方 `PublicAlias` 时，`Loader` 按需加载官方已验证 `io.mq.md` 并生成普通 `FunctionId` 绑定；`Runtime` 只执行 `Marqdo Function` 目标，不对打印名称特殊处理。
- 别名元数据只能来自官方已校验 stdlib bundle，不接受第三方伪造 `core.*` 或 `std.*` 名称；通用第三方导入正常以 package alias 暴露，不能修改核心全局函数别名。
- 删除 `program_uses_print` / `stmts_use_print` / `expr_uses_print` 及 `Program.print_override`（在回归测试全部通过后）；不保留 `--legacy-print` 或双调度。必要时以单独 ADR 记录打印语义从特殊覆盖到通用绑定的实现迁移，**输出、权限和错误行为必须不变**。
- 不重新引入 Host `Print` 操作；仍然通过 Marqdo 标准库 `format.显示`、`io.写值` 走 `resource.write(stdout)`，保持纯计算与 I/O 的界限。
- 只因某个公开别名被真实引用时才加载目标模块；无打印调用的程序启动成本、模块图、Analysis 不应增长。

### 8.3 对外部第三方库的影响

第三方 `marqdo-ai`、`marqdo-quantum`、`marqdo-proof` 不用添加官方标准库别名就能运行。它们通过 `import` 声明可调用函数，或者 `plugin.yaml` 声明加速入口。官方 bundle 的内部公开别名表仅用于语言标准 API 迁移；**不是**所有第三方必须申请核心名字的注册表。

### 8.4 PR-06 退出标准

至少运行现有打印全值类型测试、格式化 goldens、stdout/stderr 隔离、`pure`/`project`/`strict` 授权测试、`--json` 输出与 Trace goldens、缺 bundle 报错、未使用打印时不加载 `io`、两个不同包均可正常使用全局打印、与本阶段 Callable 冲突规则一致。打印 byte-for-byte 输出差异为阻塞；禁止以“新设计更合理”删除现行正常使用行为而不经 ADR 批准。

---

## 9. PR-07：View / Proof / Knowledge / Agent 等其他仓库只消费稳定边界

### 9.1 独立第三方仓库的合法依赖方式

`marqdo-view`：使用 `marqdo check FILE --emit ANALYSIS.json` 和 `marqdo run FILE --trace-file TRACE.json`，再以原 Markdown 源文档渲染；严禁把私有 AST/IR Rust 结构链接到 View 中。`marqdo-proof`：消费 Analysis、锚点/引用/函数符号、源码摘要，产生自己的证明义务和证明证书；证明结果不能直接改变 Runtime 权限。`marqdo-knowledge`：从普通 Markdown AST/source + Analysis 收集文档知识锚点，索引结果由自身管理。`qdagent`：根 SKILL/聊天 SKILL 等正常 `.mq.md` 文档调用外部算法模块与 Provider，不应要求 CLI 增加 `agent` Builtin 或专用 `@agent` mode。

### 9.2 公开协议稳定性规则

1. CLI 的 Analysis、Trace 与版本化数据输出是正式的消费者边界。私有 IR（包括新 `CallBindingTable`）不要求跨发布稳定；Analysis 公开 schema、版本字段、Unit/Module identity、source span、源码 SHA、声明导出和 Call target 信息如已有字段不可悄悄改义。
2. 如确实要对外暴露 `resolved_call_target`，新增 `Analysis v3` 或合法的向后兼容可选字段，并提供 **v2→v3** 迁移说明与 golden；不能把内部 `CallableId` 直接裸序列化到现有 Analysis v2 并假定消费者无感。
3. Trace 的 `call/return/branch/loop/host-effect` 有可关联来源标识，密钥、完整网络 payload、敏感 argv 默认不输出；运行时关闭 Trace 时，不能因构造 Trace 文本导致显著性能损失。
4. `check --emit`、`inspect` 必须完全无执行副作用；`view/proof/knowledge` 通过已注册 Tool 执行时也不能偷偷获得项目 grants。
5. Tool 的子进程如需运行项目，必须显式启动新一轮 `marqdo run` 并重新按公开 CLI 合同获得授权；Tool 安装不等于运行权限。
6. Unknown JSON fields 和 schema version 应按对应公共规范处理：版本不兼容拒绝，允许的额外字段忽略，不能把所有未知字段当语义指令。

### 9.3 第三方证明消费者验收（无需实现证明内核）

在外部 fixture 中实现一个不到 100 行的 Python/Go/任意语言消费者：仅用标准 JSON 库读取 `check --emit` 的 Analysis、列举函数及其源位置，并输出对某个 heading anchor 的确定性摘要。再用独立 Tool Protocol 安装到同一冻结 CLI：

```bash
marqdo tool install ./fixtures/tool-inspect/tool.json
marqdo inspect-ext --help
marqdo check examples/factorial.mq.md --emit /tmp/analysis.json
```

上面的 `inspect-ext` 为 fixture 中自行声明的 Tool command，非核心内置命令；调用 `Analysis` 的具体格式以当前 `spec/analysis-v2.md` 为准。完成标准为：独立 Tool 在无 Rust workspace 依赖情况下消费 Analysis；更新核心私有 IR 而不改 Analysis schema 后，consumer goldens 不变；插件缺失或未授予运行权限时，Tool 不会因为已安装就启动任意项目。

---

## 10. 外部仓库开发模板与可复制黑盒脚本

### 10.1 推荐第三方仓库目录（与 marqdo-cli 物理分离）

```text
marqdo-extension-conformance/             # 独立 Git 仓库，不加入 marqdo-cli workspace
  README.md
  CONTRACT.md                             # 使用的 Marqdo/ABI/Wire 版本
  .github/workflows/conformance.yml
  pure-lib/
    marqdo.yaml
    index.mq.md
  consumer/
    marqdo.yaml
    index.mq.md
  plugin/
    plugin.yaml
    solver.c
    build.sh
  kernel/
    kernel.json
    mocklang                         # 独立可执行程序
    demo.mq.md
  provider/
    provider.json
    sensor.c
    build.sh
    demo.mq.md
  tool/
    tool.json
    inspect_ext.py
  tests/
    verify_frozen_cli.py
    test_positive.py
    test_negative.py
  evidence/
    binary-sha256.txt
    results.json
```

真实业务仓库 `marqdo-quantum` / `marqdo-ai` 可以只拥有其中一类或多类扩展，而不是每个仓库都必须复制全部结构。`pure-lib` 的例子：

`pure-lib/marqdo.yaml`：

```yaml
project:
  name: example-math
  version: 0.1.0
```

`pure-lib/index.mq.md`：

```markdown
# @exec 示例数学库

## 加倍

`params(x: Int)`

`return x * 2`
```

`consumer/index.mq.md`（消费者通过显式 alias 导入外部依赖）：

```markdown
---
import:
  m: "@math"
---

## @exec main

`return m.加倍(21)`
```

`consumer/marqdo.yaml` 示例（本地构建时使用 path；完成测试后替换为已提交的固定 Git revision）：

```yaml
project:
  name: example-consumer
  version: 0.1.0
dependencies:
  math:
    path: "./vendor/pure-lib"
```

上述本地 path fixture 要先将外部库的**已固定版本快照**复制到 `consumer/vendor/pure-lib`，它只是本地依赖功能测试。真正的**跨仓库解耦**黑盒必须把第三方 `pure-lib` 独立提交为 Git repo，并使用经允许的 `git:` 源形式和**完整固定 SHA**，不能靠把外部仓库源码直接纳入 CLI Workspace；任何 `../` 逃出项目根的路径都应保持拒绝。

### 10.2 冻结 CLI 验收：不能在安装第三方扩展后重编译核心

在 marqdo-cli 根目录先生成一个独立的**候选发行二进制**；构建出来后所有外部扩展脚本只接受该二进制的**绝对路径**，禁止 `cargo run`、`cargo build -p cli` 或把第三方源码复制入 `crates/`：

```bash
set -euo pipefail
cargo build --release -p marqdo-cli
CLI="$(realpath target/release/marqdo)"
sha256sum "$CLI" > /tmp/marqdo-frozen.sha256

# 此后转到完全独立的外部仓库工作目录
cd /path/to/marqdo-extension-conformance
export MARQDO_BIN="$CLI"
python3 tests/test_positive.py
python3 tests/test_negative.py
sha256sum -c /tmp/marqdo-frozen.sha256
```

以上只是执行器命令草案：开发者必须在对应 OS/包中根据真实 `Cargo.toml` 的 binary 名称验证 `cargo build --release -p marqdo-cli` 可用；如果当前 Cargo package 名不同，使用实际 Cargo package 名，但不得改变“冻结并校验同一二进制”的要求。外部套件必须记录其 Git SHA、CLI SHA、Host target、插件 Provider SHA、每条测试结果、错误码与最小可复现命令。每个文件由 fixture 自己构建，不直接消费核心源码目录中的测试 fixture。

### 10.3 运行侧四类典型命令

```bash
# 1. 普通外部 .mq.md 包：
marqdo sync                         # 物化 pinned Git dependencies
marqdo check consumer/index.mq.md
marqdo run consumer/index.mq.md

# 2. Callable Plugin：沿用现有支持的 flags：
marqdo run plugin/demo.mq.md \
  --plugin plugin/plugin.yaml --allow-plugin demo.solver

# 3. Foreign Kernel：沿用现有注册方式：
marqdo run kernel/demo.mq.md \
  --kernel kernel/kernel.json --allow-foreign

# 4. Resource Provider：PR-03 完成之后的新 flags：
marqdo run provider/demo.mq.md \
  --provider provider/provider.json --allow-provider demo.sensor
```

这里 `--provider` 和 `--allow-provider` **当前尚不存在**；`marqdo.sync` 的运行目录必须拥有正确的 project manifest。命令是预期验收契约，开发者须随本 RFC 一并实现；不应误把示例作为已验证的现行命令。

### 10.4 不依赖网络的 Git 固定版本 fixture

外部测试可在临时目录中：`git init` 一个纯 Marqdo 库，`git -c user.name=Fixture -c user.email=fixture@example.invalid commit`，读取完整 SHA，生成消费者的 `marqdo.yaml` 用 `git: file://...` 且 `rev` 固定 SHA，运行 `marqdo lock` / `sync`。随后删除网络访问和源工作目录，仅保留已校验缓存，再运行 `check/run`；证明包源与运行时分离。在 Windows 使用平台合法的 file URL 或本地 Git 路径形式，不能假定 Unix `file:///tmp` 永远成立。若当前 Git resolver 限制 `file://`，先加入明确可控的测试源适配或使用完全离线的本地 bare Git repo，并为新的 SourceResolver 写安全 ADR，而不是临时启用未经审计的任意 URL。

### 10.5 外部生态必须提供的 `CONTRACT.md`

逐项列出：需要的 Marqdo Core 版本范围、语言语义 RFC、ABI/Wire/Analysis/Trace 版本（只列实际使用的）、选用的 target triples、manifest/schema 版本、外部依赖 SHA、如何计算工件 hash、是否需要 Host grants、默认运行模式与对未知平台/缺权限的行为。不要写“最新版本均兼容”，不要隐式依赖系统已安装 Node/Python/USB 驱动。需要的运行时通过 Kernel/Provider 的 manifest 明确声明，缺环境时 fail-closed。

---

## 11. PR-08：回归、CI、性能、安全与发布硬门禁

### 11.1 测试分层：任何红灯都不能以“文档解释”掩盖

**L0 单元测试**：Registry 重名、优先级、不同模块作用域、签名绑定、Provider v2 parser、路径/target/hash、Capability Handler 缺失、句柄生命周期。**L1 内部集成**：`check/run/inspect` 从同一个 `PreparedProgram` 走，Host Worker/ABI，持久化 lock。**L2 黑盒命令**：通过真实 `CARGO_BIN_EXE_marqdo` 执行独立 fixture，核对退出码、stdout、stderr/JSON diagnostics 与无副作用。**L3 外部仓库**：冻结发行二进制后，单独 `git init` 外部库和插件，构建扩展并成功调用，验证 CLI SHA 不变。**L4 平台/安全**：在真实 Linux/macOS/Windows 上按声明矩阵验证；Linux strict L1 继续 fail-closed，独立 OS VM lane 不能用容器 smoke 取代。

新增测试位置建议：

```text
crates/cli/tests/callable_resolution.rs
crates/cli/tests/provider_cli.rs
crates/cli/tests/extension_selection.rs
crates/cli/tests/stdlib_alias_routing.rs
crates/cli/tests/external_repo_conformance.rs
crates/marqdo-host/tests/capability_handlers.rs
crates/marqdo-host/tests/provider_worker_v2.rs
crates/marqdo-frontend/src/calls.rs  (单元 fixture)
tests/conformance/semantics/*           (统一语法 fixture / golden)
tests/consumers/*                        (公开协议消费者)
tools/ci/verify_extension_contracts.py  (冻结二进制 E2E)
```

现有 `architecture_boundaries.rs` 只检查目录依赖和部分源码形状，不能替代真实黑盒；但应该新增“禁止核心硬编码第三方名字”的结构测试。不要以 `rg` 命中个数替代行为测试。

### 11.2 编译与静态测试（每个提交都运行）

```bash
set -euo pipefail
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
cargo build --release --locked -p marqdo-cli
cargo test -p marqdo-cli --test callable_resolution --locked
cargo test -p marqdo-cli --test provider_cli --locked
cargo test -p marqdo-cli --test extension_selection --locked
cargo test -p marqdo-cli --test external_repo_conformance --locked
```

每个新测试文件必须在加入后才启用其对应 `cargo test --test ...` 命令，PR-00 阶段可先生成空的可编译脚手架，但不得以空测试声称通过。完成 PR-08 时这些文件都必须存在，且每个包含正反例。对于 platform-specific fixture 用 `#[cfg]` 显式标记，CI 结果需区分 `PASS`、`UNSUPPORTED`、`SKIPPED`；被跳过的平台不能算 PASS。

### 11.3 不可替代的黑盒证据清单

每次 release candidate 必须自动生成 `docs/evidence/extension/<SHA>/manifest.json`，至少包含：

```json
{
  "schema": "marqdo.extension-evidence",
  "schema_version": 1,
  "tested_sha": "<40-hex-source-SHA>",
  "cli_binary_sha256": "<64-hex>",
  "fixture_repo_sha": "<40-hex>",
  "os": "linux",
  "arch": "x86_64",
  "results": {
    "callable_registry": "PASS",
    "plugin_worker": "PASS",
    "foreign_kernel": "PASS",
    "resource_provider_cli": "PASS",
    "package_pinned_git": "PASS",
    "tool_analysis_consumer": "PASS",
    "negative_security": "PASS"
  },
  "commands": ["cargo test --workspace --locked"],
  "failures": [],
  "unverified": []
}
```

占位符由 CI 实际数据替换；不允许人工手填 `PASS`。`tested_sha` 与当前 release SHA 不一致时整份证据无效。单独存在单元测试结果而没有冻结 CLI SHA 的套件也不能认定外部仓库解耦完成。**仓库必须为这个 JSON 提供 JSON schema 和校验脚本**，并将它挂到既有 `tools/ci/release_evidence.py` 或等价的主发布门禁，不能造独立“绿色但不参与发布”的报告。

### 11.4 性能基线与回归判定

该架构改动涉及每次函数调用的解析路径、启动时的静态绑定和 Provider 工件校验。要求测量四类基线：

| Workload | 测量 | 要求 |
|---|---|---|
| 纯递归/循环/函数调用 | 运行 CPU 时间 / calls/s | 不因每次调用重新扫描全局 Registry 而退化；绑定查找应为 O(1) 或 O(log n) |
| 无外部扩展的 hello/简单函数 | 冷启动中位时间、最大 RSS | 不应强制装载插件或 Provider，不因无用模块导入显著增大成本 |
| 有 1、10、100 个静态 descriptor 的项目 | `check` 耗时、峰值内存、重复导入成本 | 扩展装载至多线性于描述符数或有合理索引，不得每次调用重复扫描全量清单 |
| Resource Worker `read/write` 往返 | p50/p95 latency、吞吐与峰值缓冲 | 不复制多层无界 JSON，不向正常纯计算路径引入 Worker 开销 |

采用仓库现有 `tools/perf/bench.py` 的固定节点方式：同一 OS/arch、Rust 版本、CPU governor、`--release --locked`、构建配置与输入；暖机后各测不少于 20 次，记录 median/p95、测量波动、前后 SHA。**默认回归门限**：纯计算与无扩展冷启动的 median 不得比基线慢超过 10%，且差异应大于固定节点测量抖动；如果超出，必须提供 flamegraph/基准分析、优化或经维护者批准的独立 performance ADR。跨平台 CI runner 的绝对耗时不可直接互相比较；容器/QEMU 只能 smoke，不作为 `MEASURED` 性能门禁。增加 Provider/Plugin 时的必要启动成本可单独统计，不应归责于无扩展路径。

### 11.5 平台与 strict 安全矩阵

| Lane | 必须跑的任务 | 允许声明 |
|---|---|---|
| Linux native x86_64 | workspace fmt/clippy/test，Plugin Worker，Foreign mock，Provider CLI v2、Tool/Package 外部仓库 | 这些全部真实 PASS 时可声明 supported |
| macOS native | Rust 构建/运行、真实目标 dylib、Worker、Foreign、Plugin/Provider 目标工件 | 只有实跑对应 fixture 才声明该扩展 supported |
| Windows native | Rust 构建/测试、纯 `.mq.md` Package、Tool/Foreign 已支持部分；Native/Worker C ABI 需目标实现 | 尚未实现的 ABI 明确 `UNSUPPORTED`，不得由 Linux PASS 代替 |
| Linux strict disposable VM | 原 L1 pure/Math/stdout 正例，Foreign/Plugin/Provider 否定用例，真实隔离证据 | L1 PASS，不宣称 L2 |
| Fixed Linux perf node | `tools/perf` 固定输入与前后 SHA 对比 | `MEASURED`、`WARN` 或 `UNVERIFIED`，绝不把 SMOKE 当性能结论 |

strict VM 需符合仓库 `docs/security-vm.md` 对宿主内核、Bubblewrap、用户、namespace、真实 OS 隔离的条件。不能用 `sudo` 下“随意拥有更多权限”的运行结果证明拒绝路径；不能用 CI 运行器本身隔离的事实替代 Marqdo strict supervisor。对第三方 Worker 的 `--mode strict` **必须继续拒绝**，直到另有 strict L2 RFC 和独立破坏性安全测试。

### 11.6 发布条件（必须全部满足）

1. PR-00～08 所有强制测试、正例与负例 PASS；所有公共 CLI 选项有 `--help`、spec、fixture 和错误码文档。
2. 外部仓库套件冻结 CLI binary SHA，构建新 `.mq.md` 库、新 plugin、新 kernel、新 provider、新 tool 全部无需 CLI 源码修改且无重编译；对真实未支持平台有明确 unsupported。
3. `check/run/inspect` 唯一解析语义闭合，未知调用在静态阶段拒绝，动态 FunctionRef 不可逃逸为 PluginInvoke。
4. Plugin/Provider manifests、锁文件与 Worker ABI 均有版本与摘要校验；Native trust 与 Worker 信任边界真实表述。
5. 现有官方 stdlib、打印、内存、资源、HTTP、JSON/CSV、Package、Tool、Analysis/Trace goldens 不退化；禁止修改 golden 使错误通过而无审查记录。
6. `docs/STATUS.md`、`docs/architecture/CURRENT.md`、`COMPATIBILITY.md`、`README.md`、`spec/*` 与新增 ADR 同步更新；发布报告指向**同 SHA** 的执行证据。
7. 既有仓库 `docs/releasing.md` 的主发布门禁（包括独立 G-C 和 G4）仍然生效。本 RFC 的扩展测试是**新增的必要条件**，绝不替代任何安全/平台门禁。
8. 未满足时设置 `DO NOT RELEASE`；如只是某平台扩展未实现，要清楚标平台不支持并使运行时拒绝，而不是悄悄把未验证功能宣称跨平台可用。版本号、tag、发布资产必须经仓库所有者批准，开发计划本身不构成授权。

---

## 12. 全仓库文档、示例、测试和协议迁移清单

### 12.1 必须新建的文件

| 路径 | 内容 | 所属 PR |
|---|---|---|
| `docs/architecture/RFC-EXT-0001.md` | 本文完整规范；状态 Proposed → Implemented 需同 SHA 证据 | PR-00 / PR-08 |
| `docs/adr/0054-callable-binding-single-source.md` | 已绑定 CallableId 的静态/运行一致性、冲突策略 | PR-01 |
| `docs/adr/0055-resource-provider-cli-v2.md` | Provider v2 清单、CLI/Worker 生命周期、信任边界 | PR-03 |
| `docs/adr/0056-extension-selection-locking.md` | ExtensionSelection、lock v4、显式 activation 与授权 | PR-04 |
| `docs/adr/0057-capability-handler-registration.md` | 处理器目录、权限链、不支持服务的失败语义 | PR-05 |
| `docs/adr/0058-stdlib-public-alias.md` | 官方 bundle 公共函数 alias、打印迁移与权威边界 | PR-06 |
| `spec/resource-provider-v2.md` | JSON schema、target/sha256/transport/rights、Worker 用法 | PR-03 |
| `spec/callable-resolution.md` | 本 RFC §4.3 的命名解析/绑定冲突矩阵及诊断 | PR-01 |
| `spec/extension-selection.md` | `marqdo.yaml.extensions`、锁文件与 CLI 选择优先级 | PR-04 |
| `tests/consumers/extension-schema-consumer.*` | 不链接私有 IR 的独立消费 fixture | PR-07 |
| `tools/ci/verify_extension_contracts.py` | 冻结 CLI SHA、外部黑盒及 evidence 校验 | PR-07 |
| `docs/evidence/extension/<SHA>/manifest.json` | 自动生成的本 SHA 外部生态证据 | PR-08 |

**文件编号冲突规则**：上述 ADR 编号仅为建议。创建时若仓库已有 0054/0055 等编号，取下一个未占用编号并修改全部引用；不覆盖别人的 ADR。`docs/architecture/CURRENT.md` 继续作为唯一当前架构总览，本 RFC 是开发计划，不是第二份当前状态文档。

### 12.2 必须修改并重新验证的现有文件

| 文件范围 | 需要做什么 | 为什么不能省 |
|---|---|---|
| `README.md` | 新增明确的五类扩展示例、`--provider/--allow-provider`、明确 strict 限制 | 新使用者无需看历史 RFC 才知道怎么接扩展 |
| `COMPATIBILITY.md` | Plugin ABI/Provider schema/Wire/Analysis/Trace/Lock 等版本和支持 OS 矩阵 | 避免把 Host 接口存在当作 CLI 跨平台支持 |
| `CHANGELOG.md` | 描述 CLI Provider、函数绑定、lock 变动及破坏性修正 | 用户可识别迁移成本 |
| `docs/STATUS.md` | 按新 SHA 填 IMPLEMENTED/TESTED/UNVERIFIED/UNSUPPORTED，不复制历史值 | 状态应与证据一一对应 |
| `docs/architecture/CURRENT.md` | 新组合根、BoundCallable、Provider CLI、操作权限层、仍保留的语义封闭边界 | 防止后人重新造第二套架构 |
| `docs/ci.md` / `docs/releasing.md` | 外部仓库证据、provider worker 发行、原 G3/G4 不变 | 扩展通过不能替代发布安全证据 |
| `spec/plugin-abi.md` | 显式说明静态/运行 descriptor 一致、参数位置、Worker parity、错误 | ABI v1 不可静默改义 |
| `spec/evaluation.md` / `spec/security.md` | 调用目标、Provider 授权、句柄、strict L1 边界 | 让结果与权限语义可测试 |
| `spec/value-wire.md` | 明确是否可传某类 handle；不得把插件输出变成权限凭据 | 避免跨进程伪造句柄 |
| `spec/analysis-v2.md` / `spec/trace-v2.md` | 仅在确有字段变更时更新；必要时新开 v3 | 外部消费者合同稳定 |
| `stdlib/stdlib.json` / `tools/pack-stdlib.py` | 若新增 public alias metadata，将它计入官方包摘要与验证 | 避免元数据被替换而运行了别的函数 |
| `examples/` | 新增外部 Plugin/Provider/Foreign/Package 示例；所有 `.mq.md` 使用现行语法 | 示例必须真实可运行 |
| `crates/cli/tests/*`、`crates/marqdo-host/tests/*` | 新正反例、目标工件跨平台 fixture、专用黑盒 | 不接受只改 README 而没有测试 |
| `.github/workflows/ci.yml`、`security-linux.yml`、`performance-linux.yml` | 扩展生态门禁、平台区分、固定性能节点、strict 不放宽 | 证据必须在真正的 runner 上产生 |

### 12.3 迁移约束

- **语言版本不分叉**：仍是统一 RFC-LANG-0002；所有新增 `.mq.md` fixture/stdlib/README 都遵循该语法；不得为了兼容老 `**/*/列表` 控制映射添加新的编译分支。
- **内部允许破坏，外部必须版本化**：`CallableRegistry`、IR 私有绑定、CLI CompositionRoot 可以破坏；Plugin ABI v1、Analysis/Trace v2、Provider v1、Lock v3 涉及公开消费时必须明确新版本或兼容范围。
- **整个 README/文档同步迁移**：如果本次更改公开命令、函数解析冲突规则、Provider schema，必须批量搜索所有相应词条和示例，对涉及的 md/fixture/JSON goldens 同 PR 修改；保留历史计划文件时加 `Historical` 说明，不能让读者误认是当前实现。
- **测试不得偷偷调整 golden**：旧 goldens 改动须附差异表，说明预期语义变化、理由、对应 ADR；输出/诊断变更先审后改。
- **构建产物不进入源码**：Provider/Plugin .so/.dylib/.dll 由 fixture 或发行构建产生；源码里只存示例、manifest 模板及真实测试脚本，不提交本机私有二进制或密钥。

---

## 13. PR Review Checklist（逐个 PR 强制填写）

每个 PR 描述必须回答并给证据路径：

- [ ] 当前 `git rev-parse HEAD` 是多少，和本 RFC 基线相比哪些源代码事实已变化？
- [ ] 这次新增的能力属于 `.mq.md` 包、Callable Plugin、Foreign Kernel、Resource Provider、Tool，还是需要另立语言 RFC？
- [ ] 是否可以仅更改扩展仓库实现？如果不可以，是缺少哪个**稳定基础原语/版本化协议**，而不是哪个业务函数还没写进 Core？
- [ ] 有没有新增 `BUILTINS`、`HostOp`、`resource_request()` 的业务分支？如果有，提供不能通过现有 `.mq.md` 或 Provider 实现的证明、ADR 与替代方案比较。
- [ ] `check` 是否从不执行第三方插件/Worker/Foreign？`run` 是否使用同一绑定结果？
- [ ] 所有新 public names 是否有 owner、signature、digest、version、权限说明以及命名冲突负例？
- [ ] `pure/project/strict` 的差异是否测试？Worker 是否被错误描述为 OS sandbox？
- [ ] 非授权、超时、崩溃、重复注册、工件被篡改、版本不匹配、错误目标平台是否 fail-closed？
- [ ] 是不是保留真实官方 stdlib、Analysis/Trace、Package、Tool 的正常行为并执行 goldens？
- [ ] 能否用冻结 CLI + 独立 Git repo 做一次无需改 Core 的功能测试？
- [ ] 是否更新 `README`、`STATUS`、`CURRENT`、`COMPATIBILITY`、spec/ADR、CI evidence？
- [ ] CI 安全/性能/平台证据是否与当前 SHA 相同？有没有跳过却宣称通过的 lane？

---

## 14. 重要的设计否决事项（防止后续开发者偷懒）

以下提交一律拒绝：

1. **以“扩展语义”为由让第三方覆盖 `@exec/@knowledge`**：这会使同一 Markdown 文档在不同扩展环境下有不同执行含义，破坏可验证性。
2. **为一个新函数增加 3 个 `match` 分支**：比如为 `quantum.hadamard` 同时改 frontend、runtime 和 host。普通算法通过 `.mq.md`；性能加速走 Plugin；设备 I/O 走 Provider。
3. **让 `check` 真的加载或运行插件**：哪怕只是探测导出签名，也违反静态检查的安全与可重复性。
4. **根据 runtime 能否加载再改变静态符号解析**：未知函数不能在全无插件时落入“试执行”路径。
5. **在 `run` 合成不存在的插件签名**：不能用空参数签名、泛化 `effects=[host]` 掩盖 manifest/runtime 不一致。
6. **把 `--allow-provider` 视为可信任二进制授权**：Operator grant 允许资源操作，不能把未经 OS 隔离的 Worker 描述为安全的第三方代码环境。
7. **把 `provider.json v1` 伪装为完整性锁定**：v1 没有新 schema 中的工件 hash，需要升级为 v2 或显式可信本地 API。
8. **把 Tool、Foreign、Plugin、Provider 统一进无约束 shell 命令**：他们的生命周期、值模型、权限与故障边界不同。
9. **偷偷修改 `marqdo.lock` 的旧版本含义**：必须 bump schema 且写清迁移；不得无锁自动下载“最新”扩展。
10. **为标准库每个高级函数复制 `print_override` 模式**：必须实现一次标准函数公开别名绑定，普通三方函数仍走 import。
11. **测试夹带假路径、假 hash、无效 YAML 而声称通过**：示例里的 `<占位符>` 必须由脚本真实填充，输出依据当前真实 CLI 的退出码断言。
12. **借一套“平台抽象层”重新拆分很多 crate**：在已存在的 workspace 上小步重构，不重复建立第二套 IR、runtime、plugin SDK。
13. **声称远端 green 但没有确切 SHA/runner 证据**：安全与性能门禁不能用本机 smoke 或另一个提交的 PASS 代替。

---

## 15. 故障处理、回滚与阶段退出准则

### 15.1 开发中发现本 RFC 设计不适配真实源码时

遵循以下顺序：复现；最小 fixture；确认现行规范；选用**最少新抽象**的修复路径；更新对应 ADR（尤其公共版本）；增加负例；重新运行本阶段与全仓测试；更新 evidence。不可自行发明一个“legacy”开关；不可为了满足示例去放宽 OS 权限或弱化 manifest 校验。

### 15.2 对已经合并的每个 PR 的稳定性要求

- `PR-01` 合并后，即使 PR-02 未完成，也不允许删除 Registry 或让旧解析结果和新结果并存且对外随机采用；实现期间如需要过渡，必须显式测试新旧结果一致，合并 PR-02 时删除过渡分支。
- `PR-03` 合并后，Provider CLI v2 无法运行/授权的情况必须以正确错误码失败，不保留回退 Trusted Native 路径。
- `PR-04` 合并后，`marqdo.lock` 的 version/identity 不得静默回退；用户必须可以定位到实际使用的 artifact SHA。
- `PR-06` 合并后，旧 `print_override` 代码应被移除，不保留两条运行路径。
- 外部 consumer 的公开协议测试不能链接私有 IR 以绕开版本兼容问题。

### 15.3 关键成功/失败判定

**阶段 P0 成功**：PR-00～03 全部通过；第三方 `.mq.md`/Plugin/Kernel/ResourceProvider 可以在冻结二进制上独立提供；Callable 前后端唯一解析；受控 Provider 使用 CLI 且失败安全。此时可宣布“核心第三方执行扩展闭环（project 模式、支持平台）”；**仍不得**宣布 strict L2 或自动扩展生态完成。

**全部完成**：PR-00～08 完成且本 RFC §11 发布门禁通过；官方 bundle 的高级函数迁移无硬编码蔓延；锁定的扩展集合可重复装载；外部 View/Proof/Knowledge 工具只消费公开合同；Linux/macOS/Windows 的声明矩阵与证据一致；无未解释的 `xfail`、无红色 mandatory lane。

**必须保持的发布状态**：任何失效的安全/平台/performance gate，或者某项“已支持”的扩展缺外部独立仓库验收，状态为 **DO NOT RELEASE**。未支持的平台可以真实标 `UNSUPPORTED`，但这不等于已满足跨平台发布目标；发布决策以仓库 `docs/releasing.md` 更严格要求为准。发布前需要 Owner 对标签与资产进行单独批准。

---

## 16. 补充规范：错误码、装载顺序、扩展状态机与代码审查中的歧义消除

### 18.1 对外错误协议（本阶段冻结新增错误的名称）

新错误码采用 `ext.` 前缀；已有成熟的 `symbol.unknown`、`permission.denied`、`kernel.unavailable`、`resource.unsupported` 等沿用，不为同一事件制造两个不兼容错误码。新增建议的**规范性**错误：

| 新错误码 | 产生阶段 | 精确触发条件 |
|---|---|---|
| `ext.duplicate_callable` | prepare | 同一可见作用域的完整静态导出名冲突 |
| `ext.import_namespace_conflict` | prepare | 导入别名与 core/插件 namespace 冲突 |
| `ext.plugin_binding_mismatch` | run 初始化 | 已加载 Plugin 的导出名/签名/owner/工件摘要与静态声明不完全一致 |
| `ext.duplicate_provider_kind` | run 初始化 | 两个 Provider 试图注册同一非保留 kind |
| `ext.unsupported_provider_schema` | manifest 读取 | v1/未来版本传给只接受 v2 的 CLI Provider 入口 |
| `ext.no_matching_artifact` | manifest 校验 | 目标三元组找不到对应 artifact |
| `ext.artifact_integrity` | manifest 校验 | sha256 不符合、路径逃逸、非普通文件或不允许的 symlink |
| `ext.worker_unavailable` | run 初始化 | 匹配当前发布目标的 helper 不存在或无法启动 |
| `ext.platform_unsupported` | run 初始化 | 显式不支持当前平台对应的 Provider/Worker transport |
| `ext.extension_lock_mismatch` | prepare/run | 依赖/manifest/artifact 的已锁定身份与实际文件不一致 |
| `ext.capability_handler_unavailable` | Host effect | 声明且获授权的能力无可信执行 Handler；如已有 `capability.unavailable`，以其为 wire 公共码并将本行作为内部类型别名，禁止在同版本出现两种对外码 |

**诊断内容**：每个错误至少具有 `code`、人类可读 `message`、必要时 `module_id` / `span` / `manifest_path`、最少一个可执行修复提示；不得把密钥、插件私有二进制内存、完整敏感请求体输出为诊断。`--json` 只输出一个可解析 JSON document，程序 stdout/stderr 与 CLI 诊断保持既有约定。CLI 代码要集中复用 `emit_error` / `RuntimeError`，不散落 `println!` 风格新错误。

**退出码建议**：显式的 manifest/CLI 选项错误返回 2；编译/加载/运行失败返回 1；成功返回 0；如与仓库现有某命令公开规范冲突，应保持现行语义并在对应命令的 spec 中明确列出，不要同一错误在 run/check 间无原因变化。结构化错误码优先于从 stderr 字符串判断错误种类。对跨平台文件不存在等 OS 细节，错误码保持一致，`message` 可以提供平台补充说明。

### 18.2 唯一装载时序（关键代码不得颠倒）

```text
T0 解析 CLI 参数与项目路径、确定 mode；纯模式/strict 先对扩展授权做 preflight
T1 读取 marqdo.yaml、锁文件、依赖图、Markdown 源码（不执行）
T2 读取 Plugin/Kernel/Provider 的描述符，解析 Source/ABI/Wire 版本（不执行）
T3 按包实例、完整导出名、namespace、kind 检查重复和冲突（不执行）
T4 由唯一 CallableRegistry 完成所有静态 call binding 和诊断（不执行）
T5 生成 PreparedProgram；check/inspect/Analysis 在此终止并写诊断/投影
T6 run/call 检查选定工件、平台、路径、完整性、权限声明与有效 grants
T7 组装 Host，启动被明确选择且已授权的 Worker/Kernel adapter
T8 精确比对 Plugin 动态 exports 与 T2 静态 descriptors
T9 进入 Runtime，按已绑定 CallableId 调用；每次真实资源效果都再次检查 grants
T10 正常/异常/超时退出时关闭句柄、终止 Worker、写完整审计/Trace 与退出码
```

`T2` 的 Kernel descriptor 读取可以发生在 `check`，但不得探测/启动 Kernel，`check` 也不必须为了检查纯 `.mq.md` 文档而要求本机安装第三方可执行程序。若 `check` 被显式传入扩展描述符，可验证声明及可见性；真正的可执行工件校验允许移到 T6。`T7` 启动 Worker 属于执行准备动作，不等于项目 `.mq.md` 主函数已经执行；但任何未知或未获授权的二进制不得在 T7 启动。

### 18.3 生命周期与失败传播状态机

```text
UNDECLARED ──(显式声明)──> DECLARED ──(校验签名/版本)──> RESOLVED
     │                      │                          │
     └──unknown──> ERROR    └──invalid──> ERROR        └──hash/collision──> ERROR
                                                          │
                                          (run + grants + artifact)
                                                          ▼
                                                      ACTIVATED
                                                     /         \
                                                INVOKED         ERROR
                                                  │
                                             CLOSED/EXITED
```

只有 `RESOLVED` 能出现在 `check` 的静态输出；`ACTIVATED` 之后必须有实际授权；`INVOKED` 前不得修改静态 Callable 目标；`ERROR` 不自动重新选择其他实现。资源句柄应绑定 run id + provider id + token/generation；Worker 中断后该 provider 的未关闭句柄全部失效。二次关闭/释放后重用的行为按当前 Resource API 规范返回确定错误或 Result 失败，而不是 Host panic。资源读写的上界必须有预算；`read` 返回的 count 不能超过请求的 `max_bytes`。

### 18.4 修改边界精确定位表（开发者逐文件检查）

| 文件 | 允许/期望的最小改动 | 明确禁止 |
|---|---|---|
| `crates/marqdo-ir/src/callable.rs` | Registry API、owner+digest 的身份及冲突检测 | 使用未锁定物理路径当唯一 ID |
| `crates/marqdo-ir/src/lib.rs` | 增内部 CallBindingTable/Program 绑定存储 | 无版本地改变 Analysis v2 wire |
| `crates/marqdo-frontend/src/calls.rs` | 单入口 `bind_program_calls` 读取 registry | 再次按 Builtin/Plugin 各自写独立优先级 |
| `crates/cli/src/loader.rs` | `load` 与 `finalize` 分离、stdlib alias 的按需加载 | 打印以外再加 `program_uses_xxx` |
| `crates/cli/src/commands/check.rs` | 调用共享 `prepare`，静态检查插件 export | 加载 `.so/.dll` / spawn 子进程检查签名 |
| `crates/cli/src/commands/run.rs` | 共享 `prepare`、reconcile、统一 Host 装配 | 补造 `params=[]`、按名字猜运行入口 |
| `crates/marqdo-runtime/src/interpreter.rs` | 按 BoundCallable ID 唯一 dispatch | `if external_callables.is_empty()` fallback |
| `crates/marqdo-runtime/src/builtins_impl.rs` | 必要内核基础计算、现有 Builtin | 加 `quantum.*`、`ai.*` 业务分支 |
| `crates/marqdo-host/src/host.rs` | 统一操作/权限/audit，不改 Resource handle owner | 未授权执行插件功能 |
| `crates/marqdo-host/src/host/providers.rs` | 复用现有 provider table + 校验 | 重新造第二套 provider 名字解析 |
| `crates/marqdo-host/src/host/resource_ops.rs` | 现有内置 endpoint 和新 kind 的 registry fallback | 每个第三方 kind 再写 match arm |
| `crates/marqdo-host/src/capability.rs` | 受信 Handler、真实执行预算与审计 | 用字符串 allow-list 代替真实 Handler |
| `crates/cli/src/main.rs` | 参数声明、`--provider/--allow-provider` | 增加 `marqdo quantum` 等业务顶层命令 |
| `crates/cli/src/commands/sandbox.rs` | 现有 capability 构造和 strict preflight 新字段 | 以安装成功绕开 grants |
| `crates/marqdo-package/src/lib.rs` | 显式 extension source + lock identity 版本 | 自动取某分支最新提交 |
| `crates/marqdo-foreign/src/lib.rs` | 继续用 KernelRegistry 查询 | 为 R/Julia 再 hardcode 执行路径 |
| `crates/marqdo-plugin-abi/src/manifest.rs` | ABI v1 清单的已定字段及严格校验 | 为 Provider v2 随意复用同名不同义字段 |
| `stdlib/` | 业务算法、通用格式/数据/资源编排 | 通过私有 Rust Hook 获取 OS 超权限 |

### 18.5 负责人实际操作的验收判定速查

| 用户想做的新事 | 应选的扩展方式 | 如果必须改 CLI，应如何处置 |
|---|---|---|
| 新 AI 模型提供商 / OpenAI-compatible API | 独立 `.mq.md` 网络库（TLS/HTTP），外部密钥从受控 env/配置能力获取 | 缺底层 TLS/stream/权限原语时才改 Host，并写基础协议 ADR |
| 新量子算法/变分线路生成 | `.mq.md` 可调用算法函数 | 计算原语不足时先证明最小 core necessity，不硬编码算法名 |
| 接入量子硬件 SDK | Callable Plugin 或 Resource Provider | 只可能补通用 ABI/Provider 支持，不加 `quantum` 特例 |
| 新数据库驱动 | Provider stream + `.mq.md` 查询/编码 API | 不改 parser / `HostOp`，仅在 Provider 协议真缺能力时升版本 |
| 新语言 R/Julia 执行 | 新 `kernel.json` + 已支持 wire 的外语程序 | 不修改 `SUPPORTED_LANGUAGES` 分支，仅修通用 Registry bug |
| 新的 Proof/Knowledge/View | CLI Tool + Analysis/Trace/源文档 | 公共投影缺确切证据字段才版本化扩充 Analysis，而非改语言执行语义 |
| 新的 `@agent` 执行关键字 | **不允许第三方修改 `@exec` 规则**；改用固定语义中的库调用 + 惰性元数据 | 如确有新语言构造，另立语言 RFC，全面迁移测试后发布 |
| 新基本数值类型、原子并发或 GPU 原生指令 | 正式核心语义/后端 RFC | 这类变更**不在**“无核心变动”的扩展保证中 |

**验收公式（文字约束，不是数学证明）**：`外部新功能成功 = 已存在的语言语义 + 版本化的公开扩展合同 + 显式加载与授权 + 可复现实现身份 + 黑盒正反测试`。任何新增扩展若靠“修改 Core 的业务分支”才能成功，必须回到上述边界重新判定，而不是把新增内核分支作为本 RFC 的完成证据。

## 17. 开发者执行指令（可以直接复制到新开发任务中）

> 你正在 `cflmy/marqdo-cli` 进行可扩展性闭环开发。以当前仓库 `master` 最新 SHA 为准核验本 RFC §2 的源码事实；禁止猜测代码状态。严格按 PR-00～08 的顺序或允许的并行依赖实施。保持 RFC-LANG-0002 的单一 Markdown 语义，第三方不得改变 `@exec/@knowledge`。先修唯一 Callable 解析、移除未知插件 fallback 和运行时空签名合成，再贯通 CLI Resource Provider，再做可锁定扩展选择、权限 Handler、官方 stdlib alias、独立消费者与发布门禁。所有功能必须带编译/运行一致性测试、负面权限与版本测试、独立仓库冻结 CLI 二进制 black-box 验收。不要新增业务 Builtin/HostOp，不要引入第二套插件架构或兼容旧语言 mapper。每个 PR 交付代码、spec/ADR、README/STATUS/CURRENT、测试、当前 SHA 的 evidence 和未完成项。若某路径已在当前 HEAD 完成，用证据标记已闭环，不重复造轮子。未满足任何强制门禁则维持 DO NOT RELEASE。不得未经 Owner 授权打 tag 或发布。

---

## 18. 本 RFC 的参考基线与关系

本规范已涵盖实施所需的原则、接口约束、示例、顺序与验收，不要求开发者先阅读旧审计才能知道该做什么；以下仅作为对照来源，发生矛盾时以**当前代码、正式已接受 ADR、公开 spec**为事实来源，并在 PR 中修正本 RFC：

- 语言语义：`docs/architecture/RFC-LANG-0002-unified-semantics.md` 与 `docs/adr/0052-unified-markdown-semantics.md`。
- Callable：`crates/marqdo-ir/src/callable.rs`、`crates/marqdo-frontend/src/calls.rs`、`crates/marqdo-runtime/src/interpreter.rs`，已有 ADR-0009、ADR-0010。
- Plugin：`crates/marqdo-plugin-abi/*`、`crates/marqdo-host/src/host.rs`，`spec/plugin-abi.md`。
- Foreign：`crates/marqdo-foreign/src/lib.rs`、ADR-0012 与 ADR-0020。
- Provider：`crates/marqdo-host/src/host/providers.rs`、`resource_ops.rs`、`provider_c.rs`、`provider_worker.rs`，ADR-0033～0038。
- Package：`crates/marqdo-package/src/lib.rs`、`crates/cli/src/loader.rs`，`marqdo.lock` 现行 spec。
- 标准库：`stdlib/`、`crates/cli/src/stdlib.rs`、ADR-0039。
- Tool/Analysis/Trace：`spec/tool-v1.md`、`spec/analysis-v2.md`、`spec/trace-v2.md`。
- 原有安全/发布：`docs/security-vm.md`、`docs/security-gate.md`、`docs/releasing.md`、`docs/performance.md`。
- 代码事实基线：https://github.com/cflmy/marqdo-cli/tree/595279fe38048b52b96d9ff1e7d57a867ec5c4c6

**最终原则**：让外部库扩展的是**能力、算法、资源与可读知识制品**，而不是篡改 Marqdo 的唯一执行语义。以单一静态绑定与受控 Host 为中心，保证语言内核和独立生态在功能增加时不互相挟持。

---

## 19. Implementation record (2026-11-22)

This section records the executed state. It does not tag a release or override
`docs/releasing.md`; the owner still decides tags/assets.

| PR | State | Evidence |
|---|---|---|
| PR-00 baseline | done | `docs/evidence/ext-baseline-595279f.md` (fmt/clippy/test green at `595279f`) |
| PR-01/02 unique callable binding | done | `CallSiteKey`/`CallBindingTable`, one `bind_program_calls`, runtime dispatch by identity; `crates/cli/tests/callable_resolution.rs` 14 tests; `spec/callable-resolution.md`, ADR-0054 |
| PR-03 provider v2 CLI | done on linux/x86_64 | `crates/cli/tests/provider_cli.rs` 12 tests; `spec/resource-provider-v2.md`, ADR-0055; macOS/Windows FAIL-CLOSED |
| PR-04 extension selection/lock v4 | done | `extensions:` manifest block, lock v4 identity, `inspect --extensions`; `crates/cli/tests/extension_selection.rs` 13 tests; `spec/extension-selection.md`, ADR-0056 |
| PR-05 capability handlers | done | handler registry + real `fs.read`; `crates/marqdo-host/tests/capability_handlers.rs` 12 tests; ADR-0057 |
| PR-06 stdlib public aliases | done | verified `public_aliases`/`aliases_digest`; `print_override`/scan/runtime branch removed; `stdlib_alias_routing` 5 tests; ADR-0058 |
| PR-07 external consumers | done | separate repo `marqdo-extension-conformance` (pure pinned-Git lib, plugin, unknown kernel, provider v2, Analysis/Trace tool) + frozen-CLI suite |
| PR-08 release/docs convergence | implemented; release-tag rerun pending | workspace gate `599 passed / 67 targets`, fmt/clippy clean; evidence manifest bound to `6a7bb98`; the tag action itself requires owner approval and the unchanged G3/G4/native gates |

Known deviations are recorded in `docs/evidence/2026-11-22-ext-implementation.md`
§4 (provider transport linux/x86_64 only; additive provider error codes;
analysis `calls[]` projection; no tag).

**Do not treat this record as a release.** The release decision remains
`DO NOT RELEASE` until the remote G3 artifact and same-SHA mandatory lanes
exist; the external-extension manifest is an additional necessary gate, never a
substitute.
