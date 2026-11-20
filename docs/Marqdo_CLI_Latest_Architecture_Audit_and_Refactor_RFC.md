# Marqdo CLI 最新提交架构审查与重构 RFC

> 状态：**审查报告 / 待讨论设计，不是已实施代码**  
> 仓库：[`cflmy/marqdo-cli`](https://github.com/cflmy/marqdo-cli)  
> 冻结审查提交：[`7fbe069196e61d06a00d64fe8d65049c327373eb`](https://github.com/cflmy/marqdo-cli/commit/7fbe069196e61d06a00d64fe8d65049c327373eb)  
> 编写日期：2026-11-21；依据 GitHub 仓库树、源码、ADR、spec、stdlib、测试文件和状态记录，**没有在本环境重新构建或执行完整测试**。  
> 重要约束：**标准库继续保存在同一仓库**；保持 Markdown 原生语法、`std:` 导入、能力授权及既有稳定语义。

## 0. 审查结论

**不是计算模型需要再推倒重来，而是 `语言核心 → 运行时 → 平台边界 → 标准库 → CLI` 的职责未完成收敛。**

- 已有 `Memory` 和 `resource.*`，意味着 RFC-CM-0001/0002 的重要骨架已落地；不能再把“缺少独立内存”“打印仍由 `HostOp::Print` 实现”当作现状。
- 但标准库目前主要是 `format.mq.md`、`text.mq.md`、`io.mq.md`、`data/table.mq.md`、`net/http.mq.md`；`io` 仅提供 `写文本/写值`，缺乏输入、文件和面向用户的通用流操作。
- 仓库有大量**证明可以写**的 `.mq.md` 测试，但它们不等于已经**作为标准库产品发布**：`JSON`、CSV、分配器、UTF-8 解码、字节码虚拟机等主要见于 `tests/conformance/semantics/`。
- 现有 `resource.*` 与旧 `fs.read/write` 并存；`Host` 仍有数学、Foreign、Plugin 等操作；Provider、Plugin、Foreign Kernel、Tool 具有不同职责，但其注册、策略、诊断、输送方式的实现入口日益分散。
- 物理文件组织尚可维护，不应以“crates 多”为由全面合并。真正要减少的是**同类逻辑的实现入口**、大文件承担多种职责、文档互相矛盾，以及把测试夹具当成用户标准库的错觉。

**本 RFC 的最高优先级：构建一个可靠的 `stdin/stdout/stderr + file + byte streams` 底层端点；基于它用 `.mq.md` 交付完整 `std:io` / `std:fs` / `std:data`；在此之后再拆分超大模块、收敛扩展注册面。**

## 1. 事实依据与边界

### 1.1 已实现、但容易被忽略的能力

| 能力 | 最新代码中的事实 | 限制 |
|---|---|---|
| 自有计算 | Markdown/Expression/Frontend/IR/Interpreter 均已独立实现 | 仍为 tree-walk，未实现 native optimizer/JIT |
| 程序可寻址内存 | `marqdo-runtime/src/memory.rs`：独立区域、64 KiB 页、增长、字节读写、别名共享、运行级预算 | 区域在整次 run 中存在；无语言级释放整个 region；默认每区域16 MiB、每次运行64 MiB |
| 位置更新 | `m[i] = byte` 已实现 | 并非通用的 `list[i]=...` / `map[k]=...` 原地可变引用语义；普通集合仍为值语义 |
| 结构操作 | `list.append/concat`、`map.keys/has/get/set/remove`、`table.columns/row/cell`、`text.codepoints/from_codepoint` | 只是一组必需底层操作，不能替代完整标准库 |
| 标准库载入 | `stdlib/` 同仓库；`std:` 显式导入；`stdlib.json` 摘要校验 | 缺安装包一致性和 API 分层的完整验收；不要拆库 |
| 打印 | `print/打印` 通过加载 `stdlib/io.mq.md` 的 `写值` 执行；原 `HostOp::Print` 已移除 | stdlib 输出路径仍只写一次、固定一页缓冲、未逐项检查 Result |
| 底层文件 | `resource.open("file",..., ["read"]/["write"])`；`fs.read/write` 遗留接口 | 读取时整文件载入、有 8 MiB 限额；写入时缓冲到 close；没有完整 `std:fs` |
| TCP | `resource.open("tcp", IP:port,["read","write"])` | IP 字面量、同步、5 秒超时；缺 DNS、TLS、监听、UDP |
| 资源 Provider | 进程内、worker、C ABI 等实现已经存在 | 与 Plugin、Foreign 的注册和安全边界重叠，不能认为它们都是一种 API |
| 严格隔离 | Linux Bubblewrap 且探测成功时严格执行；本地 VM 有证据 | 不能称 Windows/macOS 或所有 Provider 已获相同保障 |

### 1.2 标准库中哪些是真正的公开 API？

此 HEAD 的正式 `stdlib` 函数源码集中于 `format.mq.md`、`text.mq.md`、`io.mq.md`、`data/table.mq.md`、`net/http.mq.md`，另有 `demo.mq.md` 和三个测试文档。现状**不是空标准库**，但明显没有达到日常 CLI 所需的覆盖范围。

已在 conformance 中证明，但仍需产品化的候选：

- `tests/conformance/semantics/55-first-fit-free-list-allocator.mq.md`：空闲链表分配器；
- `59-utf8-encoder` / `60-utf8-decoder`：编码/校验解码；
- `62-json-array-parser`：JSON 解析相关能力；
- `63-memory-bytecode-vm`：虚拟 CPU/字节码；
- `68-csv-quoted-row`：引号转义 CSV；
- `70-matrix-multiply`、`71-http-parser`、`72-table-columnar`、`73-sparse-matrix`。

**结论：测试证明的是表达能力，标准库证明的是用户能依赖的 API 及错误、性能、版本契约；两者不能相互代替。**

### 1.3 可以明确定位的结构偏移

1. `crates/cli/src/commands.rs` 约 1539 行，同时承担加载调用、运行环境组装、严格隔离启动、工具输出、依赖命令和文件写入；`cli/src/loader.rs` 也掺入特定 `print` 识别/重定向。CLI 过度承担执行策略。
2. `crates/marqdo-host/src/host.rs` 约 1138 行，负责 `HostOp`、资源分发、权限与 Provider 注册；`resource.rs` 约 540 行混合 stdout、文件、TCP、Provider 句柄。资源能力正确，但组织已超出一个文件的合适边界。
3. `marqdo-runtime` 依赖 `marqdo-host`，即语言运行内核直接依赖具体平台边界模块，尚未做到执行引擎与平台实现清楚的依赖方向；是否引入一个薄接口 crate 必须根据下一阶段的独立嵌入需求决定，禁止盲目增 crate。
4. `HostOp` 同时拥有高层数学内核、Foreign、Plugin、遗留 `fs.*`，又存在新的 `resource.*`；**高级操作与基础系统操作在一个分发层并存**。
5. `Plugin ABI` / `Resource Provider` / `Foreign Kernel` / `CLI Tool` 在功能上不同，但基础注册、授权、版本、签名、传输、诊断等通用机制缺少统一的架构地图。删除其中某一种协议会伤害已有消费者；应先定义所有权，后考虑传输层复用。
6. 文档状态漂移：`docs/architecture/README.md` 仍称 P5 Provider 未实现，`docs/evidence/2026-11-20-rfc-cm-completion-audit.md` 却记录 Provider/P5 的本地验收；`docs/STATUS.md` 把较老的 `c9f01aa` 描述为 GitHub master，而此次实际最新为 `7fbe069`。**应把状态“生成与冻结”机制纳入重构，而非靠人手更新数十份计划。**
7. `stdlib/io.mq.md` 的“写文本”总是 `memory.new(1,1)`，把 Unicode 写入缓冲后只 `resource.write` 一次，不检查 `{ok, count}`；输入大于 64 KiB 会越界/失败，潜在部分写失败被吞掉。这个问题属于标准库正确性，不是“再加一个 Rust println”能解决的。
8. `stdlib/net/http.mq.md` 以 Unicode 码点文本为基础解析响应和 chunked 数据，而 HTTP 的长度以**字节**为单位；对非 ASCII 正文、混合编码、大小写不敏感头字段、碎片读取和消息上限不能据此宣称完整 HTTP/1.1 实现。

## 2. 目标架构：五个清晰的责任层

```text
   用户 .mq.md 程序 / Agent / 科研工作流
                    │
   stdlib/* (.mq.md，仓库内版本化与签名/摘要)
      ├─ core/collections/text/encoding/data/math
      ├─ io/console/stream/fs/net/process
      └─ memory/alloc + 可移植纯算法
                    │
   语言语义层：Markdown → AST → Frontend → IR
                    │
   执行层：Value / MemoryStore / evaluator / budgets
           （只描述计算，不拥有 stdout 文件句柄）
                    │
   受控平台接口：open/read/write/close + 非流式请求
        ├─ capability / budget / audit / tracing
        ├─ stdio / file / tcp / OS 适配器
        └─ 显式注册的 Provider（可 in-process / worker / C）
                    │
   操作系统 / 外部服务 / 设备

   CLI 只做 Composition Root：参数、工程加载、装配、执行、输出
   Plugin callable / Foreign legacy / Tool protocol = 并列的扩展类别，
   由独立注册模块负责，不直接污染标准库或语言 IR。
```

### 2.1 宪法性边界（应成为 CI 架构检查规则）

- **语言语义内核**：表达式、控制流、局部变量、List/Map/Table 语义、数值和字节行为、内存区域、值与位置赋值、函数/文档引用、错误源位置。它不应知道 `stdout`、项目目录、TCP 超时或第三方插件路径。
- **平台接口**：负责不可纯计算实现的 I/O、系统时钟、熵源、网络、文件句柄、权限校验；这些接口的宿主实现是必要的，但不能承载 JSON 解析、文本格式化、HTTP 语义或内存分配策略。
- **标准库**：以 `.mq.md` 实现普通 API、协议、算法、容器管理。**普通能力新增时，只改 stdlib + tests + docs，不改 Rust Builtin。**
- **CLI**：只负责选择输入项目、解析命令、加载配置、装配 Host 和 Runtime、路由人类/机器输出；不是语义策略中心。
- **扩展接口**：原生函数扩展和资源扩展都经同一授权审计机制，但函数调用与有状态流仍保留不同能力类型。

### 2.2 引擎与平台的依赖方向

当前 `marqdo-runtime → marqdo-host` 并非立即必须合并或新增接口 crate 的理由。分两步：

**第一步**：把所有 I/O 分发和许可策略严格收敛到 `marqdo-host`，`runtime` 不再增加外部操作分支；建立可以注入 FakeHost 的语义测试。

**第二步（当支持嵌入式 Runtime 或第二个执行后端时）**：引入最薄、无平台依赖的 `ExecutionPort` 契约，并让 `marqdo-host` 实现它。若 Rust crate 会形成环依赖，才考虑一个不足数百行的共享接口 crate；**禁止为每个端口和 Provider 单独设 crate**。

## 3. 目标目录（同一仓库，不拆 stdlib）

只展示重要目录；不代表一次性移动所有 crate：

```text
marqdo-cli/
├── Cargo.toml
├── crates/
│   ├── marqdo-markdown/      # 只保留 Markdown profile、AST、source span
│   ├── marqdo-expression/    # 表达式词法、语法与 AST
│   ├── marqdo-frontend/      # 映射、名称分析、静态诊断
│   ├── marqdo-ir/            # 稳定执行 IR；不积累高层 API
│   ├── marqdo-value/         # 值语义及 wire 合约
│   ├── marqdo-runtime/
│   │   └── src/
│   │       ├── interpreter.rs
│   │       ├── memory.rs
│   │       ├── place.rs     # 可赋值位置与统一读取/写入
│   │       ├── primitives/  # 确实不可由 .mq.md 有效组合的计算原语
│   │       └── lib.rs
│   ├── marqdo-host/
│   │   └── src/
│   │       ├── session.rs   # 每次执行的 Host 状态、资源预算
│   │       ├── resource/
│   │       │   ├── table.rs # 句柄登记、生命周期、权限
│   │       │   ├── stdio.rs
│   │       │   ├── file.rs
│   │       │   ├── tcp.rs
│   │       │   └── provider.rs
│   │       ├── providers/   # registry/native/worker/C 实现适配
│   │       ├── effects/     # auditing、trace、capability
│   │       ├── legacy/      # 旧 fs / foreign 迁移包装
│   │       └── lib.rs
│   ├── marqdo-package/      # import / lock / package identity
│   ├── marqdo-sandbox/      # OS 隔离证明，绝不移入 stdlib
│   ├── marqdo-foreign/      # 可选兼容执行器，非内核
│   ├── marqdo-math/         # 临时可选原生数学实现，逐项确立归属
│   ├── marqdo-plugin-abi/   # 原生可调用扩展协议
│   ├── marqdo-plugin-sdk/
│   ├── marqdo-provider-worker/
│   └── cli/
│       └── src/
│           ├── main.rs
│           ├── args.rs
│           ├── commands/    # run, call, check, inspect, package, tooling
│           ├── project/     # loader, stdlib resolver, project context
│           ├── assembly.rs  # RunSession 的惟一装配位置
│           └── output.rs
├── stdlib/                   # 保留在同一个 git 仓库
│   ├── stdlib.json           # 继续 digest 校验
│   ├── core/result.mq.md
│   ├── collections/list.mq.md
│   ├── collections/map.mq.md
│   ├── text/utf8.mq.md
│   ├── text/format.mq.md
│   ├── text/scan.mq.md
│   ├── memory/alloc.mq.md
│   ├── io/stream.mq.md
│   ├── io/console.mq.md
│   ├── fs/file.mq.md
│   ├── fs/path.mq.md
│   ├── net/tcp.mq.md
│   ├── net/http1.mq.md
│   ├── data/json.mq.md
│   ├── data/csv.mq.md
│   ├── data/table.mq.md
│   ├── io.mq.md           # 兼容门面，保留旧 std:io
│   ├── format.mq.md       # 兼容门面，保留旧 std:format
│   ├── text.mq.md         # 兼容门面，保留旧 std:text
│   └── tests/             # 对公共 std: API 做产品级测试
├── tests/
│   ├── conformance/       # 语义契约（无需变成文档产品）
│   ├── architecture/      # import graph / primitive-only tests
│   ├── e2e/               # 纯 Marqdo CLI，输入/文件/HTTP/JSON 等
│   └── security/
├── spec/                   # 真正冻结的语言和 ABI 规范
├── docs/
│   ├── architecture/       # 唯一当前架构总览 + RFC 决策索引
│   ├── adr/                # 只记录决策，不冒充实时状态
│   ├── evidence/           # SHA 固定的事实快照
│   ├── history/             # 已过时规划
│   └── STATUS.md            # 自动采集或引用当前状态来源
└── tools/                  # 打包、CI、文档一致性检查和 benchmark
```

**迁移原则：先移动职责，再移动路径。** 不因为希望目录好看就制造公共 API 大面积重命名；已公开的 `std:io`、`std:format`、`std:text`、`std:data/table`、`std:net/http` 必须有兼容导入门面和逐版本弃用协议。

## 4. 系统 I/O 的最小充分基座

### 4.1 必须补上的真正底层能力

当前 `resource.open("stdio","stdout",["write"])` 已有。**缺少 `stdin` 读端点**，这是不能只用已有资源来拼出的新 OS 交互能力。应扩展同一 `stdio` 资源类别，不增加语言关键字、`Input` builtin 或第二套控制台协议。

目标：

```text
resource.open("stdio", "stdin", ["read"])  -> Resource
resource.open("stdio", "stdout", ["write"]) -> Resource
resource.open("stdio", "stderr", ["write"]) -> Resource
resource.read(handle, Memory, offset, max_bytes) -> Result({count,eof})
resource.write(handle, Memory, offset, length)    -> Result({count})
resource.close(handle)                            -> Result
```

stdin 与 stdout 使用同一个 `ResourceTable`、同一个权限检查和同一 Trace 契约。`stderr` 必须区分**应用输出**与 CLI 自身的诊断，以免 `--json` 机器可读输出混合无关数据。所有端点均遵守可取消/超时与资源预算（第一版可采用同步阻塞，但规范明确等待策略与取消路径）。

**不能将 `read_line` 作为新 Rust Builtin。** 它应位于 `stdlib/io/console.mq.md`，由 `resource.read` + 有界缓冲 + 增量 UTF-8 解码 + 换行判定构建。

### 4.2 修复现有 `stdlib/io.mq.md` 的正确性缺陷

现状：固定 `memory.new(1,1)`，单次 `resource.write`，无 `{ok,count}` 校验。目标 `write_all` 必须：

1. 将任意长度文本按**字节**分片编码，或按需增长 Memory；禁止超过区域上限时静默截断。
2. `resource.write` 若 `{ok:false}`，返回带原因的 `Result`；若 `0 < count < requested`，继续写剩余字节；若 `count=0` 且有剩余，返回 `io.write_zero` 防止死循环。
3. 不因 `> resource.write(...)` 语句返回值被忽略而掩盖错误；`打印` 的失败须有明确可观测结果。若要求完全兼容旧 `print` 的致命失败行为，需要规定 `Result` 到致命错误的桥接方式，不能在 `.mq.md` 中擅自吞错。
4. 使用统一缓冲生命周期与容量预算；stdout、文件和 TCP 共享 `write_all`，只在入口处选择资源。
5. 确保多字节 Unicode、嵌入 NUL、超过一页、短写、失效资源、无权限、EOF 等均有测试。

### 4.3 文件：不是没有底层能力，而是接口不完整

目前 `resource.open("file", path, ["read"]/["write"])` 可工作。但实现为：读取时整文件转入缓冲（8 MiB 限制），写入时缓存在 Host 并于 `close` 提交。这是当前阶段的安全简化，不是通用 OS 文件流。

**标准库立即可构造**：`read_all`、`read_text`、`write_text`、`write_bytes`、`copy_file`（限当前文件容量）、统一 close/error 检查。

**需要真实的底层新增**：开放大文件的增量读写；追加和 seek；stat/exists/listdir/mkdir/remove/rename 等非流式路径操作。后二类不能通过 `resource.read/write` 神奇推导出来。

推荐能力合同分两类：

- `Resource Stream`：打开、读、写、关闭，负责字节流和文件位置。
- `Resource Operation`：受版本协议和**逐操作授权**的元信息/目录修改请求，用于不能归结为流的系统命令；候选形态 `resource.request(scope, operation, arguments)`，**不是无授权的万能 syscall**。引入前需 ADR 固定操作命名、路径规范化、能力范围、返回错误、审计与默认拒绝。

第一版不扩充复杂的文件事件监听、mmap 及目录枚举细节；只实现能支撑真实 CLI 文件工具的最小操作组。旧 `fs.read/write` 保持一段时间的兼容 shim，最终统一到 Resource 路线，避免双路径权限规则。

### 4.4 网络：低层缺口与 stdlib 缺口分别处理

- 现在：IP-literal TCP、同步、5 秒超时、HTTP `.mq.md` 请求构造和部分响应解析。
- 标准库：`tcp.connect/read_exact/write_all`、可增量的 HTTP/1.1 状态机、二进制 body、Content-Length 和 chunked 字节协议、错误 Result。
- 新 OS/Provider 能力：DNS 解析、TLS（可通过经过审计的安全传输 Provider）、监听、UDP、系统证书存储和可信熵源。**不应宣称普通裸字节内存自动生成安全 TLS 或 DNS 能力。**
- 当前 `stdlib/net/http.mq.md` 不应以码点长度处理分块字节长度；在面向通用网络发布之前先改为字节流解析。

## 5. 功能归属：内核 / 平台 / 标准库 / 可选加速

| 能力 | 推荐归属 | 是否应新增 HostOp / Rust 算法 |
|---|---|---|
| `Int/Float/Bool` 算术与比较、函数控制流 | 内核 | 核心原语，保留 |
| `Memory` 区域创建、增长、边界检查、字节访问 | 内核 | 保留；由解释器维护抽象存储 |
| `List/Map/Table` 必要结构观察与构造 | 内核 | 保留最小闭包，不增排序等算法 |
| `type/len`、结构访问、值/位置语义 | 内核 | 保留契约，慎重扩展 |
| 动态分配器、free-list、内存池、链表和树 | `stdlib/memory`、`collections` | 不增加 Rust 算法 |
| UTF-8 解码、文本转义、数字格式化 | `stdlib/text` | 原生可作为等价加速，不拥有唯一语义 |
| `打印`、输入行、文件复制、日志格式化 | `stdlib/io` / `fs` | 不增高层 Builtin；底层系统操作必要 |
| JSON/CSV、GFM 表格变换、HTTP 解析 | `stdlib/data` / `net` | 纯 `.mq.md` 必须有参考实现 |
| `std:fs` 目录/元数据/重命名 | 标准库包装 + 平台 `Resource Operation` | 需要**通用能力端点**，不是每个函数单独 HostOp |
| OS 文件句柄、网络 socket、stdio、时间、熵源 | Host / OS adapter | 必须有少量可信系统调用实现 |
| 数学符号求值/微分、科学优化 | 可选 library 或加速层 | 逐项评估；不要放进语言语义基石 |
| Python/Node Foreign | 可选兼容扩展 | 不作为标准库存在性的依赖 |
| 原生插件导出 | Callable Plugin | 不能替代普通自举算法 |
| 资源 Provider | Resource Provider | 不能替代普通 `std:io` |
| `marqdo-<tool>` 工具 | CLI Tool Protocol | 与应用运行时 API 严格分离 |

特别说明：当前的 `Value::Table` 使用 `Vec<Vec<Value>>` 作为存储并不意味着架构失败。先保证 GFM 源码、逻辑 Table、物理存储三层分离、确认普通 Table 的值语义，再按性能证据改存储；优先建设**能直接使用的表格 API**而不是立即上列存引擎。

## 6. 扩展体系如何收敛，而不推倒已有工作

**不要粗暴删除四种看起来类似的扩展方式**：它们的边界不同。

| 当前机制 | 保留的真正职责 | 应停止承担的职责 |
|---|---|---|
| Tool Protocol | `marqdo` CLI 自身的外部工具、inspect/verify/IDE 等命令 | 在用户程序运行过程中替代标准 I/O |
| Callable Plugin ABI | 可调用的原生函数扩展、受控实验加速 | 把普通 JSON/CSV/格式化都做成原生 ABI |
| Resource Provider | 新资源类型和设备/网络/文件端点 | 为每个高级算法创建一个 Provider |
| Foreign Kernel | 明确请求跨语言计算的兼容层 | Marqdo 实现常见算法的常规路径 |

共用基础设施（**先集中到 `marqdo-host` 内部的统一 Registry/Policy 模块，暂不增加 crate**）：

- package/adapter identity、descriptor/version validation；
- 精确能力授予与每次请求审计；
- 统一超时、预算、错误分类和 trace metadata；
- 进程内/worker/C 等传输后端的记录方式；
- 严格模式拒绝未经证明的适配器，不把“worker 子进程”误称沙盒。

不应人为强迫函数调用与资源流共享一份万能二进制 ABI。共同的是**注册和权限元模型**，而不是计算语义。

## 7. CLI 模块治理与目录重构步骤

### P0：冻结最新行为、建立一张事实总表

- 建立单一 `docs/architecture/CURRENT.md`，固定审查 SHA、实现/测试/标准库发布/多平台验证四个状态维度。
- 对照 `docs/STATUS.md`、`docs/architecture/README.md`、RFC 完成审计，消除旧 `master` SHA 与 P5 状态冲突；旧状态移动至 `docs/history/` 或加明显“历史快照”标志。
- 建立源依赖图、公开 API/stdio/file/TCP 契约、现有扩展路径分类图；记录 baseline 而非先改文件名。
- CI 加入简单文档一致性检查和标准库 manifest/digest 校验。

**验收**：任何对外状态声明都能指向固定 commit、真实测试或源码位置；不允许“测试有例子”冒充“stdlib 已发布”。

### P1：把标准输入与 I/O 正确性做成第一条闭环

1. Host 增加 `stdio/stdin`（可选 `stderr`）且复用 `resource.*`；保留现有 stdout 安全检查。
2. 实现 `stdlib/io/stream`：`read_chunk`、`read_exact`、`read_to_end`、`write_all`、`close_checked`。
3. 实现 `stdlib/io/console`：`read_line`、`prompt`、`print`、`println`，行缓冲、EOF 与 UTF-8 错误一致。
4. 修复当前 `stdlib/io.mq.md`：复用这些函数，保持 `打印` 外部行为不变。
5. 保持 `--json` 通道纯净，诊断/trace 与程序输出严格分流。

**验收**：通过管道将三行输入送入 `marqdo run`，程序按行响应；单行超过 64 KiB、短读/短写、空输入 EOF、无权限、错误 UTF-8 不崩溃、不丢字节；同一测试必须使用 `.mq.md` 实现 `read_line` 而非 Rust 新内置。

### P2：文件与完整数据标准库

- 发布 `std:fs/file` 基本 API：read/write/copy；迁移旧 `fs.read/write` 到兼容包装而不是形成两个长期安全边界。
- 将 conformance 中成熟的 JSON、CSV、UTF-8 decode、整数编码、free-list allocator 迁为公共 stdlib API；移动逻辑后重写测试，确保测试导入**实际标准库模块**。
- 文件增量读写与目录/元数据行为需要最小的底层 Resource 扩展 ADR；**不能将目标 API 列入已实现直到完成**。
- `std:data/table` 保留可读 API，增加列名选择、空值/类型校验、按键访问、流式 CSV→Table；对稳定输出保留复制/值语义。

**验收**：纯 `.mq.md` 实现并测试文件复制、JSON 读改写回、CSV→Table→GFM、UTF-8 错误报告；追加一个新的数据格式适配器仅改 `.mq.md`。

### P3：分离 CLI/Host 中的多重职责

- 不改公开 API，先将 `cli/commands.rs` 按 run/call/check/inspect/package 分成内部模块；`stdlib.rs`、`loader.rs` 属于 project 加载路径。
- 将 `host.rs` 的资源处理和 Provider 注册拆到 `resource/`、`providers/`，Policy 与 Trace 保留统一入口。
- 将 StdHost 建立为一次性 `RunSession`；配置、权限、stdlib digest、资源句柄生命周期及输出路由由一个装配函数确定。
- 外部 Tool Protocol 不参与 `marqdo run` 的普通标准库函数解析。
- 不改变 `marqdo-runtime` 对外执行结果，不因“美观”合并所有小 crate。

**验收**：Public API/trace/ABI digest 不变；主 CLI 的 run/call/check 与 strict 安全测试全部维持；对 `src/commands.rs`、`src/host.rs` 做职责级拆分，而不是纯机械拆文件。

### P4：系统资源扩展与生产可用性

- 在稳定的授予模型上增加 `fs` 元数据与目录事务、DNS/TLS Provider（安全审计单列）、stderr、时间/熵源等；其适用顺序以真实应用需求和安全边界为依据。
- 把 HTTP 从 Unicode 文本演示升级成二进制增量解析；协议处理仍为 `.mq.md`。
- 标准库可在不修改冻结 CLI 二进制的情况下增加新模块/补丁版本，仍在同仓库发布和统一打包。

**验收**：能用标准库完成交互式 CLI（逐行读写）、大文件增量处理、文件清单与复制、HTTP 请求（在有授权的传输端点上）；新增协议解析不引入 Rust 算法。

### P5：可选优化，而不是再次堆原语

- 测量 `format.显示` 文本拼接、表格值复制、List 结构变换、字节循环读写的成本。
- 保留纯 `.mq.md` 的参照语义；仅对经基准证明的瓶颈设计内核优化（例如有界 bulk memory copy、结构共享、字符串 builder、编译后端），不得改变可观测语义。
- 在确实需要长驻 Agent/并行的情况下，再讨论对象与资源生命周期、任务调度和原子语义；不提前推入本轮最小修复。

## 8. 能力缺陷与优先级清单

| 优先级 | 缺陷 | 性质 | 应由谁完成 |
|---|---|---|---|
| P0 | 无 stdio/stdin 可读端点 | 真正缺失的 OS 能力 | Host `Resource` adapter |
| P0 | `io.写文本` 固定一页、未检查短写/错误 | 标准库正确性 | stdlib/io |
| P0 | `stdlib` 输入、read_line/read_exact/write_all 缺失 | 标准库产品缺口 | stdlib/io |
| P0 | stdlib 测试夹具与公共 API 混淆 | 工程产品化 | stdlib + tests |
| P0 | 架构状态文档相互矛盾 | 文档治理 | docs/CI |
| P1 | `std:fs` 文件工具缺失 | 标准库产品缺口 | stdlib/fs |
| P1 | 文件 read 预读、write 关闭提交、8 MiB 上限 | 当前低层契约能力限制 | Host Resource + 安全审查 |
| P1 | 缺文件 stat/list/rename/remove/mkdir/append/seek | 新 OS 操作需要平台端点 | Resource Operation + stdlib |
| P1 | JSON/CSV/UTF-8 decode 主要停留在 tests | API 发布缺口 | stdlib/data,text |
| P1 | 大型 `host.rs`/`commands.rs` 的职责混杂 | 架构债 | Rust 模块重组 |
| P1 | 多协议注册/安全/追踪概念交叉 | 架构边界问题 | Provider/Plugin Registry |
| P2 | HTTP 解析基于字符，非通用字节流 | 语义与正确性限制 | stdlib/net |
| P2 | TCP 缺 DNS/TLS/监听/UDP | 平台端点能力缺失 | Provider/OS adapter |
| P2 | 标准库缺 path、regex、datetime、env、process、安全随机、压缩等 | 依赖具体 OS/算法的混合缺口 | stdlib 优先；必要时端点 |
| P2 | 当前每区域16MiB、run 64MiB 的限制 | 资源预算设置限制 | Runtime/CLI 配额策略 |
| P3 | 无并发任务、原子同步、可取消的长驻应用调度 | 高级执行语义缺口 | Runtime，另作 ADR |
| P3 | 无 native 优化器、表格高效后端 | 性能能力缺口 | 依据 benchmark 决定 |
| P3 | 多平台严格隔离与 CI 证据有限 | 验证/发布缺口 | CI/Sandbox |

## 9. 计算闭包验收：防止又变成 Rust Builtin 项目

必须区分四种状态：

- `S0` 原语可表达：语义核心拥有必需的计算/系统边界；
- `S1` 证明可写：测试文档写出过一个算法；
- `S2` 标准库已发布：用户可通过 `std:` 稳定导入；
- `S3` 工程可用：正确性、边界错误、权限、性能预算、文档、版本契约得到验收。

下表要求只在冻结 CLI 之上改 `.mq.md`（涉及新增 OS 端点的用例须先完成对应底层适配器）：

| 能力 | 核心闭包验收 | 标准库验收 | 防作弊条件 |
|---|---|---|---|
| 内存分配 | bump / free-list / splitting / coalescing | `std:memory/alloc` | 不加 Rust allocator builtin |
| 字节编码 | UTF-8 / endian / LEB128 | `std:text/utf8` | 不调用 Foreign |
| 数据格式 | JSON/CSV roundtrip、错误位置 | `std:data/json`/`csv` | 不增加 JSON/CSV HostOp |
| 表格 | filter/sort/join/group/GFM | `std:data/table` | 不加表格算法 Builtin |
| 输入输出 | stdin→read_line→stdout；多行 EOF | `std:io` | 只有底层字节读写由 Host 实现 |
| 文件 | read/copy/write/metadata/rename | `std:fs` | 新增 OS 端点须有精确授权，不加 `copy_file` Rust Builtin |
| HTTP | 解析分块/内容长度/UTF-8 body | `std:net/http1` | 不调用 Python HTTP 包 |
| 独立更新 | 仅替换 stdlib bundle 和摘要 | frozen-binary 测试 | CLI 二进制哈希不变 |

**完成门槛**：不允许只在 `tests/conformance/semantics` 放一个 demo 就关闭 stdlib 的待办事项；测试必须从用户导入路径走完整执行流。

## 10. 重构的安全与兼容性验收

1. **纯模式**：任何文件或网络端点无授予则拒绝；控制台 stdin/stderr 是否允许由明确的新 ADR 决定，不能无意突破既有严格模式契约。
2. **项目模式**：每种文件元数据/写入/删除操作单独授权；绝不能复用普通 read 授权执行目录删除；路径规范化防越界、符号链接竞态需要攻击性测试。
3. **严格模式**：目前只明确支持受限 stdout，stdin、file、network、Provider 必须在隔离证明达到要求后才能逐个开放；拒绝时可诊断，不可静默降级。
4. **资源句柄**：跨 run、伪造、关闭后使用均拒绝；标准库 `close_checked` 保证正常路径不遗漏 close；异常致命退出必须由 Host 在 run 结束时清理资源。
5. **短读短写**：所有 `read/write` 返回实际字节数，标准库循环并在零进度时终止；不得认为一次调用即完整成功。
6. **字节与 Unicode**：二进制 I/O 不隐式调用 `文本()`，协议长度以字节计算；UTF-8 解码错误返回有类型的 `Result`。
7. **兼容**：`打印` 输出文本契约、`std:io` 名称、Result 结构和 `Resource` 权限语义不改变；若要调整已有接口，以兼容门面和重大版本演进。
8. **测试**：最基本验收建议为 `cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --locked`、stdlib digest tests、frozen CLI tests、对应 strict VM 安全矩阵；**这些是目标命令，不代表本审查执行了它们**。
9. **发布文档**：CI 的成功/失败、commit SHA、环境、精确测试命令由自动报告生成；不能用旧审查快照声称新提交已过关。

## 11. 禁止事项：防止再次架构偏移

- 不创建独立的 `marqdo-stdlib` 仓库，除非用户以后决定改变已接受的 ADR-0039。
- 不为 `input()`、`read_line()`、`file_copy()`、JSON、CSV、表格排序分别新增 Rust Builtin。
- 不把标准库失败伪装成成功；不能忽略 `resource.write(...).ok` 与短写。
- 不为了“统一”把普通 `List/Map/Table` 直接改成可共享可变裸内存，破坏既有值语义。
- 不将 Callable Plugin、Resource Provider、Foreign 和 Tool 合并成一个不可审查的万能入口。
- 不在 `strict` 中默许新 native provider，或将 worker 进程隔离称作操作系统沙盒。
- 不因为小 crate 数量多而一次性合并十几个 crate；先识别依赖方向和职责。
- 不再把一次性测试示例宣称成已经交付的标准库产品。
- 不用单纯文本/Unicode 码点算法假装处理 HTTP 的二进制长度和文件协议。
- 不在第一阶段建设无需求支持的 GPU/QPU、JIT、裸机器码或复杂任务调度框架。

## 12. 按文件的最先修改计划（建议前两个 PR）

### PR-1：Correct IO / stdin First

- `crates/marqdo-host/src/resource.rs`：扩展 Stdin、Stderr stream 变体和读写权限；可拆 `resource/stdio.rs`。
- `crates/marqdo-host/src/host.rs`：使 stdio 资源发现和授权支持新端点；保持严格模式 fail-closed。
- `crates/cli/src/commands.rs`：确认真实 stdin 传入 Runtime Session 的路径与 CLI JSON 输出互不污染。
- `stdlib/io.mq.md`：改为兼容门面，提供正确的 write_all + input line API。
- 新增 `stdlib/io/stream.mq.md`、`stdlib/io/console.mq.md`、`stdlib/text/utf8.mq.md`。
- `crates/cli/tests/resource_streams.rs`：新增 stdin/EOF/partial 语义与 stdout/stderr 区分测试。
- 新增 `tests/e2e/interactive-cli`：冻结 CLI、不使用 foreign/plugin。
- 更新 `spec/evaluation.md` 和 security ADR；更新 stdlib digest。

### PR-2：Stdlib Productization + CLI Module Boundaries

- 新增 `stdlib/fs/file.mq.md`、`stdlib/data/json.mq.md`、`stdlib/data/csv.mq.md` 和公共 API 文档。
- 将已验证的 conformance 示例改造为标准库测试，保留原有语义夹具作为后向回归。
- 将 `crates/cli/src/commands.rs` 分为 `commands/`，`loader.rs/stdlib.rs` 归入 project 模块；不改变命令行接口。
- 将 `crates/marqdo-host/src/host.rs` 的资源操作迁至 `resource/*`，将多 Provider 注册迁至 `providers/*`；对 ABI/Capability 策略固定回归。
- 新增 CI 的 `stdlib-import-only`、`no-new-builtin-for-algorithm` 和文档 SHA 一致性门禁。

文件重排应遵循“先行为保护、后内部抽取、最后删冗余”的顺序。不要让大规模结构性 diff 与系统 I/O 新功能混在同一个 PR 里；若严格实施，应把 PR-2 再拆成 stdlib 产品化与纯 Rust 文件重排两个独立变更。

## 13. 与之前 RFC-CM-0001 / RFC-CM-0002 的关系

- 继续沿用 `Managed Value + Addressable Memory Region + Capability Resource`，**不再重启机器码或门电路讨论**。
- 已有 `Memory`、`Resource`、`print` 迁移和表格算法是真实进展，不需要重复开发。
- RFC-CM-0002 中“标准库独立仓库”的早期预设已经被 ADR-0039 改为**同仓库标准库**；应修改旧 RFC 顶部的状态注释并链接 ADR-0039，而不是保留两种相反的实施指导。
- 以前以“证明计算闭包”为主；本次以“**闭包已证明的能力能否成为可靠的、可发行的用户 API**”为中心。

## 14. 审查来源（冻结到同一 commit）

以下路径均对应 `7fbe069196e61d06a00d64fe8d65049c327373eb`：

- [Cargo workspace](https://github.com/cflmy/marqdo-cli/blob/7fbe069196e61d06a00d64fe8d65049c327373eb/Cargo.toml)
- [CLI commands](https://github.com/cflmy/marqdo-cli/blob/7fbe069196e61d06a00d64fe8d65049c327373eb/crates/cli/src/commands.rs)
- [CLI project/print loader](https://github.com/cflmy/marqdo-cli/blob/7fbe069196e61d06a00d64fe8d65049c327373eb/crates/cli/src/loader.rs)
- [Stdlib resolver](https://github.com/cflmy/marqdo-cli/blob/7fbe069196e61d06a00d64fe8d65049c327373eb/crates/cli/src/stdlib.rs)
- [Runtime memory](https://github.com/cflmy/marqdo-cli/blob/7fbe069196e61d06a00d64fe8d65049c327373eb/crates/marqdo-runtime/src/memory.rs)
- [Builtins](https://github.com/cflmy/marqdo-cli/blob/7fbe069196e61d06a00d64fe8d65049c327373eb/crates/marqdo-ir/src/builtins.rs)
- [Host dispatch](https://github.com/cflmy/marqdo-cli/blob/7fbe069196e61d06a00d64fe8d65049c327373eb/crates/marqdo-host/src/host.rs)
- [Resource table](https://github.com/cflmy/marqdo-cli/blob/7fbe069196e61d06a00d64fe8d65049c327373eb/crates/marqdo-host/src/resource.rs)
- [Standard library IO](https://github.com/cflmy/marqdo-cli/blob/7fbe069196e61d06a00d64fe8d65049c327373eb/stdlib/io.mq.md)
- [Standard library HTTP](https://github.com/cflmy/marqdo-cli/blob/7fbe069196e61d06a00d64fe8d65049c327373eb/stdlib/net/http.mq.md)
- [Standard library Table](https://github.com/cflmy/marqdo-cli/blob/7fbe069196e61d06a00d64fe8d65049c327373eb/stdlib/data/table.mq.md)
- [Evaluation semantics](https://github.com/cflmy/marqdo-cli/blob/7fbe069196e61d06a00d64fe8d65049c327373eb/spec/evaluation.md)
- [Stdlib deferred split ADR](https://github.com/cflmy/marqdo-cli/blob/7fbe069196e61d06a00d64fe8d65049c327373eb/docs/adr/0039-stdlib-extraction-intent.md)
- [Architecture index](https://github.com/cflmy/marqdo-cli/blob/7fbe069196e61d06a00d64fe8d65049c327373eb/docs/architecture/README.md)
- [Completion audit](https://github.com/cflmy/marqdo-cli/blob/7fbe069196e61d06a00d64fe8d65049c327373eb/docs/evidence/2026-11-20-rfc-cm-completion-audit.md)

---

**最终设计判断**：核心三域（托管值、可寻址内存区域、受控系统资源）是合理的；现在必须把“资源端点完整性、标准库产品化、CLI/Host 职责收敛”放到第一优先级。不要再为了扩展某个普通函数修改 Rust Builtin；但也不能假装 stdin、目录删除、TLS、系统时钟和熵源在没有 OS 适配器时能够自动涌现。
