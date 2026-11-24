# RFC-OBS-0001: Runtime observability and real-time execution events

> **Status:** Historical design input. [ADR-0068](../adr/0068-single-current-public-contracts.md)
> replaced Execution Events v1 and Trace v2 with the single current
> [Observation contract](../../spec/observation.md); the retired spec named
> below exists only in Git history and in tagged archives.
>
> **Source:** issue `#1` of `cflmy/marqdo-cli`, filed 2026-11-22 against
> `master@eeea01d672c07b6be7d6444031d1619309dc88ab`. It is reproduced here so
> the design is reviewable without the issue tracker. Where this document and
> `spec/execution-events-v1.md` disagree on the wire format, the spec is
> normative.

---

# RFC-OBS-0001：Marqdo 3.x 运行时可观测性与实时执行事件协议

> **建议 Issue 标题**：`[RFC][P0/P1] Runtime Observability & Execution Events v1：实时输出、调用帧、可选变量观测和外部工具解耦`
>
> **状态**：历史设计输入；实现范围与证据以 [`docs/STATUS.md`](../STATUS.md)
> 和 [`spec/execution-events-v1.md`](../../spec/execution-events-v1.md) 为准。
> 本文中的 `PROPOSED` 命名保留为设计决策记录，不再表示“未实现”。
>
> **目标仓库**：[`cflmy/marqdo-cli`](https://github.com/cflmy/marqdo-cli)；旧产品 [`cflmy/marqdo`](https://github.com/cflmy/marqdo) 只用作历史参照，不能作为 3.x 的语义或 API 依据。
>
> **源码审查基线**：`master@eeea01d672c07b6be7d6444031d1619309dc88ab`（2026-11-22）。已发布基线：`v3.0.0-beta.1@2c7c4f31e7a388d3fcfa717a77067e696ed13854`。
>
> **读者**：CLI、Runtime、Host、View、Agent、Knowledge、Prove、IDE、第三方扩展库开发者。**本文自足；无需阅读任何历史对话**。文中明确标出的 `spec/` 和 `docs/` 是实际开发时必须共同更新的仓库文件，而非理解本文的前置条件。
>
> **核心目标**：使外部仓库仅使用**发布版 `marqdo` 二进制 + 公开版本化协议**即可实时获取计算输出和经授权的运行观测信息，不重编译 CLI、不链接其私有 crate、不实现第二套解释器。

---

## 0. 决策摘要（先读此节即可确定工作范围）

本 RFC 作出以下架构决策：

1. **需要修改 CLI 的通用观测基础设施**，不是为 `marqdo-view` 加专用 API。`run/call` 可选输出 **Execution Events v1**；`check`、`inspect` 等静态命令仍绝不执行。
2. **CLI → 外部工具用单向 NDJSON/JSON Lines 流**；`marqdo-view` 后端接收 JSONL 并转换为浏览器 **SSE**。CLI 不实现 HTTP、SSE、WebSocket、浏览器会话或 Token/Agent 专用协议。
3. **Trace v2 保持兼容**：不改变 `marqdo.trace` `schema_version:2` 的既有字段语义，不在默认 Trace 中添加参数、变量值、内存字节。实时协议具有独立 Schema、独立版本与独立预算。
4. **优先实现实时 stdout/stderr 和任务生命周期**；紧接着实现可靠的调用实例标识、分支/循环事件；之后才实现**显式授权、精确选择且有界**的变量/参数值观测。
5. **内存统计与写入元数据可以增量实现**，但本 RFC 不引入原始内存读取、任意地址快照、断点、单步、时间旅行和 Jupyter 式持久内核。它们属于以后独立的 Debug Protocol RFC。
6. **观测默认关闭且不改变语言含义、执行结果、权限判定与预算**；除了有定义的输出背压/观测开销外，不允许将“启用观测”变成改变分支或修复运行错误的行为。
7. 外部开发者的主验证方式是**冻结 CLI 二进制后的黑盒消费者测试**。View、Agent 和第三方消费者使用相同协议，不得出现 `if consumer == view` 一类特殊分支。

### 0.1 优先级定义

| 优先级 | 内容 | 对发布的影响 |
|---|---|---|
| **P0 / OBS-01～03** | 协议、观测边界、实时 stdout/stderr、终态、`run/call` JSONL 模式、黑盒验证 | 建议在下一个 3.x Beta 实施；对交互 View/Agent 的价值最高 |
| **P1 / OBS-04～05** | 调用实例及父子关系、变量写入元数据、**显式**参数/变量值观测、源位置关联 | 建议稳定 3.x 前完成并验证；可拆分提交 |
| **P2 / OBS-06** | 内存区域/预算统计及变更元数据、采样、性能优化 | 可以放到后续 3.x；禁止牵连 P0 |
| **非目标** | 通用调试器、内存转储、逐指令暂停、SSE server、UI、AI Token 专用协议 | 不得为了本 Issue 一并开发 |

`P0/P1/P2` 代表项目规划优先级，不等同于当前仓库已有的 CI 门禁编号。

---

## 1. 背景：为什么现在必须补齐观测边界

Marqdo 3.x 把 Markdown 源、语言映射、IR、独立解释执行、Capability Host 及第三方工具协议分开了。`.mq.md` 的 H2 `@exec` 函数可以由 `marqdo run/call` 执行；外部 `marqdo-view`、`marqdo-knowledge`、`marqdo-prove`、Agent、IDE 则应当通过公开协议消费静态/动态信息。

已公开的两类机器可读信息是：

- **Analysis v2**：`marqdo check index.mq.md --emit analysis.json`。它描述模块、函数、语句结构、引用、代码资源、UTF-8 源码区间及静态诊断；**不执行**。
- **Trace v2**：`marqdo run/call ... --trace-file trace.json`。它记录 `call.enter`、`call.exit`、`return`、`branch.eval`、`loop.iteration`、`effect` 及可能的截断事件；在运行结束时原子写入，**不包含调用实参、秘密、句柄或文件内容**。

CLI 还提供 `run/call --json`，但它在执行结束后一次性打印返回值、捕获的 stdout/stderr、错误等 JSON。以上能力能支持静态阅读及事后执行展示，却无法让外部工具在一个长时间运行的任务中即时看到输出，也不足以恢复多次/递归调用的完整调用实例与局部变量变化。

### 1.1 具体用户场景

- **View**：用户在文档旁点击“调用函数”，希望实时看到正常程序输出，并在完成后把调用、分支和返回结果关联至原文；不希望 View 内置第二个 Marqdo 解释器。
- **Agent/SKILL**：需要以低延迟消费长任务的阶段性输出，得知任务是否仍执行，并能中止已无必要的工作。
- **科研/计算工具**：需要计算进度与调用轨迹，之后依据源版本比对结果，不能等数分钟后才一次性获得日志。
- **IDE**：需要知道某次函数调用、某个赋值事件属于哪个运行帧；当前仅有函数标识、没有稳定的“调用实例”身份。
- **Knowledge/Prove**：可能用运行事实生成经验记录或反例，但必须区分“运行中观察到”和“通用证明成立”。**不为这两类工具把推理/证明逻辑塞进 CLI。**

### 1.2 仅把 Trace 文件边写边读为什么不够

现有 Trace v2 是**完整记录文件协议**，由 CLI 在运行结束时原子发布；其协议承诺了安全摘要和容量规则。把它改成一边执行一边追加 JSON，不但会破坏现有消费者对完整文件的假设，还不能自然支持实时 stdout/stderr、执行前错误、管道背压和“主动订阅但不保存文件”的工具。因此需要一个**并存而非取代**的事件传输协议。

### 1.3 不实现这些能力会造成什么长期成本

不增加公共观测协议，独立工具将被迫采取以下错误方案：解析面向人的终端日志、读取私有 Host 结构、在 View 中重写解释器、从 Trace 中猜测变量值、为了一个工具额外改一条 CLI 私有接口。它们都会使 3.0 的仓库解耦退化。因此本 Issue 的成功不是“View 看起来会动”，而是“任意消费者使用一个稳定的观测接口都能做同类工作”。

---

## 2. 已核实的现状与缺口（EXISTING，不是提案）

| 当前源码/规范 | 已有行为 | 缺口/注意事项 |
|---|---|---|
| `crates/marqdo-runtime/src/interpreter.rs` `call_function` / `bind_args` | 调用期间有带 `locals` 的 `Frame`；`call.enter` / `call.exit` 已记录 | 事件只有函数身份、尚无每次调用的 `frame_id`/`parent_frame_id`；值留在内部 |
| 同文件 `exec_stmts_inner` | 成功赋值写入 `frame.locals`；记录返回类型、分支真假、循环 continue/exit | 不发布每次赋值事件/值；不能用最终变量值倒填历史语句 |
| `crates/marqdo-runtime/src/memory.rs` `MemoryStore` | 持有区域和提交字节统计；`MemoryHandle` 非原始主机指针 | 不存在公开的通用运行时内存观测协议；不应导出原始字节 |
| `crates/marqdo-host/src/host.rs` `Host`、`StdHost` | Host 处理副作用；`output`、`stderr` 为缓冲字节数组；`resource_write` 对 stdio 写入缓冲 | 想获得**真实的**增量 stdout/stderr 必须在 Host 输出路径上挂钩，而不是事后拆分 |
| `crates/marqdo-host/src/trace.rs` | `SemanticEvent` 含 `seq/kind/function_id/module_id/span/detail`；Host 有 `TraceBudget` | 没有公共调用实例 ID；现有安全摘要协议有意不提供参数值 |
| `crates/cli/src/commands/run.rs` | `run/call --json` 完成后组合 stdout/stderr/返回值；`--trace-file` 结束时调用写入 | 不提供实时事件通道；`--json` 不可和流式协议混作同一 stdout |
| `crates/cli/src/trace.rs` | Trace v2 原子写完整文档，并与 Analysis digest 关联 | 修改已有 v2 的隐私/完整性含义是破坏性行为 |
| `spec/analysis-v2.md`、`spec/trace-v2.md` | 公共版本化静态/动态合同 | 新增协议必须独立命名/版本，说明如何与其对齐 |
| `spec/tool-v1.md`、`docs/developers/tools.md` | 独立工具通过 `MARQDO_BIN` 调用已发布 CLI，不依赖私有 Rust 类型 | 新协议需要按工具协议方式向外公开与声明兼容性 |

**证据基线**：上述结论仅依据已检查的 SHA；后续实现前必须重新确认 HEAD，不能假定未检查的后续提交仍维持同样行为。文中设计决策均标记为 **PROPOSED**。当前发布版**没有** `--event-stream`、`--observe-values` 或 `marqdo.execution-events`。

### 2.1 必须保留的既有保证

1. `check`/`inspect` 不执行；`run`/`call` 通过统一预备流程和能力 Host 执行。
2. `--json` 仍是**完整、非混杂**的单次执行结果；不能把 JSONL 事件插入其 stdout。
3. `--trace-file` 仍在完成后形成符合 Trace v2 的完整 JSON，含合法的 `source.analysis_digest`。
4. `pure/project/strict` 及插件、Foreign、Provider 的授权规则保持原样；观测开关不是额外 capability grant，也不是绕过安全策略的方法。
5. 默认 Trace 继续不包含参数值、原始内存、敏感数据。即使 Events v1 未来可显式输出选定值，也不意味着 Trace v2 变成高敏快照。
6. 语言 `.mq.md` 的 `@exec/@knowledge`、语句载体和控制流映射**不修改**。这是运行时观测项目，不是 RFC-LANG 重构。

---

## 3. 最终架构与职责边界（PROPOSED）

```text
                         marqdo-cli (唯一运行时)
 .mq.md ──> Frontend / PreparedProgram ──> Runtime Interpreter
                     │                            │
                     │                      运行中事件（借用数据）
                     │                            ▼
                     │                     Observation Facade
                     │                  ┌─────────┴──────────┐
                     │           Metadata/Frame         Value Policy
                     │                  └─────────┬──────────┘
                     │                            ▼
                     │                      Event Dispatcher
                     │                 ┌──────────┼───────────┐
                     ▼                 ▼          ▼           ▼
                Analysis v2       JSONL Sink   Trace v2   Null Sink
               （静态文件）      （实时stdout） （已有完整文件） （默认无开销）
                                       │
                     ┌─────────────────┼──────────────────┐
                     ▼                 ▼                  ▼
                 marqdo-view       Agent / SKILL       其他 CLI Tool
                     │
            转换为 HTTP SSE
                     │
                   浏览器
```

### 3.1 模块划分：哪里能增加代码、哪里不该增加

**Runtime (`crates/marqdo-runtime`)**
- 仍是计算、函数调用、局部变量赋值、内存读写的**唯一语义来源**。
- 在已经成功完成的状态变化边界提供可选观测钩子；不能仅为输出“赋值事件”再次执行表达式。
- 函数调用实例 ID 在进入帧时生成；递归/异常退出也必须配对和归属正确。
- 不认识 View、HTTP、SSE、JSONL、HTML，也不负责隐藏/展示参数输入框。

**Host (`crates/marqdo-host`)**
- 继续作为所有被支持的 stdout/stderr 和副作用的汇聚位置。
- 定义**内部 typed observation interface**、可选 `EventSink` 和受预算约束的分发；Host 接收 Runtime 提交的纯语义事件，再根据策略编码成对外数据。
- 不为了观测对插件/Foreign/Provider 放宽权限，也不自动把外部子进程的 stdout 当成 Marqdo 程序的受控 stdout。
- 编码/写出时禁止在全局锁下等待一个不可信外部消费者；先解决锁与背压的线程安全边界。

**CLI (`crates/cli`)**
- 只负责选项解析、协议版本协商、观测配置、JSONL writer、稳定错误映射、退出状态和原有 Trace 文件写入。
- `--event-stream jsonl` 使用**专有机器输出通道（CLI stdout）**；程序 stdout/stderr 是事件，不能再原样写到同一个 stdout。
- CLI 没有 HTTP 监听、浏览器 session、SSE 编码、View 专用功能分支。

**独立 `marqdo-view`**
- 使用 Tool Protocol v1 的 `MARQDO_BIN` 启动 CLI 子进程，消费 JSONL。
- View Server 负责 `POST` 发起、通过 SSE 转发并支持取消；不改变事件语义，不补造内部变量值，不读取 CLI 私有数据。
- View 仍默认只读；只有用户显式确认才执行。View 的 SSE 和 CLI 的 JSONL **属于不同传输层**。

### 3.2 是否需要新 Rust crate？

**首选不新增**只为观测而存在的多层框架或十几个 crate。将类型定义放在 `marqdo-host` 的有界模块（例如 `observe/{mod.rs,event.rs,policy.rs,sink.rs}`），在 `marqdo-runtime` 以轻量 Host 方法或明确 trait 传入只读事件，在 CLI 内维护 `execution_events.rs`（公共 JSONL 序列化）。

如果编译期依赖图迫使 Host↔Runtime 环依赖，则可建立**一个**只含 `Event`/`Policy`/`Span`、无 IO/CLI 的 `marqdo-observe` 内部基础 crate；必须附 ADR 说明为什么现有边界不足。**不得因为“未来也许要 IDE”先创建通用插件总线、会话管理器或所有业务通用 EventBus。**

### 3.3 三种不同对象，不能混淆

- `RuntimeEvent`：内部 typed 事件，允许只读借用 `Value`、`Span` 等，以便延迟序列化。不是公共协议。
- `ExecutionEventV1`：经过过滤/预算/转码后写到 stdout 的**公开**单行 JSON 对象。是第三方消费者所依赖的协议。
- `TraceV2`：事后落盘、独立的公共证据对象。运行时可复用部分事件来源，但**不承诺每一种 RuntimeEvent 都写进 Trace v2**。

---

## 4. 外部协议：Execution Events v1（PROPOSED）

### 4.1 命名和传输

建议协议 ID：`schema = "marqdo.execution-events"`；主版本：`schema_version = 1`；媒体类型文档可记为 `application/x-ndjson`。每行是一个完整 UTF-8 JSON 对象，以 `\n` 终止；一条事件内不得出现未转义换行；事件超过大小限额必须拒绝/裁剪并明确标记。

提议 CLI 使用：

```bash
# 下列命令是提议 API，当前 3.0.0-beta.1 不支持
marqdo run index.mq.md --event-stream jsonl
marqdo call index.mq.md main --arg n=5 --event-stream jsonl
marqdo call index.mq.md main --arg n=5 \
  --event-stream jsonl --trace-file trace.json
```

选项决策：
- `--event-stream jsonl` **仅** `run/call` 可用，表示整个 stdout 都属于 JSONL；普通程序输出必须封装成事件。
- `--event-stream` 与已有 `--json` **互斥**，不混合两种 JSON 输出；`--trace-file` 可共存（保留 Trace v2 原子语义）。
- 普通人类可读 `run/call` 不启用事件、行为和 stdout 字节保持不变。
- `check`、`inspect`、`fmt`、`tool` 等命令不接收该选项；不将静态检查伪装成运行。
- CLI 选项错误（clap 在命令分发前拒绝）仍走普通 stderr 与非零退出；一旦进入事件模式且成功初始化 writer，预备/授权错误须使用结构化 `run.rejected` 终态，**不输出半行 JSON 或人类可读文字到 stdout**。

### 4.2 公共事件信封（提议）

所有正常输出的事件必须包含：

```json
{
  "schema": "marqdo.execution-events",
  "schema_version": 1,
  "run_id": "run-opaque-01",
  "seq": 2,
  "kind": "call.enter",
  "payload": {}
}
```

- `run_id`：CLI 生成的**本次运行**不透明字符串；不是 OS PID，不得被误用作跨机器持久身份。
- `seq`：从 0 开始的全局、严格递增整数；一个运行内不重复、不倒退；由统一分发者赋值。语义事件与 stdout/stderr 的**可观察**顺序按进入该分发者的顺序定义，不能保证其他操作系统子进程内独立流的真实物理先后顺序。
- `kind`：稳定、带命名空间的事件名；未来新事件种类是**加法扩展**。消费者可以忽略未知 `kind`，但必须拒绝未知 `schema_version` major。
- `payload`：与 kind 对应的有界对象。每类的 required 字段由 JSON Schema 约束；未知可选字段可以忽略。
- 为避免无关冗余，不强制每条事件重复完整 `project` 元信息；建立时通过 `stream.opened` 提供协议、生产者和观测配置；**成功准备项目后**，由 `run.started` 提供 `source` 身份和摘要；后续事件可带 `module_id`、`span`。预备失败时 `run.rejected` 可没有 `source`。

### 4.3 最小公共事件表

| 事件 | 何时发出 | 必需信息 | 观测级别 |
|---|---|---|---|
| `stream.opened` | JSONL writer 成功初始化，**第 0 条** | `producer`、`protocol`、预算、观测范围（无须已有 `source`） | base |
| `run.started` | 校验、授权通过，解释器准备真正执行 | `entry`、`mode`、`source`（`root_id` / `analysis_digest`）、`started`（仅诊断用途） | base |
| `stdout.chunk` | Host 真正接受一段程序 stdout | `encoding`、`data_base64`、`byte_count` | base |
| `stderr.chunk` | Host 真正接受一段程序 stderr | 同上 | base |
| `call.enter` | 参数已成功绑定、创建本次函数帧 | `frame_id`、`parent_frame_id`、`function_id`、`module_id`、`span` | structural |
| `call.exit` | 某次已进入的函数帧结束（含失败） | `frame_id`、`status`、`module_id`、`span` | structural |
| `return` | return 语句实际执行完 | `frame_id`、`value_type`、`module_id`、`span` | structural |
| `branch.eval` | 实际求值某条条件 | `frame_id`、`result`、`module_id`、`span` | structural |
| `loop.iteration` | 条件实际求值完成 | `frame_id`、`decision`、`module_id`、`span` | structural |
| `effect` | Host 已完成受控副作用尝试 | `operation`、`result_code`、`module_id`、`span` | structural |
| `variable.assigned` | 成功提交局部变量赋值 | `frame_id`、`name`、`value_type`、`module_id`、`span` | selected values / metadata |
| `observation.truncated` | 仅观察事件达预算 | `reason`、`limit`、`written`、`dropped`（若已知） | any |
| `run.completed` | 正常成功退出 | `status:ok`、`value`（公开 wire 值，遵循既有返回规则） | base；终态 |
| `run.failed` | 已开始执行但运行失败 | `status:error`、稳定 `error.code`、安全消息 | base；终态 |
| `run.rejected` | 进入 event mode 后、运行前校验/授权拒绝 | 结构化错误及阶段 | base；终态 |

**状态一致性**：进入 event mode 且 writer 存活时，协议应有恰好一条 `stream.opened`、最多一条 `run.started`，以及恰好一条终态 `run.completed/failed/rejected`。对于 OS 强制终止、进程崩溃、SIGKILL、断管，此保证不成立；消费者必须按“未收到终态 + 进程非正常退出/EOF”标记为 **incomplete / unknown**，不能补造成功事件。

### 4.4 输出数据的字节安全

stdout/stderr 可能包含无效 UTF-8、跨写操作分裂的多字节字符、NUL 和二进制数据。**规范载荷统一使用 `data_base64`，按字节保真；不把 `String::from_utf8_lossy` 当作协议原始数据。** UI 需要人类文本时自行使用增量 UTF-8 解码，并定义无效字节替换或十六进制展示策略。

```json
{
  "schema": "marqdo.execution-events",
  "schema_version": 1,
  "run_id": "r1",
  "seq": 2,
  "kind": "stdout.chunk",
  "payload": {"encoding":"base64","data_base64":"5byA5aeL5paH5pysCg==","byte_count":13}
}
```

上面的 `data_base64` 对应 UTF-8 字符串 `开始文本\n`（13 字节）；实现时必须逐字节回读验证，`byte_count` 必须与解码后长度一致。`stderr.chunk` 同理。

`stdout.chunk`/`stderr.chunk` 是 **Marqdo Host 接受的程序流**；第三方插件或外语内核若绕过 Host 直接写 OS fd，其输出可能逃逸或与事件流混杂，属于安全/完整性风险，必须在 OBS-03 中测试并禁止。对无法截获的外部 stdout/stderr，**要么隔离/重定向并单独标注来源，要么禁止该组合使用 event mode**，不能默默将其解释为可信程序 stdout。

### 4.5 完整的最小示例

```jsonl
{"schema":"marqdo.execution-events","schema_version":1,"run_id":"r1","seq":0,"kind":"stream.opened","payload":{"producer":{"name":"marqdo","version":"3.x"},"observation":{"level":"structural","values":false}}}
{"schema":"marqdo.execution-events","schema_version":1,"run_id":"r1","seq":1,"kind":"run.started","payload":{"entry":"root:demo@0.1.0::index.mq.md::main","mode":"pure","source":{"root_id":"root:demo@0.1.0","analysis_digest":"sha256:<digest>"}}}
{"schema":"marqdo.execution-events","schema_version":1,"run_id":"r1","seq":2,"kind":"call.enter","payload":{"frame_id":"f1","parent_frame_id":null,"function_id":"root:demo@0.1.0::index.mq.md::main","module_id":"root:demo@0.1.0::index.mq.md","span":null}}
{"schema":"marqdo.execution-events","schema_version":1,"run_id":"r1","seq":3,"kind":"stdout.chunk","payload":{"encoding":"base64","data_base64":"T0sK","byte_count":3}}
{"schema":"marqdo.execution-events","schema_version":1,"run_id":"r1","seq":4,"kind":"call.exit","payload":{"frame_id":"f1","status":"return","module_id":"root:demo@0.1.0::index.mq.md","span":null}}
{"schema":"marqdo.execution-events","schema_version":1,"run_id":"r1","seq":5,"kind":"run.completed","payload":{"status":"ok","value":{"type":"int","value":"10"}}}
```

示例中的源版本、`source` 内容、`value` wire 格式应以最终协议 Schema 与实际 `marqdo_value::to_wire` 的一致性测试为准；占位的 `<digest>` 不是有效 SHA-256，**不能直接作为 JSON Schema 测试 fixture**。

### 4.6 与 Analysis v2 和 Trace v2 的一致性

`run.started.payload.source` 至少包含：`root_id`、`analysis_digest`；若需要还可包含锁文件 digest、入口 module id。`stream.opened` 在预备之前发出，因此**不得要求它携带尚未计算的 source**；预备失败时的 `run.rejected` 同样不要求携带 source。`analysis_digest` **必须等于与当前执行相同 prepare/projection 配置下 Analysis v2 文件原始字节**的 SHA-256，而不是随手重新序列化 JSON 后的摘要。源变化后必须标记旧事件为 stale。

- 建议在现有 Trace v2 的 `run.id` 字符串字段中重用本次 CLI 生成的 `run_id`；**不新增字段、不改变其类型**。如果现有内部生成路径不同，应在 CLI composition root 统一创建 run ID，再分别交给两种 writer。
- Trace v2 仍在执行完成后原子生成；JSONL 可以在运行中消费。两者只共享运行身份、来源摘要与可信事件来源，**不要求事件逐条同构**。
- 需要识别调用实例的是 Events v1；不能宣称 Trace v2 已经具有 frame_id。
- 消费者拒绝未知 Schema major、来源不匹配、非法 span 和无终态事件，不能误把旧事件渲染到新源码上。

### 4.7 版本与兼容规则

采用独立协议主版本 **v1**，不与 CLI 产品版本或 Trace v2 混淆。增加可选事件、可选字段可在 v1 允许；改变必填字段含义、事件排序语义、脱敏承诺、源摘要定义或将私密数据改为默认公开，必须发新 major。`--help` 和 `docs/developers/compatibility.md` 应列出该协议、支持范围和 feature flags；Tool Protocol v1 的 `tool.json` 目前只协商 Analysis/Trace，**不能悄悄加入一个未经 Tool v1 规范确认的 `events` 字段并当作自动协商**。初期工具通过调用 `MARQDO_BIN` 的已定义 `--event-stream jsonl` 并核对 `stream.opened` 协商协议；若将来扩展 `tool.json.compatibility`，需单独更新 Tool Protocol 的 Schema、实现和测试。

---

## 5. 内部事件机制：既要实时，又不能重建 EventBus

### 5.1 最小类型和调用方向

建议实现可替换的内部接口（伪 Rust，具体命名可在 PR 中调整，但语义不可改变）：

```rust
// 内部接口；不是公共 SDK。借用 Value 以避免无谓 clone/序列化。
trait ObservationSink {
    fn emit(&mut self, event: RuntimeEventRef<'_>) -> Result<(), ObservationError>;
}

struct ObservationConfig {
    enabled: bool,
    structural: bool,
    selected_variables: Vec<String>, // 精确名称选择，不自动全量输出
    selected_parameters: Vec<String>,
    allow_text_values: bool,
    max_observation_events: u64,
    max_observation_bytes: u64,
    max_value_bytes: usize,
}

// RuntimeEventRef 包含 typed 事件、源身份及可选 &Value，
// 绝不将所有 Frame.locals 或整个 MemoryStore 交给 sink。
```

运行正常流程：

1. CLI 根据选项创建 `ObservationConfig`；无观测选项时使用 **Null/None sink**。
2. Runtime/Host 在已有状态提交点发出轻量 typed 通知；**先完成原始语义**，再形成不会影响业务结果的只读观测资料。
3. 分发层先执行级别、变量名精确选择、数据敏感度、事件/字节预算判断。
4. 通过的事件才做序列化并同步写入 CLI 所持有的**独立协议 stdout writer**。
5. `--trace-file` 仍走现有 Trace Writer；内部可以共享事件创建，但不能因为打开 JSONL 就自动在 Trace v2 输出变量值。

如果已有 `Host::record_semantic(...)` 不返回错误，不能据此把 IO 异常吞掉。开发者必须采取以下两者之一：① 新增明确可传播的 fallible emission 方法并在解释器安全点传播错误；② 在 sink 保存错误，解释器的下一次预算/控制边界与最终返回路径强制检查。**不得让 JSONL 管道已损坏而 CLI 继续无限执行有副作用的程序。**

### 5.2 输出产生点与正确的实时性

真正的实时流必须在 `StdHost::resource_write` 记录 stdout/stderr 的同一条授权/预算路径上送出 `stdout.chunk` / `stderr.chunk`。**严禁**把 `take_output()` 得到的最终字符串按行切碎，再声称“实时”。

行为要求：

- 写出的字节恰好等于 Host 已接受的字节；如果 Host 的输出预算拒绝一次写入，则不能先推送该未提交字节。
- 原有 `--max-output` 语义仍有效；stream 不能绕开 Budget。
- 行缓冲只能影响吞吐，不能把短小增量积累到运行结束；每次成功的 Host 程序输出写入应在合理有界延迟内可读取（见验收）。
- 输出事件不必等待 Trace/Analysis 文件，也不要求所有扩展原生支持流。Foreign/Plugin 调用若本身阻塞直到结果返回，不得宣称内核可以神奇地实现中间 Token 流。
- 允许有界 chunk 合并，但顺序和字节精确性必须保持；不同设备返回的字节不能冒充主程序 stdout。

### 5.3 预算、背压与失败策略

**需要分别管理三类资源**：

1. **原有执行预算**：`--fuel`、`--timeout`、`--max-output` 等。观测不得扩权或改变其含义。
2. **可丢弃的结构化观测预算**：例如建议 `100_000` 条、`16 MiB`，达到上限后只停止**非关键结构事件**，发一条 `observation.truncated`，继续运行和真实 stdout/stderr。值大小默认限制（例如 `256 B`），未观察的值不序列化；常规执行不分配大量事件缓冲。
3. **不可静默丢弃的关键事件**：`stream.opened`、`run.started`、stdout/stderr、最终终态。它们不受“结构事件条数”预算截断；stdout/stderr 仍受原有输出预算。若其传输不可继续，必须明确导致**观测传输失败**，不能伪造完整成功流。

背压策略：**首版优先使用有界、同步写入**；下游不读时可使执行等待，属于可解释的 IO 背压，而不是无界增加内存。避免新建全局异步队列和大量线程。若后续引入专用有界队列，必须分别定义关键输出阻塞与非关键结构事件截断的策略；不能静默丢失 stdout 字节。

断管/写失败：JSONL sink 写失败需使 `run/call` 返回非零；在可以恢复的条件下打印仅到 OS stderr 的 `observe.transport_failed` 诊断，不得向坏掉的 stdout 再写半条 JSON。**对于此前已经发生的副作用不能声称回滚**，消费者记录为“观察失败、运行状态可能不完整”；强制取消或 SIGKILL 同样不能凭空补完终态。

### 5.4 输出顺序和线程安全

- 同一运行使用一个分发序号源，EventSink 写入过程对外形成全序。
- 调用帧栈、局部变量数据仅在正确的解释器安全点采样；禁止后台线程无同步地遍历 `Frame.locals`。
- 插件/外语进程如果同时写流，必须使用经验证的 Host 接口或一个区分来源的受控转发器；不能让第三方 fd 原始输出污染 JSONL。
- 不在持有 MemoryStore 内部互斥锁/全局运行状态锁时等待阻塞 IO；先形成有界摘要再执行发送。

---

## 6. 调用帧身份与结构化执行事件（P1）

### 6.1 需要 `frame_id` 的理由

同一个函数可以多次被调用、递归调用，两个分支/循环的源位置也可能相同。现有 `function_id` 只能说明**哪个函数**，不能说明**哪一次调用**。没有调用实例身份，外部 View 即使知道某条赋值发生过，也无法可靠附着到具体调用。

### 6.2 Frame identity 规则

- `frame_id`：本次运行内唯一、不透明、不可复用。建议 `f1, f2, ...` 或整型字符串；只用于展示和事件关联，不暴露内存指针。
- `parent_frame_id`：顶层 `null`，其余指向进入当前函数时的直接调用者；必须存在或有明确表示“外部入口”的规则。
- `function_id`：复用现有全限定函数标识和 module identity。
- 在 `bind_args` 成功后进入新帧、发 `call.enter`；一旦进入帧，即使提前 return 或异常，也必须发对应 `call.exit`（若事件传输未截断/中断）。
- 参数绑定失败时尚未进入函数帧：可以发 `call.bind_failed` 非终态摘要或由 `run.failed` 报告；**不能生成一个永远不会退出的虚假 frame**。
- 每次 `return`、`branch.eval`、`loop.iteration`、`variable.assigned` 都带当前 `frame_id`，且源码区间属于声明的 module。
- 调用序号与事件序号不同；不能拿 `seq` 代替 `frame_id`（一个帧有许多事件）。
- 建议为实际循环增加 `iteration_index` 或唯一 loop occurrence 标识，但需要先明确是循环条件求值次数还是 body 执行次数，不能用模糊的次数误导 UI。第一阶段可维持逐次 `loop.iteration` 事件，不保证完整 iteration frame。

### 6.3 例子：递归与重复调用

`main -> f(2) -> f(1)`，然后 `main -> f(3)`，应产生三个不同的 f 调用实例。消费者能够由 `frame_id,parent_frame_id` 构造树；无论函数名是否重复、源位置是否相同，都不发生归属混淆。

### 6.4 同一函数遇到错误和提前 return

- 显式 `return` 时先形成 return 事件，再形成 `call.exit(status=return)`。
- 抛运行错误时形成 `call.exit(status=<error-code>)`，再最终 `run.failed`；必要时沿栈逐帧退出，顺序和错误归属一致。
- 观察项达到上限时可缺少结构化事件，消费者明确显示“部分调用轨迹”，不得重建不存在的调用帧。

---

## 7. 变量与参数值：必须“可选择且有界”，不是默认调试转储（P1）

### 7.1 先做元数据，后做具体值

变量赋值应在**实际成功提交**后发布元数据事件：变量名、类型、帧身份、准确源位置、是否发生写入等。不得重新计算 RHS，不得使用最终局部变量值填充历史赋值，更不能在失败的内存写入后伪造“已成功”。

```json
{
  "schema":"marqdo.execution-events",
  "schema_version":1,
  "run_id":"r1",
  "seq":15,
  "kind":"variable.assigned",
  "payload":{
    "frame_id":"f2","name":"result","value_type":"int",
    "module_id":"root:demo@0.1.0::index.mq.md",
    "span":{"start":120,"end":141},
    "value_state":"not_requested"
  }
}
```

### 7.2 值观测授权模型

本 RFC **不**接受 `--dump-all-locals` 作为默认开发接口。建议提供下列显式参数（命名为拟议）：

```bash
# 都是拟议命令；首版可先只实现结构事件，再实现选定值
marqdo call index.mq.md main --arg n=5 \
  --event-stream jsonl \
  --observe-variable result \
  --observe-parameter n \
  --observe-max-value-bytes 256
```

具体契约：

1. 默认 `--event-stream` **只允许类型/状态**，不含任何实参或局部变量原始值。
2. 用户必须显式列出允许观察的参数名/变量名；首版只接受**精确名称匹配**，不支持 `*`、正则、路径表达式或“全部变量”，以免范围失控。
3. 可观测值类型在第一阶段仅允许简单标量：整数、浮点、布尔；**字符串 Text 默认不输出**，需要另一个明确的 `--observe-text-values` 同意选项。列表/Map/Table/Formula/Code、Memory、Resource、PluginHandle 默认仅记录类型与安全摘要，不自动递归展开。
4. 观测值由同一个 `Value` 安全编码策略处理，不得使用 Rust `Debug`、随意序列化嵌套引用或打印 MemoryHandle 内部字段。
5. 每个值有 `max_value_bytes`、截断和失败状态；将“值未请求”“被规则遮蔽”“超过限额”“类型不支持”分别表示为 `not_requested/redacted/truncated/unsupported`，不能都显示空字符串。
6. 即使是 Int、Float、Bool 也可能表示敏感数据。观测值的权限意味着**由用户显式同意写入该输出通道**，协议不能承诺凭变量名自动正确识别秘密。
7. 文档中明确指出：值观测降低数据机密性；CLI 允许的是开发者主动的调试泄露，View 不可默认勾选，更不能从项目 manifest 自动开启。
8. 不在普通 Trace v2、普通 `--json` 的 effect trace 或 Analysis v2 中偷偷新增这些值。

### 7.3 `call.enter` 上的参数值

- 默认仅有 `function_id/frame_id` 等身份和可公开的参数签名；实际参数值不出现。
- 通过 `--observe-parameter` 选择后，可单独附加 `parameter.observed` 事件，按相同脱敏/值类型/预算策略处理。
- 函数绑定默认值也视为真实运行参数，但事件必须注明来源为 `provided/default`，不能把源码默认表达式误当做用户传参；不允许将第三方插件收到的所有原始请求自动拷贝到观测流。

### 7.4 对 View 的呈现原则

- 默认显示“变量 `result` 已赋值（Int）”，不显示 `result = ...` 的猜测值。
- 获得明确观测值时显示“本次调用 f2 在源位置 x/y 写入 42”，并能切换 f1/f2。
- 缺少值时显示“未请求/受保护”，而不是误导性的 `null/0/None`。
- 不能把运行时观测值称为形式证明。

---

## 8. 内存观测：只做元数据，保留未来的 Debug Protocol（P2）

当前 `MemoryStore` 的页、容量、已提交字节及区域概念适合提供**非内容型指标**。建议以后允许以下只读事件：

| 事件 | 允许的内容 | 禁止内容 |
|---|---|---|
| `memory.region.created` | 本次运行内不透明区域 ID、逻辑大小、容量限制 | 操作系统地址、裸指针、初始内存字节 |
| `memory.region.grown` | 旧/新提交字节数，是否成功 | 原始内存数据 |
| `memory.region.written` | 区域 ID、偏移/长度、来源 span、操作结果 | 写入字节的内容 |
| `memory.budget` | 总已提交字节数、区域数量、预算 | 进程地址空间映射 |

`MemoryHandle` 当前是带运行生命周期的内部能力，不应被事件流序列化为可在下一次调用使用的句柄。区域 ID 仅供同一次运行内展示，与 CLI 用户可操作的资源句柄无关。

**明确不开发**：随时从外部读取 MemoryStore、逐字节 dump、任意读写、设置断点、暂停线程、时间旅行。这些需要安全模型、快照一致性、会话协议与可重放语义，应另立 Debug Protocol RFC。

---

## 9. 安全、隐私与可信边界

### 9.1 威胁模型

至少考虑：不可信 `.mq.md`、不可信 Plugin/Provider/Foreign 子进程、恶意或慢速 JSONL 消费者、意外重定向到日志服务器、循环大量打印/写变量值、内存中的 Token/密码、跨源浏览器请求、源码变更产生的错位展示。

**本 RFC 不提供 OS 沙箱**。现有 `pure/project/strict` 能力与隔离承诺按照当前产品文档执行；Worker 是进程故障隔离，不等于对所有敌意扩展的 OS 级安全边界。观测模式不能改变安全级别、临时扩大权限或强制载入 Plugin。

### 9.2 强制约束

- 观测开关只改变**允许输出的信息**，不能授权网络、文件、进程、Foreign、Provider 或插件。
- 观测输出与普通程序 stdout **严格分离**；stderr 中的诊断也不得偷偷泄露被隐藏的变量/密钥。
- 用户有意识地打印的内容属于程序 stdout；该内容可能包含秘密，事件协议对此只保证字节保真、**不自动替用户脱敏 stdout**。
- `Value` 观测默认关闭；不输出资源句柄、内存字节及未经明确允许的文本值。
- 禁止将变量值记录到 CI artifacts、Issue、公开诊断中作为测试失败的默认附带信息。测试使用合成哨兵并断言其不出现。
- 与外部扩展组合使用时，应确保其原始 fd 不污染 JSONL；不能保证该点时明确标记组合不支持并 fail-closed。
- 不向浏览器直接暴露 CLI 机器管道：View Server 仅在用户明确运行后启动子进程并将受限事件转换为 SSE；其 HTTP 授权与 CSRF 防护属于 View 仓库，不要求 CLI 实现。

### 9.3 特殊模式与流输出边界

- 即使 `--mode pure`，读取 stdout 流/传事件的行为属于调用者对 CLI 进程 stdout 的消费，**不得扩大 Marqdo 程序本身的 capability grants**。
- `strict` 模式依赖平台/隔离环境；若已支持进程 stdout 管道，事件流应经过同样的隔离 preflight 验证，不能绕过 strict；不支持时显式拒绝。
- 若项目声明插件而 `pure/strict` 拒绝激活，则 event mode 也必须以相同原因拒绝，不能为了 UI 可观测性自动退回 project。

---

## 10. 执行生命周期与故障处理

```text
CLI 参数解析
  │ ├─ 无效参数 → stderr + 非零（尚未进入事件协议）
  ▼
初始化 JSONL Writer → stream.opened(seq=0)
  ▼
准备项目 / 静态诊断 / 安全预检
  ├─ 失败 → run.rejected + EOF + 非零
  ▼
run.started
  ▼
Runtime 真实执行
  ├─ stdout/stderr → *.chunk（已提交输出）
  ├─ call/branch/loop → structural（有界）
  ├─ selected Value → observed（显式授权、有界）
  ├─ Runtime 失败 → run.failed + EOF + 非零
  ├─ 成功 → run.completed + EOF + 0
  └─ OS 强杀/断管/崩溃 → 可能无终态；消费者标 incomplete
```

### 10.1 正确的终态和退出码

- `run.completed(status=ok)` 只表示解释器实际成功返回，不代表所有 stdout 已被浏览器消费；必须在关键输出**成功写入**协议通道后发出。
- `run.failed` 的 `error.code` 应尽量复用现有稳定错误族，不在新协议中另创造一套语言错误分类。
- `run.rejected` 区分 preflight/compilation/authorization；不提供不存在的返回值。
- 观测流断开后不能继续写半条 JSON；若产生副作用后出错，禁止伪称事务回滚。
- 取消由外部 Tool 管理 OS 子进程（或未来明确的 CLI 取消机制），SIGTERM/SIGKILL 后无终态是合法边界；外部消费者必须以 EOF、退出状态和超时综合判断。
- 多次运行具有不同 `run_id`；不能跨运行复用 `frame_id` 或将 A 的 `source_digest` 绑定给 B。

### 10.2 stdout 完整性与任务终态次序

成功运行必须满足：先观察到全部已接受的 `stdout.chunk` 和 `stderr.chunk`，之后才可以出现 `run.completed`。拼接 base64 解码字节应与同一个 fixture 在普通运行模式下的原始输出一致（前提是相同参数与能力配置）。不得因为 UTF-8 chunk 边界将中文变乱码，也不得因为换行将一条事件误拆成多条。

### 10.3 复杂外部扩展的输出

Host 管辖的 `resource.write("stdio")`、`打印` 正常路径必须覆盖；Foreign/Plugin/Provider 若在自己的子进程 stdout 打印，输出默认视作**外部进程数据**，必须由受控 transport 收集或拒绝该组合，不能透传到 CLI 协议 stdout。提议为可归属外部进程的输出保留未来 `external.stdout.chunk` 事件，但**不在本 Issue 假定所有第三方扩展已经具备该能力**。

---

## 11. 对既有公开协议和外部仓库的影响

### 11.1 各协议边界

| 接口 | 当前/建议 | 需要承担的职责 |
|---|---|---|
| Analysis v2 | **保持** | 静态结构、模块/函数身份、源码位置和 digest |
| Trace v2 | **保持** | 一次执行结束后的低敏、完整的受限轨迹文件 |
| Execution Events v1 | **新增** | 在线 stdout/stderr、生命周期、调用帧身份、选定可观测值 |
| CLI Tool Protocol v1 | **保持**，必要时另立兼容性扩展 | 启动独立工具、定位 `MARQDO_BIN`，不得硬编码 View |
| View SSE | **View 仓库实现** | POST 执行、GET SSE 推送、取消、页面状态与结果呈现 |
| Debug Protocol | **以后另议** | 断点、单步、状态快照、内存字节等高风险双向操作 |

### 11.2 View 无需重新实现执行引擎

`marqdo-view` 先按 RFC-VIEW-0002 已公开 `--json`/Trace v2 完成独立安装、默认不执行、显式运行和 SSE 生命周期。一旦新 Events v1 发布，**只替换一个 CLI output adapter**：

```text
现有：CLI --json 结束结果 + Trace v2 → View Execution Controller → SSE
新增：CLI --event-stream jsonl（增量）+ 可选 Trace v2 → 同一 Controller → SSE
```

不调整文档渲染核心、不生成 HTML 的第二条路径、不引入 Host 私有依赖，也不在 CLI 里判断“是不是 View 进程”。View 应对没有新协议的旧版 Beta CLI 采用明确降级/提示，不假装实时 stdout。

### 11.3 Agent、Prove、Knowledge 的边界

- Agent 能消费 stdout.chunk、状态与终态，但是否有 LLM Token 流仍取决于模型扩展是否**实际**增量输出；事件协议不生产 AI 内容。
- Prove 可消费实验观测/反例；不能把 `variable.assigned` 作为通用证明证书。
- Knowledge 可记录来源一致的执行观测；必须显式区分动态观察、人工确认与形式证明。
- 任何第三方仓库都只能依赖公开 CLI、Schema、Release Artifact，不得通过 Cargo path 依赖 Runtime 内部类型。

---

## 12. 分阶段开发任务（可直接拆分 PR）

### OBS-00：基线、ADR 与 Protocol Skeleton（P0）

**为什么**：不在不确定的源码/规范之上新增互不兼容的事件格式。

**要开发**：在 `docs/architecture/` 或 Issue 留存此 RFC；新增架构 ADR（建议下一可用编号），固定现有 `--json`、Trace v2、Host 输出语义；新增 `spec/execution-events-v1.md` 和 `spec/schemas/execution-events-v1.schema.json` 初始合同；建立 `tests/consumers/execution_events_consumer.py` 或等价独立消费者骨架。

**验收**：代码尚未实现也能针对 Schema 校验正反向 fixtures；独立文档明确 `CURRENT/PROPOSED`；不修改旧 Trace v2 Schema 的私密字段保证。

### OBS-01：Host 与 Runtime 的观测挂钩（P0）

**可能修改**：`crates/marqdo-host/src/host.rs`、`crates/marqdo-host/src/host/effects.rs`、`crates/marqdo-host/src/trace.rs`、`crates/marqdo-runtime/src/interpreter.rs`（仅观测钩子）、可能新增 `crates/marqdo-host/src/observe/`。

**要开发**：Null/Enabled Sink；Run 生命周期事件的内部表示；统一序号与安全来源身份；在 stdio 真实写入路径上发出只读字节事件；预算/IO 错误传播；兼容既有 Trace 收集。

**验收**：默认未启用时不序列化、不为每次变量赋值 clone `Value`；已有普通运行的返回、stdout、stderr、Trace/错误/权限测试全部不变；内部可对写入时刻的事件进行测试（不能事后分片）。

### OBS-02：JSONL CLI 参数与真实实时流（P0）

**可能修改**：`crates/cli/src/main.rs`、`crates/cli/src/commands/run.rs`、`crates/cli/src/output.rs`，新增 `crates/cli/src/execution_events.rs`；`spec/` + 开发者文档。

**要开发**：`--event-stream jsonl`；writer；`stream.opened`/`run.started`/`stdout.chunk`/`stderr.chunk`/`run.completed|failed|rejected`；与 `--json` 互斥、`--trace-file` 并存；严格的 stdout 纯净性和失败退出。

**验收**：长任务在结束前可读到前置输出；二进制字节可重组；不同渠道无 JSON 污染；事件行逐条合法；故障/超时/拒绝均有明确定义；无需改 View 或核心语言语法。

### OBS-03：独立消费者与发布验证（P0）

**要开发**：Linux x86_64 release 构建的已发布二进制，冻结 SHA-256；在独立 Python/Rust/Go fixture 项目读取 stdout JSONL，解析、验证 schema、重组字节，确认与终态；一个最小 View adapter JSONL→SSE 的端到端演示（属于 `marqdo-view` 仓库，不移入 CLI）。

**验收**：二进制不重新编译、不变化；独立消费者在正常/失败/断开情形均按规范完成；`marqdo --help`、README、CHANGLEOG、COMPATIBILITY、STATUS、Release Notes 明确声明新协议及适用版本。

### OBS-04：调用实例 ID 与结构事件（P1）

**可能修改**：`crates/marqdo-runtime/src/interpreter.rs`、Host 观测类型、Schema；现有 Trace v2 仅能做不破坏的内部身份对齐。

**要开发**：`frame_id`/`parent_frame_id`、enter/exit 配对、递归/多次调用归属；return/branch/loop/Host effect 与调用帧关联；`run.failed` 异常路径收束。

**验收**：递归/多调用 fixture 100% 正确归属；不存在相同帧 ID 的重复进入；传输未截断时帧完整闭合；嵌套调用与错误回卷不悬空。

### OBS-05：受控值观测（P1）

**可能修改**：Runtime 的 `bind_args`、局部写入、Host 观测过滤器、CLI 参数和 Schema。

**要开发**：精确的 `--observe-variable`、`--observe-parameter` 选择；类型摘要；标量有界序列化、文字的二次 opt-in；有据可查的 `redacted/truncated/unsupported`；**不读 MemoryStore 原始内容**。

**验收**：默认运行和只开启结构事件都不会泄露哨兵值；未选择变量绝不输出；中文字符串跨字节/超长值正确截断；无效选择和不支持的嵌套类型有明确定义；启用后每个值的帧和源码坐标正确；关闭观测时基准性能满足门禁。

### OBS-06：内存元数据（P2，可延期）

**可能修改**：`crates/marqdo-runtime/src/memory.rs` 和 Host 事件类型。

**要开发**：创建、增长、成功写入和预算统计；不透明区域标识；禁用裸指针与字节读取。

**验收**：内存区域/总提交量与真实 Store 一致；扩容失败不能产生“成功增长”；权限、类型和 budget 均不受观测影响。

### OBS-07：正式门禁、文档治理与跨平台（P0/P1）

**要开发**：在原有 Linux 主通道以及平台声明范围内运行 CI；Perf 追加 disabled/metadata/values 三档；严格隔离的组合测试，不用本次改动推断未验证平台已经受支持；更新协议版本矩阵和源码状态文档。

**验收**：见第 13/14 节。对于稳定 3.x，性能和安全门禁不能因 Beta 阶段允许 WARN 而永久免检。

---

## 13. 最小测试项目与验收矩阵

### 13.1 可执行 fixture A：真实输出与返回

```markdown
# @knowledge Stream protocol example

## @exec main

`打印("START")`

`n = 10`

`打印("DONE")`

`return n`
```

**验收**：普通 `marqdo run` 与 `--event-stream jsonl` 两条运行结果一致；JSONL 前置输出发生于 run.completed 之前；输出 chunk 重组为相同字节；返回值保持官方 wire 编码；运行结果不混入原始输出文本行。此 fixture 只测试输出事件的时间顺序，不能充当“有时间延迟”测试；延迟由下面的独立 fixture 测量。

### 13.2 实时性 fixture B：长任务中段输出

选取**当前语言已经支持**的确定性长任务：程序先打印 `READY`，再进行可测量的大量计算/循环，最后打印 `FINISH`。测试进程在读取到 `READY` 时必须仍处于运行中；如果本机任务太快，则使用受控、可重复且经授权的 `process.run` fixture 或测试用 Host 延迟，避免依赖未被正式支持的 `sleep` 关键字。**严禁仅靠 run.completed 之后 replay 的字符串伪造 PASS。**

验收标准（建议 CI）：在固定测试机上，`READY` 事件的可观察到达时间必须明显早于最终退出（例如与后续受控延迟 ≥ 300ms 之间至少有 100ms 可辨窗口），在负载波动时保留 timing 诊断，不把共享 CI 的单次偶然卡顿视为核心失败证据。可用同步闸门 fixture 做确定性测试：子进程收到 READY 后测试方才释放后续计算。

### 13.3 结构 fixture C：递归、重复调用、嵌套循环

编写合法 3.0 RFC-LANG-0002 的 `main`、`f`、`g`，至少有 `main→f→g`、两次 `f`、一次递归调用、一条真假分支和循环两次。测试断言：不同 `frame_id`、正确 `parent_frame_id`、`function_id`、源位置、事件排序和真实循环次数；提前返回与深层异常均能形成可归属的失败事件。

### 13.4 变量 fixture D：默认不泄露 / 精确启用

```markdown
## @exec main

`secret_token = "DO_NOT_LEAK_SENTINEL_1"`

`visible_count = 42`

`return visible_count`
```

注意：这是测试值泄露策略的 fixture，**不要同时打印 secret_token**，否则 stdout 按程序意图包含它，不能拿来证明观察系统泄露。

- 默认普通运行/默认 event mode / metadata mode / Trace v2：均不应包含 `DO_NOT_LEAK_SENTINEL_1`。
- `--observe-variable visible_count`：在该事件收到 42（若整数值被允许）。
- `--observe-variable secret_token` **但未显式开启文本值选项**：仅显示 `redacted`，不出现哨兵。
- 即使允许文本，也只输出被精确选择且未超预算的值；不得连带输出其他变量。

### 13.5 负面与边界验收表（每项都须可自动测试）

| 编号 | 测试 | 预期结果 |
|---|---|---|
| T01 | 不加 `--event-stream` | stdout、错误、退出码、Trace v2 与当前版本一致 |
| T02 | `--event-stream jsonl --json` | usage 错误，无混合输出 |
| T03 | `check`/`inspect` 请求 event mode | 拒绝；不执行 |
| T04 | 每条 JSONL 行 Schema v1 验证 | 全部通过；seq 从 0 单调递增 |
| T05 | 未知 Schema major 的独立消费者 | 明确拒绝、不静默当 v1 |
| T06 | stdout 中文多字节跨 chunk | base64 重组原始字节完全一致 |
| T07 | stderr 非 UTF-8 / NUL | 无数据损坏、无输出混杂 |
| T08 | 长任务中段输出 | 可在执行仍未退出时观测到 |
| T09 | `run --trace-file` 同时使用 JSONL | Trace v2 正常原子写入，digest 对应 |
| T10 | 预备失败、语义错误、授权拒绝 | event mode 有 `run.rejected`，非零退出，无 `run.started` |
| T11 | 运行错误 | `run.failed`，无 `run.completed` |
| T12 | SIGKILL / 进程崩溃 | 消费者判 incomplete，不凭空补成功 |
| T13 | 断管 / JSONL writer IO 错误 | 非零退出或已终止，不能继续无限副作用 |
| T14 | 慢消费者 | 背压有界、内存不无限增长、无静默输出丢失 |
| T15 | 结构事件预算耗尽 | 有 `observation.truncated`，后续 stdout/终态仍可传输 |
| T16 | stdout 执行预算耗尽 | 与旧 Budget 一致，不绕过限制 |
| T17 | 重复/递归调用 | `frame_id` 唯一、parent 正确、来源正确 |
| T18 | 参数绑定失败 | 不出现悬空的假调用帧 |
| T19 | return / error | enter/exit 正确配对（未截断前提下） |
| T20 | `variable.assigned` | 只在真实提交后发，源 span 精确 |
| T21 | 默认观测 | 不包含未选择的局部变量/参数值 |
| T22 | 显式选中整数变量 | 值可用且有预算限制 |
| T23 | 文本值未二次授权 | 哨兵字节不存在 |
| T24 | 不支持的复杂类型/MemoryHandle | 仅安全类型/遮蔽状态，不序列化句柄 |
| T25 | 观测打开/关闭同一程序 | 原业务结果与权限判定一致 |
| T26 | 不可信插件原生 stdout | 不污染 JSONL，必要时 fail-closed |
| T27 | `pure/project/strict` 模式 | 授权、拒绝和已声明的支持矩阵不变 |
| T28 | 源文档更改后回放旧事件 | digest 失效提示，不贴到新源码 |
| T29 | 多次运行 | 不共享 run/frame 身份 |
| T30 | 冻结发布 CLI 二进制的外部 Python/Go/Rust 消费者 | 不改 CLI 源码、不重编译、成功消费新协议 |
| T31 | 旧 Analysis v2/Trace v2 消费者回归 | 无行为/Schema 回归 |
| T32 | CI 性能对照：关闭、metadata、values | 记录可重复 p50/p95、分配和内存，异常回归阻止发布 |
| T33 | 值观测写入现有 `--json`/Trace v2 | 不允许出现新增秘密字段 |
| T34 | 错误的 `module_id/span` | 独立消费者拒绝/明确告警，不能装饰错误源码 |
| T35 | `stdout.chunk` 与 run.completed 顺序 | 全部已提交输出先于终态；逐字节保真 |
| T36 | 外部工具按 `MARQDO_BIN` 调用发布版 | 不需要私有 Cargo crate 或 Rust Host 嵌入 |

### 13.6 性能与容量验收原则

不要不经测量地宣称“零开销”。至少固定三个等级：`disabled`、`structural`、`selected values`，在相同 SHA、同工具链、同硬件节点上对循环、递归、输出密集、内存密集和 Foreign/Plugin 典型负载记录时间分布、峰值 RSS、输出字节、事件字节与丢弃计数。**未启用时不应发生 Value 序列化、长驻事件队列或全帧 clone**。任何目标百分比阈值须先通过基线验证后写入正式 release gate，不能用主观“无明显影响”代替数据。

---

## 14. 文档、CI、版本与发布完成标准

实现过程中必须同步更新：

- `spec/execution-events-v1.md`：**规范真相**，包含每类事件 required 字段、序号、终态、版本/未知字段、值策略与错误边界。
- `spec/schemas/execution-events-v1.schema.json`：Draft 2020-12；配套正负 fixture。
- `docs/developers/analysis-trace.md`：加入 Execution Events v1 的精确 CLI 使用示例、Analysis/Trace 绑定规则和禁用边界。
- `docs/developers/tools.md`、第三方手册：新增外部消费者指南；若 Tool 协商改变，必须升级对应协议规范和测试。
- `docs/developers/compatibility.md`、`COMPATIBILITY.md`：明确该协议与 Marqdo 产品/Analysis/Trace/Tool 各自独立的版本轴。
- `README.md`、`CHANGELOG.md`、`docs/STATUS.md`、`docs/architecture/CURRENT.md`、`docs/releases/`：明确已实现、已验证、已发布和限制，**不能将本 Issue 的 Proposed 描述当成已实现**。
- `crates/cli/tests/`、`crates/marqdo-host/tests/`、`crates/marqdo-runtime/tests/`、`tests/consumers/`：增加上述测试。
- 适用平台的 CI：`cargo fmt --check`、clippy、workspace tests、JSON schema 正负验证、独立 frozen-binary contract、必要的 strict/安全与性能 lane；发布结论以同 SHA 证据为准。

### 14.1 Definition of Done（完全完成的判定）

必须同时满足：

- [ ] `marqdo run/call` 真实增量输出 Events v1，至少具备 P0 事件、严格 NDJSON 和终态/错误行为。
- [ ] 事件来自**实际的 Host 输出提交点**，不是最终缓冲拆包；持续运行时消费者能读到先发出的 stdout/stderr。
- [ ] 现行 `--json`、Trace v2、Analysis v2、授权模式、语言语义及普通 CLI stdout 保持原样。
- [ ] P1 若作为本 Issue 完成范围：正确 frame identity 和变量/参数观测，默认不泄露值且显式选择严格生效；否则在发布记录明确 `OBS-04/05 pending`，不对外宣称已支持。
- [ ] 禁止原始句柄/内存、未获同意的文本值和外部插件 fd 污染 JSONL。
- [ ] 慢消费者、终端断开、预算截断、取消/崩溃、严格模式均有负面测试；无伪造成功或无限缓冲。
- [ ] 使用发布版二进制与独立第三方消费者完成黑盒验证，二进制 SHA-256 测试前后不变。
- [ ] 文档、Schema、fixtures、版本矩阵和同 SHA 的 CI/性能/安全证据一起提交并更新，避免文档与代码脱节。

---

## 15. 反模式与禁止事项（适用于所有 PR）

**禁止**：

1. 在 CLI 增加 `view` 专用命令/模块或只在 View 进程启动时才启用的私有路径。
2. 在核心引入 HTTP server、SSE、WebSocket、前端、Notebook、Agent 会话、Knowledge/Prove 业务逻辑。
3. 把普通 stdout 和 JSONL 事件写到同一物理 stdout；把最终字符串拆成“实时”chunk。
4. 将 `Frame.locals`、整个 MemoryStore、原始 C ABI/OS 指针或任意 `Value` 的 Rust `Debug` 输出给外部消费者。
5. 修改 Trace v2 的“不包含敏感值”承诺而仍维持 `schema_version:2`。
6. 以强行重写 JSONL 或自定义解析 Markdown 语义方式绕过原来的 Analysis digest。
7. 允许消费者拖慢时分配无界 Vec、队列或后台线程；静默丢 stdout 或伪造已完成。
8. 观测失败后继续无限运行带副作用任务而不回报错误。
9. 依赖私有 `inspect --ir`、`marqdo-ir` Cargo path 或宿主内存布局来实现第三方工具。
10. 将多次运行的某个值当作通用定理的证明，或把“本次未观察到”当作“本次没有发生”。

---

## 16. Issue 可以直接拆出的执行清单

- [ ] **OBS-00**：确认最新 SHA / 审批 RFC / ADR / 公共 Schema / fixtures。
- [ ] **OBS-01**：实现内部 typed sink、Null/Enabled 配置、Host stdout/stderr 原位事件与可传播 IO 故障。
- [ ] **OBS-02**：`run/call --event-stream jsonl`、Lifecycle、base64 Chunk、预算与既有 `--json` 兼容。
- [ ] **OBS-03**：真正独立的 frozen-binary 消费者和 live-output gate；更新 CLI 版本文档。
- [ ] **OBS-04**：Frame ID / Parent ID / 递归调用及分支、循环事件正确归属。
- [ ] **OBS-05**：显式选择、预算受限的参数/局部变量值观测；默认隐私 gate。
- [ ] **OBS-06（可后移）**：Memory metadata（仅大小、区域、写入范围）。
- [ ] **OBS-07**：覆盖所有负面测试、安全、性能、同 SHA 证据和 Release Notes。

**建议合并策略**：先交付 P0，可供 View/Agent 消费；再交付 P1 的帧身份与值观测；P2 不阻塞。P0 结束后，即使 P1 未完成，也应能真实改善实时输出体验。任何实施中发现的语言语义缺口要记录为**独立 issue**，不能把本项扩展成全能调试器。

---

## 17. 参考源码与权威文档（便于接手开发者逐文件定位）

**当前 CLI（审查 SHA `eeea01d`）**：

- [`crates/cli/src/main.rs`](https://github.com/cflmy/marqdo-cli/blob/eeea01d672c07b6be7d6444031d1619309dc88ab/crates/cli/src/main.rs) — 参数定义与 `run/call` 分发。
- [`crates/cli/src/commands/run.rs`](https://github.com/cflmy/marqdo-cli/blob/eeea01d672c07b6be7d6444031d1619309dc88ab/crates/cli/src/commands/run.rs) — 预备、授权、Host 配置、执行、完成后 JSON/Trace。
- [`crates/cli/src/trace.rs`](https://github.com/cflmy/marqdo-cli/blob/eeea01d672c07b6be7d6444031d1619309dc88ab/crates/cli/src/trace.rs) — Trace v2 完整文件 Writer。
- [`crates/cli/src/analysis.rs`](https://github.com/cflmy/marqdo-cli/blob/eeea01d672c07b6be7d6444031d1619309dc88ab/crates/cli/src/analysis.rs) — Analysis v2 / digest。
- [`crates/cli/src/output.rs`](https://github.com/cflmy/marqdo-cli/blob/eeea01d672c07b6be7d6444031d1619309dc88ab/crates/cli/src/output.rs) — 当前 `--json` 诊断与最终 envelope。
- [`crates/marqdo-runtime/src/interpreter.rs`](https://github.com/cflmy/marqdo-cli/blob/eeea01d672c07b6be7d6444031d1619309dc88ab/crates/marqdo-runtime/src/interpreter.rs) — 调用帧、参数绑定、赋值/分支/循环及来源。
- [`crates/marqdo-runtime/src/memory.rs`](https://github.com/cflmy/marqdo-cli/blob/eeea01d672c07b6be7d6444031d1619309dc88ab/crates/marqdo-runtime/src/memory.rs) — MemoryStore 的区域和预算边界。
- [`crates/marqdo-host/src/host.rs`](https://github.com/cflmy/marqdo-cli/blob/eeea01d672c07b6be7d6444031d1619309dc88ab/crates/marqdo-host/src/host.rs) — Host Trait、StdHost、stdio 缓冲与 resource 写入。
- [`crates/marqdo-host/src/host/effects.rs`](https://github.com/cflmy/marqdo-cli/blob/eeea01d672c07b6be7d6444031d1619309dc88ab/crates/marqdo-host/src/host/effects.rs) — 现有 Host effect/sequence 记录。
- [`crates/marqdo-host/src/trace.rs`](https://github.com/cflmy/marqdo-cli/blob/eeea01d672c07b6be7d6444031d1619309dc88ab/crates/marqdo-host/src/trace.rs) — 现有语义事件/预算类型。
- [`spec/analysis-v2.md`](https://github.com/cflmy/marqdo-cli/blob/eeea01d672c07b6be7d6444031d1619309dc88ab/spec/analysis-v2.md)；[`spec/trace-v2.md`](https://github.com/cflmy/marqdo-cli/blob/eeea01d672c07b6be7d6444031d1619309dc88ab/spec/trace-v2.md)；[`spec/tool-v1.md`](https://github.com/cflmy/marqdo-cli/blob/eeea01d672c07b6be7d6444031d1619309dc88ab/spec/tool-v1.md) — 现行公共协议边界。
- [`docs/developers/analysis-trace.md`](https://github.com/cflmy/marqdo-cli/blob/eeea01d672c07b6be7d6444031d1619309dc88ab/docs/developers/analysis-trace.md)、[`docs/developers/tools.md`](https://github.com/cflmy/marqdo-cli/blob/eeea01d672c07b6be7d6444031d1619309dc88ab/docs/developers/tools.md) — 第三方消费接口。

**关联产品需求（不是 CLI 接口真相）**：`marqdo-view` 的 RFC-VIEW-0002 定义“默认静态、显式执行、SSE 实时任务状态、交互只通过 CLI 子进程”，但本 Issue **不依赖**该文件才能实现；CLI 只需提供以上通用 JSONL 流。旧 `cflmy/marqdo` 的 View 功能和源码仅能辅助回归 UI 体验，不得迁移旧私有 Host/Notebook API。

---

### 结语：应从“View 要变量面板”转为“语言提供可信可观测性”

这次改动的价值不在于让某个旧页面复活，而在于：**Marqdo 的执行语义由 CLI 唯一负责，所有外部工具获得同一份可验证、可限制、可授权、可版本化的运行事件。** 默认运行不泄密、不改变既有程序；需要更深观测时用户明确选择且能看到风险；每条可见结果都有来源、身份和失败语义。这样才能在后续开发 `marqdo-view`、`marqdo-knowledge`、`marqdo-prove`、Agent 和扩展库时继续保持真正的仓库解耦。
