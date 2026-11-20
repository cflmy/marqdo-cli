# M0.6 扩展性审计：源码问题、改造路径与测试契约

> 源文档 `docs/NEXT_PHASE_IMPLEMENTATION.md` §1A 的审计摘录，供开发者按问题创建 PR；具体开发次序和发布门槛请看主文档。该审计之后的稳定化基线与 G-A/G-B/G-C 门槛见 `docs/CORE_STABILIZATION_AND_ECOSYSTEM_GATE.md`。

## 1A. **扩展性审计（v3 增补，优先级高于后续阶段）**

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

