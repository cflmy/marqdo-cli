# Marqdo CLI M0.6（扩展性审计修订 v3）— 轻量核心、开放能力注册与独立生态实施规范

> **Historical planning input (M0.6).** This plan is superseded by
> [`STABLE_RELEASE_IMPLEMENTATION.md`](STABLE_RELEASE_IMPLEMENTATION.md)
> for next-phase work and is not current status. See
> [`architecture/CURRENT.md`](architecture/CURRENT.md) for the tree as it is today.
> Keep this file as design history; do not use it as an implementation claim.


> **文档性质**：面向没有参与任何历史讨论的开发者的独立架构决策、实施计划及验收契约。  
> **建议仓库路径**：`docs/NEXT_PHASE_IMPLEMENTATION.md`  
> **设计状态**：**Proposed / v3 破坏性架构纠偏**（需要维护者批准并通过验收；基于源码进行的扩展性审计要求：Tool Protocol、可调用函数、外部内核、ABI 导出、模块来源均消除不必要的硬编码扩展瓶颈）。  
> **仓库与审查基线**：[`cflmy/marqdo-cli`](https://github.com/cflmy/marqdo-cli)，`master`，commit [`0116160399fda6df69a72da438e04b0072566b0f`](https://github.com/cflmy/marqdo-cli/commit/0116160399fda6df69a72da438e04b0072566b0f)，检查日 2026-11-20。  
> **语言身份**：独立 Rust 解释执行的 Markdown 原生语言，`.mq.md`；新项目不从旧 `cflmy/marqdo` 仓库迁移代码或承诺旧语法/API/ABI 兼容。  
> **交付目标**：在**不扩张 CLI 顶层命令面**的前提下，先消除 M0.5 中阻碍第三方扩展与多版本依赖的结构性瓶颈，再将工具链整理为轻量稳定核心；提供足够小的、版本化的公共分析与执行接口，以及 `marqdo tool` 外部工具分发机制，从而允许 Knowledge、Proof、View 三个独立仓库和第三方工具开始独立开发。  
> **判定用语**：`MUST/必须` 为本阶段规范；`SHOULD/建议` 为可经 ADR 调整的默认方案；`DEFER/后置` 不属于本轮完成门槛；`EXISTING/现有` 为仓库已有实现，不代表经本次现场构建验证。  
> **后续阶段基线**：`docs/CORE_STABILIZATION_AND_ECOSYSTEM_GATE.md`（Proposed，审查基线 `0adebe4`）基于本计划之后的提交，重新审查 CI、锁图、源码身份、Analysis/Trace、Tool 与安全隔离，并定义 G-A/G-B/G-C 发布门槛。两者冲突时以该文为当前阶段基线；本文保留 M0.6 的设计与审计来源。

## 0. 阅读顺序、权威性与禁止误解

本文件用于告诉陌生开发者“下一步为什么做、具体做什么、哪些不要做、怎样验收”，**不是**另起一套取代现有实现的语言规范。建议依次阅读：

1. `docs/CORE_STABILIZATION_AND_ECOSYSTEM_GATE.md`：后续稳定化、协议冻结与独立生态准入的当前阶段规范（Proposed）；先用它刷新基线与发布门槛。
2. 本文件：产品边界、M0.6 实施次序与发布条件。
3. `docs/STATUS.md`：M0.5 当前实现的阶段性记录；该文件可能因后续修复提交而局部滞后，存在冲突时以当前源码和回归测试为准，并同步修正状态表。
4. `spec/markdown-profile.md`、`spec/mapping.md`、`spec/expression.ebnf`、`spec/evaluation.md`、`spec/references.md`：**现有语言行为的规范来源**。
5. `spec/package-format.md`、`spec/value-wire.md`、`spec/security.md`、`spec/foreign.md`、`spec/math.md`、`spec/plugin-abi.md`：工程协议与边界。
6. `docs/adr/0001-*.md` 至 `0007-*.md`：已经接受的决策；若本文件要求改变其行为，先提新 ADR，不能静默覆盖。
7. `tests/README.md`、`tests/conformance/`、`tests/negative/`、`crates/cli/tests/`：通过测试展示真实行为。
8. `docs/DEVELOPMENT_BLUEPRINT.md`：历史 M0.5 全量设想与设计依据；**在命令规模、能力注册、插件寻址、包身份、仓库解耦和发布门槛上以本文件为新的目标方案**，已接受的具体语义规范仍以 `spec/`、ADR 和测试为准。

**不要混淆四个不同的名称**：`marqdo-cli` 是本仓库和核心工具链；`marqdo` 是用户使用的命令；`marqdo tool` 通过统一清单管理**任意**独立 CLI 工具；运行时 C ABI 插件用于扩展程序函数，**不是**外部 CLI 工具。

### 0.1 不可违背的设计原则

- Markdown 的**AST 结构**承担程序控制语义，而不是在 Markdown 中粘贴另一套 Python/JS 代码；普通散文不产生执行效果。
- 只使用固定的 CommonMark/GFM/Math 配置解析 Markdown；严禁以正则或自然语言识别替代标准解析器；遇到已占据执行位置但非法的节点，报错，不回退为普通文字。
- 表达式求值、词法作用域、函数、分支、循环、返回、调用栈均由 Marqdo **原生 Rust 解释器**实现；**不得将 Marqdo 编译/翻译成 Python 作为正式执行路径**。
- Math/Foreign 是原生语言值与受控 Host 能力；读取、检查、引用或渲染代码围栏/公式不得触发执行。
- 一切副作用通过 CapabilityHost；严格隔离不可用必须拒绝，不能静默降级。C ABI 本身不提供沙箱。
- 普通 `.mq.md` 必须在没有 Knowledge、Proof、View 的环境中独立运行。
- 对外输出**稳定、最小、版本化**的数据契约，不公开整个私有 IR 并让三个外部仓库依赖 Rust 内部结构。
- **用户入口统一，工程仓库独立**：通过同一 `Tool Protocol` 安装任意合规清单后自动得到 `marqdo <command>`。`view`、`knowledge`、`prove` 只是示例，不在 CLI 中注册硬编码的特殊名字、发布地址或适配器。
- 拒绝为了“以后可能有用”增加顶层命令；先用真实消费者需求、测试和 ADR 证明必要性。
- **开放扩展点，但不开放语言语法的任意重定义**：允许注册外部函数、工具、内核、依赖来源；禁止插件重新解释 `Strong`、`Emph`、列表或引用的核心语义。
- **允许破坏性修正错误架构**：现处 `0.1.0` 早期阶段，凡未对外冻结或存在显著扩展阻塞的内部 API 可重构；涉及公开 ABI、wire、清单或语言行为时必须显式升级版本、加负例和迁移说明。

## 1. 本次真实仓库审查（不是假定的空仓）

### 1.1 仓库事实与证据

以下是审查 `master@0116160` 源码和随仓文档所得结论；**本轮未在本地复刻构建或独立运行完整 CI**，因此“测试通过”指仓库的测试用例、状态文档和最近提交记录，不能取代开发者后续的真实 CI 证据。

| 范围 | 观察到的现状 | 证据文件 | 本轮判断 |
|---|---|---|---|
| Workspace | Rust 1.82、edition 2021、版本 `0.1.0`，15 个 workspace crates | 根 `Cargo.toml` | **保留现有按职责分层结构**，不再提前拆出更多 crate |
| Markdown 解析 | 单一 `marqdo-markdown`，固定 comrak 1.0.0 及 `marqdo.md.v1`，有源码跨度、frontmatter | `crates/marqdo-markdown/`、`spec/markdown-profile.md` | 已有正确基础，不重写 Markdown 解析器 |
| 语言前端 | `marqdo-expression`、`marqdo-frontend`、`marqdo-ir` 有函数、推断、调用和 IR | `crates/marqdo-frontend/src/{infer,map,calls,units}.rs` | 核心路径已建立，重点补严格诊断与跨模块位置 |
| 执行器 | `marqdo-runtime` Rust tree-walk，算术、循环、递归、函数调用和运行时预算 | `crates/marqdo-runtime/` | 保留；**字节码/JIT 不属于本阶段发布门槛** |
| 函数与链接 | ModuleId/UnitId/FunctionId、显式 YAML import、静态 ref 图、`check --refs` | `spec/references.md`、`crates/cli/src/loader.rs` | 函数级引用足以支持后续 Knowledge 原型；跨文件链接调用当前不完整；包身份仍基于项目相对路径，尚不适合冻结对外协议 |
| 数据/Math | BigInt、浮点类型化 wire；有限精确有理式 Math | `spec/value-wire.md`、`spec/math.md` | 保留已支持子集，不扩成 CAS/定理系统 |
| Foreign | Python/Node 一次性外联、类型化协议、超时与输出限制 | `spec/foreign.md`、`crates/marqdo-foreign/` | 不是 Marqdo 后端；缺会话/虚拟环境自动管理 |
| 包管理 | `marqdo.yaml` / lock、路径依赖、`add/remove/lock/sync` 基础命令 | `crates/marqdo-package/`、`spec/package-format.md` | Git 解析、传递依赖、环境物化未完成；**不得宣称具有完整的 venv/uv 体验** |
| Host / 沙箱 | CapabilityHost、纯/项目模式及预算；`strict` 在缺 OS adapter 时拒绝 | `crates/marqdo-host/`、`crates/marqdo-sandbox/` | 安全边界已有，但严格沙箱未实际提供 |
| ABI 插件 | 新项目自己定义的 C ABI v1、Rust SDK、C 示例、受信任的进程内加载 | `include/marqdo_plugin_abi_v1.h`、`crates/marqdo-plugin-{abi,sdk}/` | 保留 ABI v1；隔离 worker、能力桥和安装尚缺 |
| CLI 命令 | `init/run/call/check/inspect/fmt/plugin inspect/add/remove/lock/sync` 已定义 | `crates/cli/src/main.rs` | **已有命令偏多**，下一版只增加 `tool`，不继续生长管理命令 |
| 验证 | 150 个 AST goldens、36 个语义 goldens、负例/性质/插件测试、Linux CI | `tests/conformance/`、`crates/cli/tests/`、`.github/workflows/ci.yml` | 有坚实基座，但跨平台、隔离、独立工具消费者测试不足 |

**状态文档需要校正的具体例子**：当前 `docs/STATUS.md` 某些段落仍称“没有 CI workflow”“`print` 没有检查 console 权限”“check 不验证链接”（但同一文档已记载 `check --refs` 可以验证）。但仓库现已有 `.github/workflows/ci.yml`、`crates/marqdo-host/src/host.rs::op_print` 中的 console 检查，并且 **`check --refs` 已验证链接，只是默认 `check` 不启用该可选检查**。实施时先核对并同步修改状态表；不要将状态文字当成比源代码更高的事实。

### 1.2 当前真正缺的能力（按影响程度，而不是功能数量）

| 缺口 | 后果 | 优先级 |
|---|---|---|
| 通用 `marqdo tool` 发现、登记与分发完全缺失 | 任意第三方仓库都无法仅凭统一清单接入 `marqdo <command>`；`view/knowledge/prove` 只是三个可能的消费者 | **阻塞外部工具统一入口** |
| 可稳定读取的公共 Analysis v1 不存在 | 外部 Knowledge/Proof/View 只能解析终端输出或链接内部 crate | **阻塞三仓独立开发** |
| 文件级执行 Trace v1 缺统一协议 | View 难以准确定位步骤、调用帧和运行结果 | 阻塞 View 的高级执行态（不阻塞只读 View） |
| CLI 命令与历史蓝图混杂 | 每加入一项功能就新增命令/管理层 | **本轮必须收敛** |
| 依赖锁定未覆盖 Git 和传递依赖，环境未物化 | 难以跨设备复现项目 | 工程稳定版必需，原型阶段可以局部可用 |
| 严格隔离和独立 ABI worker 不可用 | 不得声称不可信外部代码/插件能安全执行 | **安全能力陈述与实现必须一致**；建议先 Linux 有验证实现，其他平台拒绝 |
| 跨模块运行错误位置、嵌套惰性语义仍有诊断缺口 | 错误定位影响可读性、View 关联 | 核心质量门槛 |
| 缺独立消费者兼容测试 | 新版 CLI 可能在不知情情况下破坏其他仓库 | **发布前必需** |
| 跨平台 CI、发布与工具安装校验尚不充分 | 真实安装/扩展易失败 | 工程稳定版必需 |

### 1.3 本轮不应该做的事

不因扩展性随意重写 Markdown 核心语义，但允许通过 ADR 修正已证实的结构性设计错误；不搬旧 `cflmy/marqdo` 的源码；不把 Proof/Knowledge/View 的实现并回核心；不开发注册表服务、向量数据库、Web 应用、Lean 适配器、知识图数据库；不在本阶段写 JIT/字节码；不开发完整 Python/PyPI 包解析器；不为每种 Host 能力增加新的顶层命令。

## 1A. **扩展性审计（v3 增补，优先级高于后续阶段）**

> 独立的审计摘录（便于按问题创建任务）见 [`docs/EXTENSIBILITY_AUDIT.md`](EXTENSIBILITY_AUDIT.md)；本节保留主文档所需的完整上下文。

> **审计基线与证据说明**：在 `master@0116160399fda6df69a72da438e04b0072566b0f` 上读取 `Cargo.toml`、CLI、frontend、runtime、Host、foreign、package、插件 ABI 及其规范，并交叉核对仓库 `docs/STATUS.md`。以下将“源码可确认的问题”“目前尚未实现的能力”和“风险预测”分开陈述。该审计为代码静态检查，**尚未自行重新构建所有平台或执行安全攻击测试**。后续提交若改变实现，应刷新本节证据。

### 1A.1 判定什么才是扩展性问题

- **真正的阻塞**：为引入一个第三方工具、函数、内核、模块来源或插件导出，必须修改编译器/解释器多个硬编码位置、重编译核心或破坏其他包的身份；公共消费者被迫使用私有 Rust 结构；安全权限不可随扩展组合。
- **不是阻塞**：Markdown AST 解释规则固定、标准表达式优先级固定、核心值模型/公共 wire 有版本、Host 持有操作系统最终授权。这些封闭边界是语言一致性和安全性的必要条件。
- **不是目标**：允许插件改写 Markdown 的语义、给任意第三方程序直接 Rust 内存访问、用一个万能动态字典替换静态类型，以及为了“可扩展”预先引入数十个抽象层。
- **可破坏范围**：编译期内部类型、`HostOp` 编排、`ModuleId` 表示、CLI 非核心历史命令、插件注册实现可以改变；公开 wire/manifest/ABI 变更必须有显式版本断点，不能无声把 v1 字段换意义。

### 1A.2 审计发现：需要进入工程待办的具体问题

| 编号/优先级 | 当前源码事实及直接证据 | 为什么不利于扩展 | 必须做的修正 | 验收测试 |
|---|---|---|---|---|
| **E01 / P0** | `crates/cli/src/main.rs` 的 `Command` 是封闭枚举，尚没有 `tool` 通用分发；`crates/cli/src/commands.rs` 处理各核心命令 | 每新增外部工具都可能要求修改 CLI 源码 | 实现清单驱动、无工具名白名单的 Tool Protocol；仅一个 `tool` 管理入口；未知命令由已注册清单转发 | 临时编译 `marqdo-xyz`，**不重编译 CLI**就能 `marqdo xyz --help`，并正确转发 Unicode/二进制原样参数、流和退出码 |
| **E02 / P0** | `crates/marqdo-ir/src/builtins.rs` 有静态 `BUILTINS`；`crates/marqdo-frontend/src/calls.rs` 使用 `builtins::find`；`crates/marqdo-runtime/src/builtins_impl.rs::dispatch` 对 builtin 名称 `match` | 新增可调用能力会碰到前端签名、运行时分发、Host 三处，且第三方函数缺独立的静态签名索引 | 增加**统一 Callable 解析/描述符**（内置 + 导入函数 + ABI exports），静态签名从声明/清单取得，运行时将已绑定目标交给相应实现；内置集合仍可封闭，但扩展注册不触碰这些 `match` | 第三方仅提供包与合法 ABI，不改以上文件，就能通过 `check` 签名检查并通过 `run` 调用；重复符号、参数错误在运行前拒绝 |
| **E03 / P0** | `crates/marqdo-host/src/host.rs::load_plugin` 只用 `manifest.plugin.exports.first()` 的前缀绑定单个 `PluginSlot.namespace`，`op_plugin_invoke` 用 `find(namespace)` | 一个插件的不同导出前缀无法可靠注册；同前缀多插件存在抢占/歧义风险 | 以**完整限定导出名**注册每个函数：`CallableId -> {package, manifest_digest, export, signature, effects, handler}`，逐项检测冲突；调用不得“查到第一个就算” | 一个插件导出 `solver.add`、`graph.search` 两个前缀均可调用；另一插件宣告 `solver.add` 应确定性冲突或要求显式别名，绝不任意抢占 |
| **E04 / P0** | `crates/marqdo-plugin-abi/src/manifest.rs::KNOWN_CAPABILITIES` 是内建固定字符串数组，未知要求直接拒绝；`Host::dispatch` 为固定 `HostOp` 枚举 | 每增加一种扩展能力都可能要发布新版核心；但无约束放开又会导致提权 | 固定最小 **Host 原语能力**（文件/网络/进程等）；允许清单声明**带发布者命名空间**的扩展权限声明，但只有宿主安装可信策略提供处理器并**明确授权**时才可使用；未知权限默认拒绝执行，不“自动认为安全” | 新增 `vendor.device.read` 的 manifest 不改核心就能被检查/描述；无可信 handler 或用户授权时执行必然拒绝；不允许扩展创造 `fs.*` 的无约束替代权限 |
| **E05 / P0** | `crates/marqdo-foreign/src/lib.rs::SUPPORTED_LANGUAGES` 只有 Python/JavaScript，`normalize_language`/`program_candidates` 写死分支 | 添加 R/Julia/SQL 等需要改核心并发布新版解释器 | 增加按**语言标签注册的 Kernel Provider**（独立执行程序 + 显式清单 + wire v1/适配器版本），原有 Python/Node 作为内置 provider；未知标签维持惰性 `Code` 值，显式调用时才查询 provider | 在不改 `marqdo-foreign` 的情况下注册测试内核 `mocklang`，执行 `foreign.call`、传值、超时、权限拒绝；未安装时报 `kernel.unavailable` |
| **E06 / P0** | `crates/marqdo-ir/src/lib.rs::ModuleId` = 项目根相对路径；`crates/cli/src/loader.rs::module_id_for` 限制根；`resolve_import` 的 `@dependency` 只读 `dependency.path` | 全局包缓存、项目外只读依赖、多版本同包不能获得可靠的逻辑身份，路径布置影响知识/证明引用 | **在公开 Analysis v1 冻结之前重设计包感知模块身份**：逻辑 `PackageInstanceId + package_relative_path`；物理路径仅是来源位置，不能作为跨项目唯一身份；Loader 通过解析后的 package store 装载 | 同名包不同锁定版本有不同身份；包移动缓存位置不改变身份；Git 固定 revision 模块可被静态定位；双版本不存在隐式覆盖 |
| **E07 / P0** | `marqdo-package::Dependency` 只有 `path`/`git` 字段；`crates/cli/src/loader.rs` 自行解释 `@dependency`；Git 尚不能解析 | 包来源、依赖求解与模块加载耦在 CLI，未来注册表/新来源会增加 `if source=...` 的多处条件 | 在 Package 层定义**Source Resolver** 统一接口与锁定结果，初期实现 path+Git；Loader 只消费 `ResolvedPackage`，不识别来源类型；不要提前实现官方 registry 服务 | 同一模块通过 path 与固定 Git revision 导入得到一致可调用语义；相同锁文件离线缓存可复用；来源不匹配时拒绝，不偷偷回落路径搜索 |
| **E08 / P0** | `marqdo-plugin-abi::PluginKind` 声明 `Native/Worker/Wasm`，实际 Loader 只允许 Native；`Host::load_plugin` 仅支持显式受信的进程内路径 | 第三方 ABI 虽可编译但默认安全接入能力缺失；崩溃与宿主共享故障域 | 实现**隔离 Worker IPC** 为非受信插件的实际执行路径；`Native` 只在显式受信模式启用；`Wasm` 未实现时显式 unsupported 而非装作可用；完整导出注册在两个传输中保持一致 | 插件崩溃/超时/越权只影响 Worker，不能使解释器崩溃；受信 Native 明确标注风险；Worker 与 Native 在许可的纯调用上结果一致 |
| **E09 / P0** | `marqdo-plugin-abi::LoadOptions::default()` 的 `capability_handler` 为空；`Host::load_plugin` 当前没有设置受控处理器 | SDK 虽声明 host capability 回调，实际扩展不能利用合法获授权的 Host 服务；开发者容易绕开 Host | 建立 **Capability Broker**：以 `plugin_instance + capability + call_context` 处理请求，映射到 Host 已实现且授权的原语；接口必须受资源预算、审计和会话作用域约束 | 被授权插件可受控读取限定文件；未授权读/写/进程请求拒绝；不能伪造他人 package identity 或复用过期 handler |
| **E10 / P1** | 核心目前有 `inspect --ir` 等内部输出，但无稳定 Analysis v1；外部 Tool 不应链接私有 crates | Knowledge/Proof/View 可能反过来挟持核心内部结构，破坏四仓独立 | `check --emit` 生成**带 profile 版本/函数身份/函数体表达式树/导入与引用/源码指纹的公开语义投影**；清晰区分公开协议与私有优化 IR，提供 schema、goldens 与版本协商 | 不依赖 Rust workspace 的独立 Python/Go 消费者解析 Analysis；Core 修改内部 IR 后消费者合同测试仍通过；不执行被检文档 |
| **E11 / P1** | `crates/marqdo-host/src/trace.rs` 记录 Host 效应，`run --trace` 是有限调试信息 | View 无法准确重建调用、分支、循环与帧级步骤，可能被迫插桩/修改核心 | `run/call --trace-file` 输出**可选低开销执行轨迹**，由 Runtime 发出语义事件，Host 发出效应事件；源位置/调用帧关联 Analysis，敏感数据默认不记 | 同一文档的循环、递归、外联异常、权限拒绝产生可关联事件，禁止泄漏密钥；无 tracing 的运行语义不变 |
| **E12 / P1** | CLI `Command` 中存在 `init/inspect/fmt/plugin/remove/lock` 等额外入口，状态文档还以其作为功能扩张依据 | 用户命令面和测试维护面膨胀；核心成为一切工具的容器 | 以 `run/call/check/add/sync/tool` 为规范主路径，已有命令可在 **0.x 破坏性发布**中删除、转入标志或迁给独立 Tool；不因为“已经写了”就永久保留，**但须逐项核算保留成本** | `marqdo --help` 核心职责清晰；没有 View/Proof/Knowledge 硬编码；既有命令的替代操作可通过公共接口完成 |
| **E13 / P1** | `crates/marqdo-host/src/capability.rs::Capabilities` 为字段固定结构，`check_net` 没对应网络执行面；`foreign` 子进程按当前用户权限运行，OS strict 拒绝 | 新 Host/Foreign/网络能力难以安全组合；进程隔离承诺不够 | 保留有限核心能力分类 + 泛化**capability descriptor/policy handler**；未知权限显式拒绝，接入新处理器不绕开 Host；严格沙箱明确按平台能力启用或拒绝 | 任意新扩展的权限必须有一条请求→用户授权→运行时检查→审计链；Foreign 子进程越权的负例不得声称已被 `project` 防住 |
| **E14 / P1** | `docs/STATUS.md` 存在“无 CI”“打印不检查 console”等与后续提交不一致的历史描述 | 开发者会错误评估架构状态，重复造轮子或错误冻结协议 | CI 自动生成或核验实现证据；状态分为 `implemented/tested/partial/unsupported`；正式 ADR 标记可破坏修改及旧行为废弃 | 状态表引用的 CI、文件和命令可在当前 HEAD 重现，过时文本在本阶段清理 |
| **E15 / P0** | `crates/marqdo-value/src/wire.rs` 能从任意结构化 JSON 重建 `plugin_handle {plugin, export, id}`；`spec/value-wire.md` 也明确该 Handle 非不可伪造令牌 | 若后续把这种可重建值直接作为权限凭证，会引入冒名/重放风险；**当前未认定已经存在可利用的提权路径**，但必须在增加有状态插件资源之前纠正 | 在宿主侧建立带**所有者、运行会话与世代**的 handle table；wire 中只能传入不透明引用，真正权限必须在 Host 以受信任上下文查表核验；不将用户提供的 JSON 本身当作权限凭证 | 构造相同字段/猜测 ID/跨会话重用/卸载后重用均拒绝；合法句柄可以传递给其授权所有者；序列化不得泄漏宿主地址 |
| **E16 / P1** | `crates/marqdo-frontend/src/map.rs::resolve_link_target` 对跨模块链接形式调用返回无法解析；`ref` 当前限定同模块（`spec/references.md`） | 函数级知识在多模块中的跨文档**可寻址调用**不完整；第三方消费者若依赖该形式会被迫自行实现符号解析 | 在不引入隐式导入的前提下，定义跨文件 `LinkCall` 的准入：仅静态、目标属于显式导入/已锁定资源闭包时可调用；普通 Markdown 链接继续惰性；也可正式决定不支持跨文件 LinkCall，但必须给出唯一替代寻址协议 | 同模块链接调用、跨已导入模块链接调用、未导入/歧义/外部 HTTP 调用拒绝，且仅 `>`/表达式调用能产生 effect；引用不执行 |

**分界警告**：E02/E04/E05 的目标不是“所有行为都通过反射动态注入”。语言表达式和控制流固定，扩展对象必须先有**静态可读取的描述符/签名**，运行时以明确身份解析、再授权、最后分发。不得把“未知函数先试执行看看”当成扩展机制。

### 1A.3 三个共享抽象，不新增一堆 crate

**A. `CallableRegistry`：统一可调用符号视图，不统一其实现语言。**

```text
CallableDescriptor {
  public_name, owner_package, callable_id,
  signature { params, kind, result? },
  effects_requested[],
  origin { builtin | marqdo | abi_plugin },
  implementation_digest?, source_span?
}

Frontend: manifest-only resolve → type/arity/binding diagnostics
Runtime: BoundCallableId → native fn | Marqdo fn | Worker | trusted C ABI
Host: enforce declared+granted capabilities per actual effect
```

- **静态与运行时一套解析结果**：`check` 读取包清单/ABI 导出签名但绝不加载动态库；`run` 根据已锁定身份选择实现，绝不自行寻找“第一个可用同名函数”。
- **核心内置函数允许是静态常量表**；问题在于不应该把**扩展函数**也写进核心枚举。最小改造可以保留 `BUILTINS`，但把调用方解析抽成 `Resolver`，其结果为 `Builtin | Marqdo | Plugin` 三种明确类型；只有外部扩展路径走可注册表。
- **效果判断不靠字符串伪装**：第三方清单声明副作用用于静态报告和权限请求，不代表插件实现已经被证明纯净。

**B. `PackageResolver`：可替换的包来源，不可替换的版本与安全规则。**

```text
PackageSource → Resolve → ResolvedPackage {
  instance_id, origin, revision, content_digest,
  module_root, package_manifest
} → Lock & ModuleLoader
```

`instance_id` 体现锁定后的包实例（可以同时容纳同名不同版本），`module_root` 只是本机物理路径。统一 `ModuleId={instance_id, relative_path}`。初期仅实现本地目录与 Git pinned revision；后续 registry/企业镜像不要求改语言前端。**不做运行期自动 import 猜测**。

**C. `ExecutionProvider`：根据明确协议注册异构执行内核，不重新定义 Marqdo。**

```text
KernelDescriptor {
  provider_id, languages[], wire_version,
  entrypoint, digest, permissions_requested[]
}

Foreign.call(CodeValue, args) → ExplicitProviderLookup → HostAuthorization
                             → IsolatedRunner → TypedResult
```

Python/Node 可以继续作为内置 provider；新增 Julia/R/自定义内核不应触碰解释器、表达式文法或 `HostOp` 主分发。**Tool Protocol（管理外部 CLI）与 Kernel Provider（执行外部语言）绝不能混用**，因为信任模型不同。未安装的语言标签仍是合法惰性 `Code`，只有显式执行时报不可用。

### 1A.4 关于扩展性与安全性的几项不可妥协边界

1. **不能给工具自动授予 `.mq.md` 的运行权限**：CLI Tool 是用户主动执行的程序，和语言沙箱隔离。工具若请求核心运行代码，必须发起新的、独立授权的 `marqdo run/call`。
2. **C ABI 与 Host API 必须可演进**：生命周期、内存所有权、结构体大小/版本协商、错误和 capability 回调是公开合同；如当前 v1 结构妨碍 Worker 等价功能，升级 v2 并保留可见拒绝行为，不在 v1 中悄悄换 ABI 布局。
3. **永远不能通过未知的权限名获得能力**：第三方可以声明 `vendor.*` 类型能力要求，但只作为申请；是否授予、如何执行、如何记录必须由可信 Host/Broker 决定，缺少 provider 就失败。
4. **公开 Analysis v1 必须先解决包感知 ModuleId**：不应把未来必然变化的项目相对路径作为对外稳定知识身份冻结下来。
5. **Trace 是观察，不是执行结果的替代**：只有 `run/call` 执行；`check/inspect/Tool install/View 初始预览` 不能执行 `.mq.md` 或插件初始化。
6. **清单可扩展字段不能“自动有语义”**：`tool.json`、`plugin.yaml`、`marqdo.yaml` 的未知字段允许按各自版本化规则保留或严格拒绝，不能暗中参与运行控制。
7. **进程隔离与声明不混淆**：Native C ABI（进程内）是受信任模式；隔离 Worker 必须具备 OS 级强制边界才可以宣称可以运行不可信原生插件，普通子进程本身不足以提供同等保证。

### 1A.5 破坏性重构策略：允许删错，不允许偷偷改变对外合同

当前项目版本 `0.1.0`，**不对错误设计承担无限兼容义务**。开发者应优先保证未来长期正确性，而不是维护早期内部 enum 的表面稳定。推荐机制：

- **内部接口（可以直接破坏）**：`HostOp` 分发、`Builtin` 内部查找、`PluginSlot` 单 namespace、模块 loader、非公开 IR、CLI 命令内部结构；每次变更要有迁移 PR、替换后的测试和明确的 PR 说明。
- **用户可见语义（默认保护）**：`Strong/Emph/List/Quote` 语义、显式 YAML import、无括号调用的拒绝规则、链接不执行、函数参数推断等。若发现客观错误，也必须有 ADR、版本升级和一致性金样，而不能把“扩展性”当作随意改语法的理由。
- **版本化公共协议（只能显式破坏）**：Tool Protocol、Analysis/Trace、插件 ABI、Value Wire、Lock 格式。新旧同时可用或明确不兼容均可，但不能把 schema 名不变、字段意义变了当作修复。
- **冗余命令（可以有序削减）**：`init`、`inspect`、`fmt`、`plugin inspect`、`remove`、`lock` 等仅作为现有 0.x 行为，保留要以维护收益证明；公开主线禁止因此增加更多命令。删除时提供对应的文档内替代方式，避免把重要能力一起删除。

### 1A.6 对新仓库的直接影响

- `marqdo-knowledge`：只读 Analysis/Source Identity/静态引用，不需要向 Core 注册新的编译器语法，也不要求自带 Rust crate。
- `marqdo-proof`：消费**公开、版本化、语义明确的函数体表示**与已声明契约；若 v1 只支持计算子集，应显式标注，不得根据私有 IR 的 serde JSON 猜语义。
- `marqdo-view`：消费 Analysis + Trace，阅读/渲染仍采用 Markdown，不会因为 `--view` 触发脚本执行。
- 任意第三方开发者：可以独立制作 Tool、ABI 插件或 Kernel Provider，**无需提交 Core PR**；其唯一要求是遵循声明的接口、版本和授权模型。

---

## 2. 完整的目标结构与依赖方向

### 2.1 四个独立仓库，一套用户命令

```text
                    用户：marqdo <command>
                               |
                  marqdo-cli （本仓库）
       ┌───────────────────────┼────────────────────────┐
       |                       |                        |
  内置语言命令             Tool Dispatcher         公共数据协议
  run/call/check          只分发，不实现            Analysis v1
  add/sync               tool install/list/remove   Trace v1
       |                       |                        |
       |                已注册且已校验的工具              |
       |                       |                        |
       |             ┌─────────┼─────────┐              |
       |             |         |         |              |
       |            工具 A    工具 B    工具 N           |
       |        （View/Knowledge/Proof 与第三方平等）    |
       └────────────── 不得有反向编译依赖 ────────────────┘

运行时扩展（与 Tool Dispatcher 不同）：
  .mq.md → Runtime → CapabilityHost → C ABI plugin / Foreign / Math
```

严格约束：`marqdo-cli` 的 `Cargo.toml` 不得依赖 `marqdo-view`、`marqdo-knowledge`、`marqdo-proof`；这三个工具不得把 CLI 私有 Rust IR 或 crate 当作运行期强制依赖。工具通过**已发布 CLI 二进制**与**版本化文件/IPC 协议**协作。

这里的“其他仓库”是三个官方工具仓库，以及未来第三方 CLI 工具或运行时 ABI 插件生态；本仓库的完成标准是**让它们可独立开发**，不是替它们开发业务逻辑。

### 2.2 既有 workspace 的职责不必重新洗牌

```text
crates/
  cli/                  # 唯一 marqdo 二进制，参数、命令分发、JSON 输出
  marqdo-markdown/      # 唯一 Markdown 语法入口；标准 AST + frontmatter
  marqdo-expression/    # 独立的表达式文法和解析器
  marqdo-frontend/      # 语义检查、参数推断、函数/资源/引用绑定
  marqdo-ir/            # **内部**可执行 IR，不承诺跨仓 Rust ABI
  marqdo-runtime/       # Rust tree-walk 原生求值
  marqdo-value/         # 语言值，不要让外部工具直接依赖内部 enum 布局
  marqdo-span/          # 源码位置与诊断基础
  marqdo-host/          # 唯一副作用 API 和预算
  marqdo-sandbox/       # 隔离与 fail-closed 检测
  marqdo-package/       # 项目依赖、锁、环境同步
  marqdo-math/          # 公式值及受支持计算子集
  marqdo-foreign/       # 可选外语内核（不做 Marqdo 后端）
  marqdo-plugin-abi/    # ABI v1 loader 与契约
  marqdo-plugin-sdk/    # 第三方插件开发便捷封装
include/
  marqdo_plugin_abi_v1.h
spec/                     # 当前冻结的语言和 wire 规范
  analysis-v1.md          # 本轮新增，包含 JSON Schema/示例
  trace-v1.md             # 本轮新增，包含事件格式/示例
  tool-v1.md              # 本轮新增，包含工具清单、分发/安全协议
  ...                     # 保留现有 spec

docs/
  NEXT_PHASE_IMPLEMENTATION.md   # 本文
  STATUS.md                      # 实际完成状态（必须实时更新）
  adr/                           # 重大语义/兼容性变更记录
  DEVELOPMENT_BLUEPRINT.md       # 历史 M0.5 设计参考
```

**新增的是三个小协议文件和经审计确实需要的注册接口，不是三个新的编译器 crate**。如果 `crates/cli/src/commands.rs` 已过长，可以将分发代码分成 `tool.rs` / `analysis.rs` / `trace.rs` 模块；除非真实模块耦合与编译时间数据支持，不应因“未来扩展性”再建立十多个 crate。

### 2.3 单向依赖纪律

- 语法层只产生 Markdown AST，不认识包、插件、知识图与 UI。
- Frontend 负责类型、符号和引用，不访问 Host/系统副作用。
- Runtime 消费 IR，并且只能通过 Host 请求外部能力；所有可调用函数先使用静态描述符完成唯一解析，运行时采用已绑定目标，第三方函数不增加 `builtins.rs` / `builtins_impl.rs` 的硬编码分支。
- Package 为 loader 提供确定的 `ResolvedPackage` 和锁文件；loader 不检查 `path/git/registry` 的来源细节。模块身份独立于物理缓存路径；不改语言表达式语义。
- Foreign 的语言 Provider 通过独立注册清单加入，Python/Node 是预置实现而非永远唯一实现；未经授权不得启动进程。
- ABI 插件通过**完整导出名**注册多个 callable，不能从第一条导出推断唯一命名空间。
- Tool Dispatcher 不链接外部工具的业务库；只负责**通用清单**的验证、安装登记、版本协商和进程启动；不得含有 `view/knowledge/prove` 之类的命令白名单、固定 URL 或专门适配器。
- Analysis/Trace 导出器读取核心已产生的数据，禁止维护另一套符号解析器。
- 外部工具不能反向要求核心增加 `knowledge`, `proof`, `view` 专用类型或运行逻辑。

## 3. 语言行为在本次迭代中保持冻结

### 3.1 Markdown 结构的唯一语义

在 **level-2 函数体内部**（当前 ADR-0002）：`Strong` = 计算/赋值；`Emph` = 退出函数并返回；`OrderedList` = 有序首匹配分支；`BulletList` = 条件循环；`BlockQuote` = 显式调用；`Text` = 惰性叙述；`CodeSpan` 在分支/循环头的特定结构位置表示条件；`Link` = 静态引用；数学节点与代码围栏 = 原生惰性资源。一级标题给命名空间，二级标题给函数，三级及以上标题给文档分节。

函数体外不执行程序；现有 ADR-0002 规定可疑“执行节点”惰性并告警，而不是直接在模块根运行。除非另提 ADR，不要偷偷改为硬错误。源码同义 Markdown 结构（例如 `**x**` / `__x__`）按同一 AST 语义执行；也不因为正文中文有“如果/返回/执行”就推断控制流。

```markdown
# 数学

## 阶乘

输入 `n`，计算非负整数的阶乘。

1. `n <= 1`，递归基。

   *1*

2. `n > 1`，继续递归。

   **结果 = n * 阶乘(n=n-1)**

   *结果*
```

期望：`marqdo call factorial.mq.md 数学.阶乘 --arg n=5` 返回 `120`。实际放置测试时以现有 `examples/factorial.mq.md` 为准并追加金样，**不要为了执行把示例转换成 Python**。

### 3.2 函数与参数推断必须严格而可检查

- 标题保持自然名称，如 `## 计算圆面积`，**不要把参数名强制放进标题**；标题变更可能影响 Markdown 锚点，参数变更不应顺便破坏链接。
- 显式签名放在约定的标题后独立 `CodeSpan` 段落，例如 `` `(r, precision=2)` ``；无签名允许静态形参推断。
- 参数推断源于执行表达式及控制流的自由变量/确定赋值分析，不从叙述文本或 LLM 中猜测；可见未初始化局部变量应报错。
- 推断签名默认只接受具名调用，显式签名可接受位置调用；无参数调用仍用括号 `()`；调用的解析器只有一套。
- 重复形参、错误实参、未解析别名、变量读写歧义产生稳定诊断码。

保留当前 `crates/marqdo-frontend/src/{infer,signature,calls}.rs` 的既有机制，补完真实用例和跨模块错误位置；不重写一套“智能推断”。

### 3.3 求值与资源边界

- 表达式解析来自 `marqdo-expression`，不是“普通 Markdown 括号里藏 Python”。
- 既有 ADR-0007 采用任意精度整数及类型化 wire v1；浮点、布尔和 `Unit` 语义以 `spec/evaluation.md` 为准。变更必须同步 ABI/Foreign/wire 的兼容性测试。
- `OrderedList` 的首个条件头为符合结构规定的 `CodeSpan`；同一段落里条件后的叙述和后续缩进段落不参与条件求值。无 `CodeSpan` 的清单在函数内是非法分支，不回退成普通清单。
- `BulletList` 的每个 item 是一个 `while` 条件与循环体；不默认把 `- [ ]` 任务清单当普通 UI 清单。
- 引用块一次只表达一个显式调用；链接形式调用是否跨文件支持由 `spec/references.md` 目前的限制决定，不能为了满足文档示例绕过模块 loader。
- 公式和代码块必须先构建惰性值，只有 `math.*`、`foreign.*` 或明确 Host API 才能执行；不自动运行任意外部语言。

### 3.4 引用的最小承诺

区别 **import（模块加载与别名）**、**reference（Markdown 静态链接）**、**call（执行函数）**。静态 Knowledge 前置只需要可查询 **包实例感知的** `ModuleId`、`UnitId`、`FunctionId`、签名、source span、解析目标及 `ref` 状态。引用不自动获得执行权。缺文件/歧义/循环须可诊断。现有 path-based 身份和导航 anchor 可以作为首版基础；不要声称“函数改名也能永久稳定引用”，因为还没有显式持久标识系统。

**明确暂缓**：句子级知识抽取、证明注解、自定义知识节点语法、复杂内部段落锚点、知识库搜索、Lean 接入。它们应由 Knowledge/Proof 独立决定需求，再以增量协议演进。

## 4. CLI 命令的“功能够用、入口极少”政策

### 4.1 正式推荐的公共主路径

| 用户命令 | 责任 | 所属位置 |
|---|---|---|
| `marqdo run [file]` | 执行默认入口 | CLI 核心 |
| `marqdo call <file> <fn> --arg x=...` | 调用命名函数 | CLI 核心 |
| `marqdo check [file]` | 静态检查、`--refs`、可选公开分析制品 | CLI 核心 |
| `marqdo add <name> ...` | 添加依赖到项目清单 | Package 表面 |
| `marqdo sync [--locked]` | 锁定并同步项目环境 | Package 表面 |
| `marqdo tool install/list/remove` | 管理已注册外部 CLI 工具 | 轻量命令路由 |

安装任意合法工具后，都会根据其 `tool.json` 中声明的 `command` 获得 `marqdo <command>` 入口。例如 `view`、`knowledge`、`prove` 与第三方 `benchmark` **地位完全平等**，核心不定义这些命令的业务参数，也不为这些名称特判。

常见工作流：

```bash
marqdo check index.mq.md --refs
marqdo run index.mq.md
marqdo call index.mq.md 数学.阶乘 --arg n=5
marqdo add geometry --path ./packages/geometry
marqdo sync --locked
marqdo tool install ./downloads/marqdo-view/tool.json
marqdo view .
```

### 4.2 现有额外命令如何处理（不可粗暴删除）

现有代码里已经有 `init`、`inspect`、`fmt`、`plugin inspect`、`remove`、`lock`。它们不需要立即删除，也不需要为了符合“六个入口”在一夜之间制造破坏性变更。

| 现有命令 | M0.6 决定 | 为什么 |
|---|---|---|
| `init` | 保留当前行为，不增加模板系统；不列为生态准入门槛 | 一次性项目便利功能，已实现，删除没有回报 |
| `inspect` | 暂保留为兼容/开发诊断入口；正式跨仓数据改为 `check --emit` | 不依赖私有 `--ir` 文字输出建立协议 |
| `fmt` | 保留当前保守实现，后置完整格式化 | 格式化 Markdown AST 风险高，无必要扩张 |
| `plugin inspect` | 保留现有安全检查用途，不增加插件生态管理子命令 | 旧 ABI 审查与调试已在使用 |
| `remove` | 保留轻量依赖清单操作 | 已有操作，不是新体系成长方向 |
| `lock` | 保留现有稳定入口；未来可把锁解析与 `sync` 收敛 | 不能静默改变现有脚本依赖 |
| `check --emit` | **本轮新增选项**，不新增 `analyze` 顶层命令 | 公共语义分析协议唯一入口 |
| `run/call --trace` | 演进为写出版本化 trace 文件（保留现有文本/JSON 行为） | View 不依赖私有日志字符串 |
| `tool` | **本轮唯一新顶层命令** | 独立生态所必需 |

禁止本轮新增 `knowledge`、`prove`、`view` 的**内置实现**，也禁止 `analyze`、`env`、`kernel`、`sandbox`、`debug`、`tree`、`update`、`vendor`、`registry`、`doctor`、`tool register` 等新顶层或工具管理命令。未来需求须经过真实用例与独立 ADR 论证。

### 4.3 `check --emit` 的含义

`marqdo check file.mq.md --emit analysis.json`：不执行源码、不触发插件加载/外语进程、不修改源文件；校验完成且没有错误后，将**公开且结构化**的 Analysis v1 原子写入指定文件。错误时非零退出，输出诊断而不留看似成功的旧产物。`--json` 仍只负责用户/CI 的诊断输出，不取代版本化 analysis 文件。

`marqdo check` 目前默认只检查入口及导入模块；若未来增加“整个项目目录”模式，必须单独测试发现范围与软链接/忽略规则，不默认递归扫描全部未知文件。**在冻结 v1 之前，ModuleId 必须使用包实例逻辑身份而不是全局物理缓存路径**，且必须暴露第三方 callable 的静态签名与来源。

### 4.4 `run/call --trace` 的含义

现有 `--trace` 可以继续控制终端/JSON 中的效应跟踪。新增独立落盘参数的候选形式为 `--trace-file <path>`，而不是重载 `--trace` 的原有布尔语义。Trace v1 应通过原子文件写入，带 run id、schema version、 source span、事件顺序、状态及红acted Value，默认不可包含密钥。不要为此增加 `marqdo trace` 命令。

## 5. `marqdo tool`：完全通用的独立工具接入协议（通用协议 v1，规范性）

### 5.1 核心决定：CLI 绝不定义具体工具

任何官方或第三方工具**只需符合公开、版本化的 `Tool Protocol v1`**，提供自己的可执行程序和 `tool.json`，由用户显式安装后，就能自动获得 `marqdo <command> [args...]` 入口。**不允许**为 `view`、`knowledge`、`prove`、`benchmark` 等名字增加 `clap` 专用变体、匹配分支、编译依赖、固定发布下载地址或特殊行为。`marqdo-cli` **不提前认识任何工具名称**，只保留原生命令集合与注册项冲突检测。

这是**命令分发扩展**，不是运行时插件 ABI。外部工具独立进程，可用 Rust/Go/Python 等语言实现；C ABI 插件属于 `.mq.md` 运行时可调用函数，有独立的安全和生命周期模型。不要把两者合并。

`Tool Protocol v1` 完整契约须单独落地为 `spec/tool-v1.md`。任何新功能都先在协议层判断是否通用，绝不可因官方工具提出需求而在 CLI 中写定制适配器。

### 5.2 极少的用户命令

```bash
marqdo tool install ./downloaded-tool/tool.json  # 从用户提供的本地清单安装（首版必须支持）
marqdo tool list
marqdo tool remove benchmark
marqdo benchmark run .                     # 自动分发；无 CLI 代码变更

# 将来有可配置且验证过的索引服务时才允许这种简写：
# marqdo tool install benchmark
```

CLI 内置的仅是 `tool install/list/remove` 和**未知顶层命令回退至已注册工具**。不增加 `tool register/update/doctor/search` 等首版管理命令；不要求安装社区注册表，不在缺少工具时自动安装或联网。

### 5.3 通用清单协议：字段与示例

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

`entrypoint` 是工具安装包中的相对路径，**不允许绝对路径、`..`、指向包外的 symlink、路径穿越或隐式 shell 执行**；内容哈希在安装后注册为不可变校验元数据。外来发行包的 SHA-256 **不能单独证明来源可信**：需要用户明确选择来源，或使用未来受信索引中提供的可信摘要/签名。首版以本地来源为最低必须实现能力。

`name` 用于登记和显示；`command` 唯一决定可使用的顶层子命令。`command` 必须满足小写 ASCII 标识符规则并拒绝原生命令冲突；`compatibility.analysis/trace` 仅在工具真正需要相应协议时声明（没有声明就不要求安装或生成这些数据）。清单不允许声明权限自动授权、后台服务自动启动或安装后任意脚本执行。

### 5.4 通用分发算法：名称来自清单，不来自源代码

```text
marqdo <first> <rest...>
  ├── first 属于 CLI 已有内置命令 → 执行核心命令
  ├── first 命中已注册 ToolManifest.command
  │      ├── 校验 manifest schema、版本范围、平台和可执行文件摘要
  │      ├── 通过直接 OS process spawn/exec 启动登记的 entrypoint
  │      ├── 原样转发 rest...（OsString，禁止 shell 字符串拼接）
  │      └── 继承 stdin/stdout/stderr，传播退出码/终止状态
  └── first 未命中 → 未知命令错误；提示显式安装，不搜索/运行 PATH 中的可疑程序
```

工具本身处理 `--help`、私有子命令和参数；CLI 只负责可验证的命令分发。Rust `clap` 前端可用统一的 `external_subcommand` 捕获未知顶层命令（参数使用 `OsString`），**不能**为每个工具向 `Command` enum 增加一个变体；仅当通用 fallback 已通过完整参数/退出码测试后采用此实现。建议额外传递 `MARQDO_BIN`（当前核心二进制绝对路径）和 `MARQDO_TOOL_PROTOCOL=1`，供工具按需执行 `marqdo check --emit ...`、`marqdo run --trace-file ...`。不自动给予工具沙箱权限，不注入密钥，也不暗示独立进程受 `pure/project/strict` 模式保护：**第三方 CLI 工具是用户明确运行的普通进程，本身不在 Marqdo `.mq.md` 沙箱内**。需要限制工具自身权限时，应另用 OS 安全隔离，不可依赖 `CapabilityHost` 的语言级约束。

### 5.5 安装与版本：工具索引是可选服务，绝非硬编码白名单

`marqdo tool install <path-to-tool.json>` 是 M0.6 必需闭环。CLI 从本地显式选择的工具包验证清单、版本、文件、平台后，原子拷贝到用户级受控目录，生成注册记录（至少命令、来源、版本、程序 SHA-256、协议兼容信息），以后仅由注册记录定位执行文件。安装时绝不运行工具；卸载删除注册项及它所管理的安装制品。保留用户无关的其他安装，失败需保持原状。

未来若要实现 `marqdo tool install view` 这样的**名称式**安装，必须依赖可配置、可替换、可审计的**索引来源**解析 `name → 可信发行清单地址及摘要`。`view`、`knowledge`、`prove` 不能被写死在核心仓库：官方工具和第三方工具采用完全相同的索引条目结构及安装流程。**没有索引时仍然可以从本地清单安装**，不能将建设在线市场变成 M0.6 的依赖。

包管理中的 `marqdo add`（项目 `.mq.md` 依赖）、Tool 安装（外部 CLI 可执行工具）和运行时 ABI 插件各用各的清单与安全边界，不能共享一套模糊注册逻辑。

### 5.6 最小兼容与独立工具黑盒测试

必须构造**至少三个名称均未出现在 CLI 源代码中的测试工具**（如 `benchmark`、`lintx`、`custom-abc`）；它们只使用 `tool.json` 和独立可执行文件，不链接 CLI 私有 crate。测试运行时创建临时用户 Tool registry，不污染真实系统。

正例：安装任意名称、自动创建同名顶层入口、原样转发空格/CJK/非 UTF-8（平台支持时）参数、保留工作目录及 stdin/stdout/stderr、正确传播成功/失败退出码、卸载后命令不可用、工具版本和可选 Analysis/Trace 兼容协商。

负例：原生命令覆盖、重复命令、损坏清单、旧版 `schema_version`、不兼容核心版本、被篡改的二进制、路径穿越/symlink 逃逸、恶意 PATH 同名程序、权限错误、原子安装失败、并发安装/卸载和临时目录残留。核心 CI 必须证明：**更换任意工具名称、业务参数、编程语言或工具仓库，不要求改动 `marqdo-cli` 源码、命令枚举和 Cargo.toml。**

## 6. 公共 Analysis v1：三仓独立开发的最小契约

### 6.1 为什么不直接导出内部 IR

私有 `marqdo-ir` 可以为解释器性能、诊断或优化重构。外部 Proof/Knowledge/View 若依赖 Rust enum 的内存布局和 crate 版本，四个仓库仍然被迫同步发布。公开协议应该是**为外部读取而设计的语义投影**，包含真实语义而不暴露内部实现指针/缓存结构。

Analysis v1 应当**可以**支持：Knowledge 确定建立函数/引用/调用图；View 对齐正文与可执行步骤；Proof 对**受支持的纯函数子集**构造证明义务，不必重新解析 Markdown 或表达式。

### 6.2 建议的最小机器结构

```json
{
  "schema": "marqdo.analysis",
  "schema_version": 1,
  "producer": { "name": "marqdo", "version": "0.1.0", "markdown_profile": "marqdo.md.v1" },
  "project": { "root_id": "sha256:<normalized-project-digest>", "lock_digest": "sha256:<lock-file-digest-or-empty>" },
  "modules": [
    {
      "id": "root:default::index.mq.md",
      "package_instance_id": "root:default",
      "relative_path": "index.mq.md",
      "source_digest": "sha256:<file-digest>",
      "units": [ { "id": "root:default::index.mq.md#数学/阶乘", "kind": "function", "name": "阶乘", "span": { "start": 0, "end": 100 } } ],
      "functions": [
        {
          "id": "root:default::index.mq.md::数学.阶乘",
          "unit_id": "root:default::index.mq.md#数学/阶乘",
          "signature": { "kind": "inferred", "params": [ { "name": "n", "required": true } ] },
          "effects": ["pure"],
          "body": { "kind": "branch", "arms": [ "<illustrative normalized nodes>" ] }
        }
      ],
      "imports": [],
      "references": [],
      "calls": []
    }
  ],
  "diagnostics": []
}
```

这是**字段布局说明，不是完整合法的生产 JSON**：`root:default` 属于逻辑包实例的示意形式，最终身份编码必须通过 R1 ADR 和协议规范冻结；尖括号字符串和 `body` 示例只代表需要进一步定义的节点；实现前必须在 `spec/analysis-v1.md` 提供一份**完全合法、能由真实 CLI 生成**的 JSON fixture 和相应 JSON Schema，并确定 byte span 的含义、路径转义、数组顺序与缺失字段规则。

MUST 导出：版本、模块及原文摘要、稳定函数/章节身份、源码字节跨度、签名（含显式/推断来源）、解析的 import/reference/call 关系、静态效应的保守摘要、诊断、**受限的规范化函数体语义树**。

特别注意 Proof 的需求：若只输出“函数名、源码、调用列表”，它仍需要自己重写表达式及控制流分析，违背解耦。因此应提供**规范化的语义节点**：`literal` / `variable` / `unary` / `binary` / `call` / `assign` / `branch` / `loop` / `return` 等；每个节点携带 span、类型/效果信息和解析后的调用目标。若节点超出 M0.6 公开子集，标记 `unsupported`，Proof 必须保守拒绝，不得假定其语义。

这份语义树**不是** `marqdo-ir` 原始 serde dump，不能暴露 Rust enum 具体序列化形状、内部优化指令或私有 ID。其稳定性由协议测试保证，不由内部 Rust trait 保证。

### 6.3 精确兼容规则

- `schema_version` 是单调的主版本；不兼容语义改变必须升主版本。
- 允许已知字段之外添加可忽略的可选字段；删除字段、变更单位/含义、改变值类型则是破坏性升级。
- 文档链接解析失败不能被伪装为已绑定；保留 `unresolved/ambiguous/external` 三态以及诊断。
- IDs 只在**指定项目路径与标识规则**范围内承诺稳定，重命名标题或移动文件可能改变身份；`definition_digest`/锁摘要属于版本而非身份。
- 索引、模块、依赖、声明、调用边必须有确定性排序；相同源码与工具版本多次导出完全一致（除文档明确标记的时间/环境字段，不应有时间戳）。
- 运行效果不能由静态描述直接宣称为完成，`effects: ["pure"]` 是经过保守检查的语言内效应摘要；动态 ABI/Foreign 目标不透明时应标记 `unknown` 或 `external`，而不是假定纯净。
- `check` 和 Analysis 导出绝不执行程序，绝不把任何证明状态写成 `proved`，不加载外部插件。
- `source_digest`、`lock_digest`、`producer` 是后续证据失效检查的必要最低字段；不可用时应声明缺失/未知而不是编造哈希。

### 6.4 独立编译消费测试

写一个使用**另一语言**的小型 consumer（例如 Python 只用于读取 JSON，而非执行 Marqdo），不链接仓库 Rust 代码，读取真实 `check --emit` 结果：解析函数签名；找到递归调用；找到 `ref` 边；读到规范化返回表达式；重建源位置并检查 `math` 资源所在章节。版本不匹配应明确拒绝。加入金样 schema 及多次字节级确定性测试。

## 7. 公共 Trace v1：View 使用的证据，不是另一个执行引擎

Trace v1 是可选运行事件流，与 Analysis v1 用 `FunctionId`、`UnitId` 和 byte span 关联。M0.6 最小版本包括：

```json
{
  "schema": "marqdo.trace",
  "schema_version": 1,
  "run": { "id": "<random-run-id>", "entry": "index.mq.md::main", "status": "ok" },
  "source": { "root_id": "sha256:<digest>", "analysis_digest": "sha256:<digest>" },
  "events": [
    { "seq": 0, "kind": "call.enter", "function_id": "index.mq.md::main", "span": { "start": 50, "end": 90 } },
    { "seq": 1, "kind": "call.exit", "function_id": "index.mq.md::main", "result": { "type": "int", "decimal": "42" } }
  ]
}
```

同样只是协议示例，必须以现有 `spec/value-wire.md` 的**真实类型化值编码**为准，不能因为示例而新造另一种整数/浮点格式。必须显式定义：`event` 顺序、父调用帧、函数/语句源跨度、输出事件、错误事件、结果值的大小上限、超时/中断状态、隐私脱敏，以及 trace 与当前源码 digest 不一致时的 `stale` 状态。

View 不允许通过“只读模式”调用 `run` 来获取结构；只读阅读从 `.mq.md` 与 Analysis 构建。Trace 只有**用户已明确运行或读取已有记录**时才能使用。`foreign`、ABI 插件和敏感值必须默认脱敏，不能自动记录密钥与文件内容。超时/中断要尽力写出明确的终止状态，不能把不完整文件标为成功轨迹。

首版 Trace **不要求**交互断点、调试服务器、时间旅行、Notebook 会话或 WebSocket。这些是 `marqdo-view` 将来自行设计的后续产品能力。

## 8. 包管理和项目环境：最小但可信

### 8.1 维持两级依赖声明

- 项目 `marqdo.yaml`：声明路径依赖、受支持的 Git pinned 依赖、能力请求、可选插件需求。
- 文件 frontmatter 的 `import:`：显式声明该文件真正使用什么模块与本地别名。
- `marqdo.lock`：具体已解析依赖的内容/修订指纹；不能被普通 `.mq.md` 修改。

**不启用**执行时根据缺失函数名扫描整个项目/网络来自动选库；静态分析依赖边可以自动，但依赖来源必须确定。`[链接](...)` 只产生知识引用，不是 import。

### 8.2 本轮做完什么就足够

1. `add --path`、路径依赖锁与 `sync --locked` 继续受支持，兼容现有测试。
2. 给 Git 依赖实现 `git + 精确 commit` 的下载、缓存与锁定（禁止浮动分支充当可重复的 commit）；传递依赖解析要有循环检测和冲突诊断；实现过程必须禁止安装脚本/构建脚本在未获授权时执行。
3. `sync` 从“只检查”补成**真实环境物化**：解析至 project-private `.marqdo/env`，共享内容寻址下载缓存但不共享可变执行状态；同步完成是原子操作。版本/哈希不匹配失败，不能自行改成最新包。
4. 锁定模式下无缓存且无法联网时明确失败；完整的离线工作流可以作为测试，用预热缓存验证不触网能恢复环境。无需新建 `env` 命令。
5. `run/call/check` 根据入口文件的最近合法项目清单确定根目录；没有清单的单文件脚本仍必须可执行。多项目嵌套、路径越界与别名冲突需可诊断。
6. Python 的 uv/venv 属于 **Foreign** 环境能力，不能直接替代 Marqdo 的项目环境；本轮可优先支持显式发现已有 venv，随后支持受控的 Python 环境物化，但不得因没有 uv 而不能运行纯 Marqdo 程序。

**后置**：包注册表/账户/发布服务、复杂版本求解器、`vendor/tree/update/registry/env` 命令、所有语言的环境管理和多平台预编译包分发。参考 Cargo lock 和 uv 的**体验**，不是照搬它们的全部工程规模。

## 9. 沙箱、权限与进程边界

### 9.1 当前安全事实

现有 `pure/project/strict` 三模式与 CapabilityHost 已存在。`strict` 当前没有 OS 实施适配器，已按 ADR-0004 拒绝执行；这比假装启用沙箱安全得多。`project` 是**合作式权限检查**，不能保证原生扩展或子进程无法绕过；不可以在文档中称其为安全隔离。

### 9.2 本轮最低承诺

- 所有 Host 操作在真正副作用前做能力检查；权限由用户/宿主显式批准，项目清单内 `capabilities` 只是申请，不是授权。
- 资源预算（fuel、stack、timeout、output）贯穿解释器；补齐内存上限与多层 Foreign 子进程清理时必须使用真实 OS 技术，不能只拦截 Rust 容器分配。
- 外部文件路径防止越界、symlink 与 TOCTOU；重点测试写入时检查与打开之间的竞争条件。过不了就不能承诺严格文件隔离。
- 不可信 `.mq.md` 在 `strict` 下必须实际运行于受 OS 隔离的进程/容器边界；平台没有相关能力时拒绝，无降级选项。
- `check`、`check --emit`、`tool list`、`plugin inspect` 都不能执行不可信程序；`tool install` 仅安装/校验工具，不在安装时运行其任意 postinstall 脚本。
- 原生 C ABI 插件只允许明确受信任的进程内路径；**任何**声称支持“不可信第三方插件安全执行”的版本，必须先实现隔离 worker 以及真实的边界测试。
- 外部 CLI 工具本身也是独立本地可执行程序，只有用户显式安装/启动后执行；它**不继承** Marqdo 语言虚拟机的 `pure/project/strict` 沙箱承诺。

### 9.3 渐进实施与声明

M0.6 至少实现一个平台（优先 Linux）的严格隔离闭环，或将 strict 功能继续明确标为 unavailable，**不得将缺失的 strict 作为已完成安全能力发布**。若项目发布条件需要“可在不可信环境运行”，就必须在受支持的平台实现并通过 OS 层隔离实测；若只是开放外部工具开发，现有 fail-closed 行为可以允许启动 Alpha 消费者开发。安全发布声明必须按平台和执行模式分别表达。

必要负例：没有授权时的 fs/foreign/plugin 访问；写入路径逃逸；子进程树终止；native 插件崩溃隔离（worker 模式有时）；超预算 loop；有权限与无权限同时存在的子任务；严禁只靠 mock 检查声称“OS 沙箱已验证”。

## 10. 运行时 ABI 插件：稳定 C 边界，默认不可信不得进程内执行

### 10.1 现有 ABI v1 是新仓库自己的版本

`include/marqdo_plugin_abi_v1.h`、`marqdo-plugin-abi`、`marqdo-plugin-sdk` 已有基础，不继承旧仓库 ABI v2，也不允许以 Rust 私有 ABI 作为第三方稳定合约。JSON/typed wire 在边界使用 `spec/value-wire.md` v1，内存谁分配谁释放必须逐条定义。ABI major 不兼容时必须拒绝，而不是尝试猜测布局。

### 10.2 两类部署方式

| 模式 | 面向 | 边界 |
|---|---|---|
| Trusted Native | 明确受信任的本地原生扩展 | 可进程内调用；崩溃/UB 可影响解释器；CLI 必须要求显式信任 |
| Isolated Worker | 普通第三方、实验性或用户未知来源扩展 | 独立进程 + 有界 IPC + 能力代理 + 超时/崩溃隔离；不满足隔离条件就拒绝 |

建议隔离 worker 第一版采用长度前缀帧 + typed JSON wire（使用已有 v1 语义、单消息大小限制），明确 `handshake / invoke / result / error / shutdown`、最大帧长度、协议版本、超时/kill、插件 crash -> `plugin.crashed`、错误码传递和能力请求。C ABI 动态库只在 worker 内加载。不要本轮同时做 Wasm Component Model、新插件市场、Hot Reload、异步回调及复杂宿主对象反向调用。

SDK 需提供最小“加法函数”示例，分别用 C 和 Rust 编写，用与主运行时隔离的编译环境构建。兼容性测试必须验证 CLI/SDK 独立构建和版本不匹配失败。第三方插件不能通过自己的 manifest 自授予文件/网络权限。

### 10.3 ABI 与 Tool 的铁律

`marqdo tool install ./downloaded-knowledge/tool.json` 安装的是**CLI 外部可执行工具**；`marqdo` 语言内调用 `solver.add(...)` 使用的是**C ABI 插件**。不能共用注册表和权限模型；它们的生命周期、数据协议、失败语义完全不同。

## 11. Math 与 Foreign：当前子集足够，先做正确性与隔离

- `Formula` 与 `Code` 由标准 Markdown 节点构建，静态检查绝不执行。
- Math 保留当前 exact rational 子集，`math.simplify` / `math.differentiate` 若未支持，返回 `math.unsupported`；**不为了“完整版知识系统”扩大成复杂 CAS**。
- Foreign 目前有 Python/Node 一次性模式，作为运行语言的外联能力。进程生命周期、限额、output/wire、错误转换必须可验证；子进程树隔离在严格运行模式下尤其重要。
- Python `venv`/uv 依赖管理可作为 Foreign 环境适配逐步实现；首先维持对显式配置的 Python 路径及环境变量的良好行为，避免启动时偷偷安装网络依赖。
- 无 Python/Node/Lean 时，原生 Marqdo `run/call/check` 与基本 Math 必须继续工作。

## 12. 未来三个官方工具仓库的真实合同

### 12.1 `marqdo-knowledge`

能直接读取 Analysis v1 并找到函数/公式/静态引用、模块/调用边、精确源码位置和内容指纹；可据此输出自己设计的知识 IR、查询、OKF 交换或图。Core 不实现 `Knowledge IR`、知识库、知识证明、向量索引，也不为 Knowledge 改写 Markdown link 语义。

### 12.2 `marqdo-proof`

能直接读取 Analysis v1 公开的**表达式和控制流语义投影**，只对受支持的纯函数片段构造证明义务。不重新写一个 Markdown parser，不将“引用了某定律”当成证明，不用 `proved: true` 等本地 JSON 标记冒充 Lean 证书，不将 `Int`/`Float`/插件副作用任意映射成数学 `Int` 后宣称运行时正确。

Core 不包含 Lean、Proof IR、证明驱动器或证书存储。新的证明器需要更多数据时先提出最小 consumer fixture，再经 Analysis v2 或兼容可选字段扩展；不向 CLI 内部塞 `prove` 命令。

### 12.3 `marqdo-view`

默认按原 Markdown 阅读，不执行；使用 Analysis v1 对源码 Strong/Emph/分支/循环和函数做精确标注。存在真实 Trace v1 时，可选择播放调用帧与历史结果；若用户选择运行，View 须明确发起 `marqdo run/call` 并尊重沙箱与权限。它不实现第二个解释器、第二个静态语义分析器，不将未运行的语义节点当成已执行结果。

### 12.4 对外工具的独立性条件

三个仓库独立版本，独立发布，可缺席；它们的引导程序只需要找到 `MARQDO_BIN` 和版本化协议，不需要把 `marqdo-runtime` 链入自身。**删除三个仓库中任一个，本仓库 build、test 和普通 run 不受影响。**

## 13. 修订后的实施顺序：先消除扩展阻塞，再冻结公共协议

> **优先级新决定**：原 v2 方案将 Analysis 和 Tool 排在最前，但源码审计发现**包身份、Callable 注册和插件多导出路由以及安全句柄身份**是它们的先决条件。不能先冻结带错误 `ModuleId`/函数解析的 Analysis v1，再向外发版后被迫迁移三个工具。

### R0：基线、状态与可破坏边界（所有重构的前置条件）

- 先运行现有 `cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`，记录当前精确 HEAD、Linux CI 和实际 golden 数量；失败则先分类，不拿旧 `docs/STATUS.md` 的描述代替证据。
- 对 `spec/mapping.md`、`spec/evaluation.md`、`spec/value-wire.md`、`spec/references.md` 中真正的语言不变量建回归测试。将已知状态表矛盾修正（E14）。
- 写 ADR：包实例身份、外部可调用符号解析、插件多导出、安全能力 Broker、外部 Kernel Provider；这些 ADR 决定之后才能公开冻结 Analysis。
- **验收**：基线 CI 可重现；每一项破坏性更改有对应 ADR/旧失败用例/新成功用例；不碰旧 `cflmy/marqdo`。

### R1：包实例身份与统一符号解析（P0 核心架构修正）

- **主要落点**：`crates/marqdo-ir/src/lib.rs`、`crates/marqdo-frontend/src/{calls,units}.rs`、`crates/cli/src/loader.rs`、`crates/marqdo-package/src/lib.rs`。
- 以 `PackageInstanceId+relative_path` 替换跨项目不稳定的 `ModuleId` 公开概念；将 package source resolution 从 CLI loader 抽离；实现 path 与 Git pinned revision；lockfile 明确源码 digest 和实例 ID，多版本不可混同。
- 为**外部 callable**新增由清单导出的签名/归属/效应信息，统一前端与运行时的符号解析。保留核心内置表的受控白名单，但不要让第三方导出依赖它。
- **验收**：双版本依赖、移动缓存目录、跨模块真实调用、跨包错误跨度、静态检查第三方函数参数，均通过。未知/重名符号不得退化为动态猜测。

### R2：ABI 多导出、权限 Broker 与内核注册（P0 扩展实现）

- **主要落点**：`crates/marqdo-host/src/{host,capability,kernels}.rs`、`crates/marqdo-runtime/src/builtins_impl.rs`、`crates/marqdo-plugin-abi/src/{manifest,loader}.rs`、`crates/marqdo-foreign/src/lib.rs`。
- 完整导出函数路由替换 `exports.first()`；启动前静态取得所有 export 描述符；运行时只根据绑定后的 `CallableId` 调用。
- 实现 Host Capability Broker（明确的请求/授权/校验/审计），插件自定义权限字符串不能隐式生成能力；实现可注册的 Foreign Kernel Provider，先添加测试用 mock provider 再验证 Python/Node 兼容。
- 设计并实现 IPC Worker 的最小纯函数路径；Native 模式仍要求用户显式信任，不把 worker 进程自动当作强沙箱。Wasm 先按不支持处理即可。
- **验收**：一个插件双命名空间、两个插件导出冲突、未知能力拒绝、插件越权、**伪造/跨会话句柄拒绝**、mocklang 内核注册、worker 超时和崩溃隔离全部通过。若变更 C ABI 布局必须显式升级 major 并同步头文件/SDK/C 示例。

### R3：Tool Protocol v1 与统一 CLI 入口（P0 外部开发所需）

- **主要落点**：`crates/cli/src/main.rs`、`crates/cli/src/commands.rs`；可按实际需要新增 `crates/cli/src/tool.rs`，不强制另立 crate。
- 以**用户已注册清单**驱动未知命令分发，不允许 `View/Knowledge/Proof` 特例或 PATH 上未授权的程序自动执行；支持 `tool install/list/remove`。
- `entrypoint` 只能是包内受控可执行文件；注册与摘要校验原子化；命令行参数 `OsString` 原样传递，子进程继承约定的 stdio/cwd，退出码原样传播；不要通过 shell 调用。
- **验收**：自行生成三个未被硬编码的名称，至少其中一个是 Go/Python/简单 C 生成的不同运行时工具，验证无需重新编译 Core 即可替换工具；测通安装失败回滚/篡改拒绝/冲突拒绝。

### R4：公开 Analysis v1 与 Trace v1（P0 独立 Knowledge/Proof/View 的接口）

- **主要落点**：`crates/cli/src/{commands,output}.rs`、现有 frontend/IR 源码位置，`crates/marqdo-host/src/trace.rs`、runtime 事件发出位置。可用单独 `analysis.rs` / `trace.rs` 模块，但不能新增大型 crate。
- `check --emit` 只读、无执行，生成 `schema_version`、Markdown profile、锁闭包 ID、模块/函数/签名、结构化函数体投影、refs/calls、effects、source hashes 和诊断；不暴露私有 `marqdo-ir` 的 serde 布局。
- `run/call --trace-file` 生成可关联函数/事件/source span 的类型化轨迹；默认对 secret/handle 等敏感值做不可逆遮蔽，明确截断与限额。
- **验收**：分别由**完全独立于 workspace** 的 JavaScript/Python/Go 假消费者读取 Analysis/Trace；修改内部 IR 字段名不破坏公共 v1；`check --emit` 不启动 Foreign、不加载 native plugin、不执行导入。

### R5：项目环境与隔离完善（工程/安全稳定门槛）

- **主要落点**：`crates/marqdo-package/`、`crates/marqdo-sandbox/`、`crates/marqdo-host/`、`crates/marqdo-foreign/`。
- `sync` 真实将锁定的 path/Git 包内容同步到项目环境或只读共享缓存，并且来源、锁文件、加载器使用**同一份包实例身份**；支持离线已有缓存（可作为 flag，不新增顶层命令）。安装不执行依赖脚本。
- 强制预算；Foreign 运行需明确授权和适配器限制，不能把默认子进程隔离称为沙箱；对外声明 strict 的平台必须有 OS 级实际隔离负例与故障注入测试，不支持平台必须拒绝。
- **验收**：依赖哈希篡改失败、包源变更失败、权限越界失败、并发安装/中断原子性、Foreign 子进程超时与进程树回收；受信 Native 的风险文档清晰。

### R6：发布、跨仓消费与停止线

- CI 运行 Linux 主矩阵，目标用户平台如 Windows/macOS 必须补齐构建、Tool 分发、环境路径与 ABI 基本烟测；受强隔离限制的平台明确在能力矩阵里标记 unavailable。
- 提供 `examples/tool-unknown-name/`、`examples/plugin-multi-export/`、`examples/kernel-mock/`、`examples/packages-multi-version/` 四个**不依赖官方工具仓库**的集成样例。
- 发布协议的 JSON Schema、正反向金样、版本协商/冲突清单、负向攻击测试。精确更新 `docs/STATUS.md`。
- **停止线**：达到“任何外部工具/多导出插件/Foreign Kernel/受支持包来源可依协议加入，并且内部结构变更不会要求三个独立工具同步改源码”的能力即可；**不**在 Core 增加 Knowledge/Proof/View 业务实现。

### 两道发布门槛

- **External Tool Alpha**：R0 + R1 的符号身份基线 + R3 + 最小 R4 Analysis/Trace 黑盒验证即可开启 Knowledge、Proof、View 的**原型**；R2 中尚未完成的扩展传输须公开标注 unsupported，不得称全部运行时扩展稳定。
- **Extension-Safe Core**：R0–R6 全部验收（至少目标支持平台上的强隔离真实实现，其他平台显式拒绝；Worker/Native/Kernel 权限路径通过安全负例），之后才对第三方工具/原生插件做正式稳定承诺。**沙箱未完成不得宣称安全执行不可信原生代码。**

## 14. 测试矩阵：必须把危险边界测出来

**v3 扩展性测试为发布前强制执行项**，与现有 AST/语义金样同等重要：

| 测试组 | 必须覆盖的正反例 | 失败时意味着什么 |
|---|---|---|
| EXT-TOOL | 未知名称 Tool 安装分发、保留 argv/stdin/exit、未注册 PATH 劫持、摘要篡改、命令冲突 | 扩展 CLI 仍依赖核心预知工具名 |
| EXT-CALL | 外部插件导出签名静态检查、同名冲突、显式别名、多导出多前缀、动态值调用的歧义拒绝 | 前端/运行时有两套不一致的符号查找 |
| EXT-PACKAGE | 相同包多版本、项目跨目录、全局缓存移动、Git pin、离线同步、锁漂移 | `ModuleId` 依赖物理路径，知识/证明制品不可复用 |
| EXT-KERNEL | 注册从未内置的语言标签、缺 provider、超时、非授权进程、wire 版本冲突 | Foreign 不可真正扩展或会绕开 Host |
| EXT-ABI | 多 namespace、重复 export、跨语言 C/Rust 示例、ABI 主版本拒绝、Worker crash、capability broker、伪造句柄与重放拒绝 | C ABI 宣称可扩展但只支持固定单一路径 |
| EXT-SEMANTIC | 修改普通叙述后的执行 IR 稳定；外部组件不能改变 `Strong/List` 等 AST 意义 | 将“扩展”错误扩大到语言执行规则 |
| EXT-CONSUMER | Core 公开 JSON 在 Rust 私有 IR 改动之后仍与独立客户端兼容；Reference 非执行 | 外部仓库将被迫跟随 Core 频繁升级 |


| 测试域 | 正例 | 必须负例/反例 | 测试位置 |
|---|---|---|---|
| Markdown AST | Strong/Emph/list/quote/heading、CJK、多行、GFM table、Math | 强调嵌套、缺失条件、task-list 错位、parser 版本漂移 | `tests/conformance/ast` + profile tests |
| 语言求值 | 阶乘、递归、嵌套分支、函数调用、命名实参、返回 | 未初始化、重复参数、非法返回、fuel/stack 超限 | `tests/conformance/semantics` + negative |
| 叙述不变性 | 只修改普通正文，IR/值不变 | 文本中插 `if/return` 不改变结果 | `crates/cli/tests/properties.rs` |
| 函数接口 | 推断形参、显式签名、跨模块别名 | 名称冲突/模糊/缺少参数/错误签名 | frontend + CLI tests |
| 引用 | 本地/跨文件静态链接、link call（当前支持范围） | 断链、歧义、路径越界、链接不可执行 | `check --refs` + analysis fixtures |
| Analysis v1 | 消费者只用 JSON 寻址函数、资源、调用 | 字段删改、无序结果、源摘要过期、版本错误 | 新 `tests/protocol/analysis` |
| Trace v1 | 调用/分支/返回/错误可正确关联 | 未授权输出泄露密钥、异常无结束标记、错版本 | 新 `tests/protocol/trace` |
| Tool v1 | **任意未预先定义的工具名**自动安装/发现/分发、退出码、卸载；变更工具不需编译 CLI | SHA 篡改、PATH 劫持、可执行路径越界、内置命令冲突、重复名称和不兼容协议 | 新 `tests/integration/tools` |
| Package | path/Git 精确锁定，离线缓存 | 无锁/变更依赖/循环/冲突/依赖逃逸 | package + CLI tests |
| Sandbox | pure/project 授权正确，strict 可用时隔离 | 未授权文件/外语/插件、TOCTOU、资源耗尽 | sandbox + host + OS integration |
| ABI | C/Rust SDK 插件，独立构建，类型化值 | ABI 版本错、错误内存所有权、崩溃、capability 绕过 | `marqdo-plugin-abi/tests` |
| Math/Foreign | 公式精确值，Python/Node one-shot，类型化 wire | 不支持的计算明确报错、超时/进程树、无权限拒绝 | math/foreign tests |
| 跨平台/发布 | Linux/Windows/macOS 可构建，`marqdo --help` 一致 | 平台功能虚假宣称、不兼容文件静默接受 | GitHub Actions |

**每种协议**须有 `valid.json`、`invalid-version.json`、`missing-required-field.json`、`unknown-optional-field.json`、`determinism` 和 `roundtrip` 用例。真实消费者 fixture **不得**依赖 private crate 或现有 repo checkout 的私有实现；测试命令应优先使用仓库内 Rust tests 和二进制黑箱测试，必要时用 Node/Python 仅作为消费者对照。

### 推荐最小质量门禁

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test -p marqdo-cli

# 已有 CLI 冒烟
cargo build -p marqdo-cli
./target/debug/marqdo check examples/factorial.mq.md --refs
./target/debug/marqdo call examples/factorial.mq.md 阶乘 --arg n=5
./target/debug/marqdo check examples/references.mq.md --refs

# 本轮新增黑盒合同（名称与路径以实现的 tests/ 为准）
# cargo test -p marqdo-cli --test protocol_analysis
# cargo test -p marqdo-cli --test tool_dispatch
# cargo test -p marqdo-cli --test trace_contract
```

上述后三项是计划中的测试名称示意，**不是已有测试**。测试无法运行时不得写“通过”；应记录具体 runner、工具链版本、commit SHA、跳过原因和验证日志。

## 15. 参考项目：只借鉴有效机制，不照搬体量

| 项目 | 参考什么 | 不照搬什么 |
|---|---|---|
| [CommonMark 0.31.2](https://spec.commonmark.org/0.31.2/) / [GFM](https://github.github.io/gfm/) | 标准语法、AST 同义性、列表/强调解析边界 | 把任意 Markdown 都作为合法 Marqdo 程序 |
| [Comrak](https://github.com/kivikakk/comrak) | Rust 解析器配置、源码位置、GFM/Math AST | 私自修改解析器语义以“方便控制流” |
| [Cargo 外部子命令](https://doc.rust-lang.org/cargo/reference/external-tools.html) | 统一 `cargo foo` 命令与独立工具的用户体验 | 原样 PATH 自动发现及权限假设 |
| [kubectl plugins](https://kubernetes.io/docs/tasks/extend-kubectl/kubectl-plugins/) | 外部子命令分发与参数转发 | 未注册任意程序默认执行 |
| [uv](https://docs.astral.sh/uv/) / [Cargo packages](https://doc.rust-lang.org/cargo/guide/dependencies.html) | project lock、缓存、环境同步体验 | 把 Python 作为 Marqdo 虚拟机或一开始自建公共仓库 |
| [Wasmtime/WASI](https://wasmtime.dev/) | 能力化扩展和未来可选 worker/Wasm 运行时 | 认为 Wasm 能自动隔离任意原生插件和子进程 |
| [WebAssembly Component Model](https://component-model.bytecodealliance.org/) | 将来 ABI 版本演进的跨语言设计经验 | 在 ABI v1 尚不稳定时并行推行第二 ABI |
| [Language Server Protocol](https://microsoft.github.io/language-server-protocol/) / [Debug Adapter Protocol](https://microsoft.github.io/debug-adapter-protocol/) | 以后 View/IDE 消费语义/调试的标准接口启发 | 在本阶段实现完整 LSP/DAP 服务和调试前端 |
| [Lean 4](https://lean-lang.org/) | 后续 Proof 的可信内核与证书重放 | 将 Lean 加入 CLI 核心运行时 |
| [OKF v0.3](https://github.com/open-knowledge-format/open-knowledge-format/blob/main/SPEC.md) | 后续 Knowledge 的 Markdown/YAML 知识包交换 | 将 OKF 词汇写成语言 AST 语义 |

## 16. 对开发者的具体 PR 约束

任何提交首先回答四个问题：改动服务哪个真实使用场景？为什么必须放在 CLI 核心而不是外部工具？公开契约是否变化？新增哪些正反测试？

- 改语言语义：更新 `spec/`、ADR、AST/语义 goldens 与负例。不得让一个单独样例绕过规范。
- 改公开协议：更新版本化 schema、兼容性矩阵和独立消费者测试。不得只更新内部 Rust serde derive。
- 改沙箱/权限：至少加一条拒绝用例，说明哪些 OS 平台真正隔离。拒绝错误必须区分 capability denied、strict unavailable 和 runtime error。
- 改 ABI：检查 C 头、Rust SDK、动态库加载器、wire 类型和独立 C/Rust 示例；破坏性变更先升 ABI major。
- 改 CLI：必须遵守六条主路径政策，不能给每个模块新开一个顶层命令。
- 改外部工具分发：必须做路径安全、安装原子性、PATH 劫持及参数转发测试。
- 测试、文档与代码同一 PR 更新；不允许 README 和 STATUS 虚报已实现；benchmark/安全实测不能用“CI 没有失败”替代。

推荐 PR 顺序以修订后的 **R0 基线 → R1 模块身份/Callable → R2 ABI/Kernel/Broker → R3 Tool → R4 Analysis/Trace → R5 环境/隔离 → R6 发布** 为准。为缩短独立工具原型启动时间，R3 和部分 R4 可在 R2 未完全完成时并行实施，但必须先冻结 R1 的逻辑身份与符号解析契约。

## 17. 最小开发者交接清单

维护者可以用以下清单判断是否允许另一名未参与设计讨论的开发者独立推进项目：

- [ ] 能解释为什么 Marqdo 的 Strong/Emph/List/Quote 具有固定控制流，而 Text 不执行。
- [ ] 知道唯一 Markdown parser 在哪里；不会编写第二套行正则解析器。
- [ ] 能在 `spec/mapping.md` 定位函数、分支、循环和返回的约束。
- [ ] 能在 `crates/marqdo-runtime` 找到原生求值，在 `marqdo-host` 找到所有副作用检查。
- [ ] 能指出 `Tool` 与 C ABI 插件的区别，理解后者不提供安全隔离。
- [ ] 知道 `check --emit` 对外协议，与 `inspect --ir` 的私有输出不同。
- [ ] 能通过虚拟的 Knowledge/Proof/View 独立进程消费者完成外部接口测试。
- [ ] 能在不安装三个外部仓库、不依赖 Python 的条件下运行阶乘和跨模块调用。
- [ ] 知道 `strict` 不可用时必须失败，不会把 `project` 模式称为 OS 沙箱。
- [ ] 知道包导入必须显式，知识引用永远不会隐式执行。
- [ ] 能运行所有文中已列出的实际质量门禁，并在 `docs/STATUS.md` 记录 commit 与结果。

## 18. 本文的最终决议与停止线

**M0.6 解决四项核心难题；对应 E01–E16 的修正和测试是其具体内容**：

1. **清除扩展阻塞点**：包感知模块身份、统一 Callable 描述符、ABI 多导出、Host Capability Broker、外部 Kernel Provider 和安全资源句柄，无需为新增第三方能力更改解释器核心语法。
2. **语言底座可信**：保持现有独立 Rust Markdown 语义/求值器，修复真实正确性与安全缺口，而不是继续增加语法及命令。
3. **外部消费者可独立开发**：Analysis v1、Trace v1（先最小）和 `marqdo tool` 三类协议让 Knowledge/Proof/View 各自在独立仓库实现，CLI 只进行协议与分发。
4. **工程可复现且诚实宣称能力**：包依赖、权限、ABI、测试、跨平台、发布状态都有明确的可靠性边界。

达到 **External Tool Alpha** 后，应主动开始外部工具仓库开发，而不是继续追求 CLI 的无穷扩张。达到 **Extension-Safe Core** 后，新的功能默认首先在外部工具或标准库中实现；只有确属语言语义、统一 Host 安全或通用工具协议之必要部分，才允许通过 ADR 纳入 CLI。

> **一句话原则**：Marqdo CLI 必须是足够小、足够确定的语言内核与工具分发器；可执行知识、证明与优美阅读可以无限进化，但不能迫使语言核心无限膨胀。


## 附录 A：扩展性审计的直接代码证据（`master@0116160`）

以下文件名均为相同提交上的现存路径，开发者应在新提交中重新核实：

- `crates/cli/src/main.rs`：`Command` 封闭枚举，没有通用 `tool` 发现/路由。
- `crates/marqdo-ir/src/builtins.rs`：核心 `BUILTINS` 静态集合；并非所有扩展都应加到这里。
- `crates/marqdo-frontend/src/calls.rs`：通过 `builtins::find` 与本地/模块函数进行调用校验，没有统一的插件清单签名解析。
- `crates/marqdo-runtime/src/builtins_impl.rs`：内置函数 `match builtin.name`，新核心内置项需要扩展这里。
- `crates/marqdo-host/src/host.rs`：`HostOp` 固定分发；`load_plugin` 采用 `exports.first()` 推导唯一 namespace；`op_plugin_invoke` 按 namespace 查找第一个 slot；loader 无 capability_handler。
- `crates/marqdo-plugin-abi/src/manifest.rs`：`KNOWN_CAPABILITIES` 固定名单；`PluginKind` 定义 Native/Worker/Wasm，而目前仅 Native 可实际加载。
- `crates/marqdo-foreign/src/lib.rs`：`SUPPORTED_LANGUAGES` 仅 Python/JavaScript，`normalize_language` 和 `program_candidates` 为匹配分支。
- `crates/marqdo-ir/src/lib.rs` + `crates/cli/src/loader.rs`：项目相对路径 `ModuleId` 与项目根物理路径检查；`@dependency` 仅从 `dependency.path` 得到候选。
- `crates/marqdo-package/src/lib.rs`：Dependency 仅路径与 Git，锁格式和来源求解尚不完整。
- `crates/marqdo-host/src/capability.rs`：核心能力字段固定，权限请求必须独立于用户授权。
- `crates/marqdo-value/src/wire.rs` + `spec/value-wire.md`：`plugin_handle` 目前结构化可反序列化，不能把此类值当作不可伪造授权句柄。
- `crates/marqdo-frontend/src/map.rs`：跨文件 `LinkCall` 当前未实现，不能让第三方工具私自推断目标。
- `docs/STATUS.md`：已有功能与开放缺口列表，但部分描述落后于最近修复与 `.github/workflows/ci.yml`。

**审计结果以源码事实和明确负例为依据；“扩展方案”属于新设计要求，不冒称已经实现。即使完成 E01–E16 的检查，也不能保证不存在其他未来问题；以后每出现新扩展类型均需跑通“无需修改核心”的独立消费者测试。**

---

## 附录 B：修订包来源说明（原 revision package README）

以下保留整合前 `docs/marqdo-cli-extensibility-audit-v3/README.md` 的原文，作为来源与审查基线记录。该修订包现已合并入当前文档体系：主规范位于 `docs/NEXT_PHASE_IMPLEMENTATION.md`，审计清单位于 `docs/EXTENSIBILITY_AUDIT.md`，Tool Protocol v1 位于 `spec/tool-v1.md`；原目录已删除。原文中的路径与“放入仓库替换”表述保留为历史记录。

> # Marqdo CLI M0.6 可扩展性审计修订包
>
> 将 `docs/NEXT_PHASE_IMPLEMENTATION.md` 放入仓库替换之前版本。
>
> - `docs/NEXT_PHASE_IMPLEMENTATION.md`：完整 v3 实施规范（含现状审查、全部修复方案、阶段规划与测试）
> - `docs/EXTENSIBILITY_AUDIT.md`：审计问题清单及代码落点，便于创建任务
> - `spec/tool-v1.md`：通用 Tool Protocol v1（作为候选规范，不等于已经实现）
>
> 源码审查基线：`cflmy/marqdo-cli` `master@0116160399fda6df69a72da438e04b0072566b0f`。未修改 GitHub 仓库，未独立执行 Cargo 测试。
