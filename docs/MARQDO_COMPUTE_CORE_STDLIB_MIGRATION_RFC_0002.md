# RFC-CM-0002：Marqdo 最小计算内核、标准库自举与系统能力迁移

> **Design input / Proposed.** This RFC records target semantics, not
> implemented behavior. The implemented first slice is described in
> [`architecture/CURRENT.md`](architecture/CURRENT.md).

> **Syntax note — RFC-LANG-0002 is the current language contract.** The
> compute-kernel / stdlib-ownership / resource model in this RFC is still the
> current design input. Its `.mq.md` examples and Markdown-role wording were
> migrated to the single current syntax
> ([RFC-LANG-0002](architecture/RFC-LANG-0002-unified-semantics.md),
> [ADR-0052](adr/0052-unified-markdown-semantics.md)): `## @exec` heading
> modes, `params`, `return`, `if/else/end`, `while/end`, and one-statement
> payloads carried by `Strong`, `Emph` or inline `Code`. Any remaining old
> spelling (for example "`##/**/*` roles") is historical and is **not** an
> active language. The frontend now implements this syntax; examples that use
> APIs this RFC only proposes remain design targets, not implemented features.
> See [`STATUS.md`](STATUS.md).


> **状态**：Draft / 架构与迁移提案，未批准、未实施。所有代码均为目标语义草案，不能当作已运行示例。  
> **日期**：2026-11-20  
> **依赖与基线**：在《RFC-CM-0001：Marqdo 自主计算机器、可寻址内存与系统交互边界》之上细化，不代替其中的多内存区域与资源边界原则。  
> **源码核对基线**：`cflmy/marqdo-cli`，`a113d894962d8c00eb02efb42e0e73f8b084a1fa`。后续实现前必须重新确认 HEAD 和工作分支差异。  
> **建议路径**：`docs/architecture/RFC-CM-0002-computational-closure-and-stdlib-migration.md`  
> **约定**：文中的“必须”指**提议批准本 RFC 后**的实现约束，不表明仓库已有此能力；“候选接口”不是已冻结的语法/API。

---

## 0. 执行摘要与最重要的结论

本 RFC 不主张“将 Rust 代码机械地翻译为 Marqdo”，而是要**改变计算功能的所有权**：

1. Marqdo Runtime 只承担**不能由自身程序构造的计算和存储机制**：基础值运算、结构访问、通用可赋值位置、有身份的内存区域、字节读写/增长、运行时安全检查、函数执行与受限资源调用。
2. 普通数据结构、缓冲区、分配器、二进制编解码、文本格式化、GFM 表格变换、标准输入输出包装、文件协议与应用算法在 `.mq.md` 标准库中实现。
3. “打印”等高级操作**不再是不可替换的 Host 特例**；其格式化、UTF-8、换行、部分写循环等属于 Marqdo。最终将字节送出 stdout 的动作仍属于操作系统边界。
4. GFM Table 不等于一段裸字节：**Markdown 表格的源码结构/可读性保留，Table 的逻辑值语义固定，底层存储可替换或选择列式/区域视图**；表格常规算法归入标准库。
5. 扩展功能不应要求经常修改 `HostOp`。资源访问使用**版本化、可发现、带授权的低层 Provider 契约**；只能以现有 ABI/CallableRegistry 为基础增量演进，绝不并列制造第二套影子插件系统。
6. “不依赖宿主语言”的精确定义：开发者仅修改 `.mq.md` 就能扩展**有共同底层原语支持的算法与系统高级功能**；操作系统新原始能力、设备驱动、新硬件指令以及部分性能优化依然需要平台适配器或编译后端。运行时由 Rust 实现，不等同于算法外包给 Rust。
7. 必须以**计算闭包验证**验收：将标准库高级 Builtin/Foreign Kernel 禁用，不增加 Rust/ABI 实现，纯 Marqdo 仍可完成指定完整应用。否则语言内核尚不充分。

**核心决策建议：** `Managed Value + Addressable Region + Capability Resource` 三领域；**只有语义不能自行实现，才有资格进入语言内核**。但“能用低级指令极慢地模拟”不等于拥有实用语言基础；基本的**有界数据结构构造/遍历、错误恢复和高效字节接口**必须被证明可用。

### 一页式职责分离

```text
普通 .mq.md 应用 / Agent / 科研程序
    │
    ├── Marqdo 标准库 (.mq.md 为主) ─────────────────────────────────┐
    │   print, read_line, format, utf8, table, json, http, allocator ... │
    │                                                                │
    ▼                                                                ▼
Marqdo 语义核心                                                   资源调用边界
  AST→IR、值、函数、控制流                                           capability + provider
  位置赋值 / 内存区域 / 有限集合基础                                    stdio / fs / net / device
  错误恢复 / 有限类型与遍历                                                 │
    │                                                                     ▼
    └─────────────── CPU 解释器/未来独立编译器 ───────────────────► OS / Device

源码 Markdown 链接/GFM Table/标题属于文档层；不是通用堆地址和系统调用指令。
```

---

## 1. 设计目标、拒绝的捷径与不变量

### 1.1 目标

- **G1 自主计算**：不需要 Python/Node/Foreign、为算法专写 Rust Builtin、为每个容器专写 HostOp，就能用 `.mq.md` 完成数据结构和计算算法。
- **G2 系统构造性**：只要基础 OS 资源端点已存在并获授权，Marqdo 应自行实现 `print/read_line/copy_file/HTTP parser/CLI commands` 等高层 API。
- **G3 内核稳定**：未来 100 个独立标准库函数，不意味着增加 100 个内置函数或触及 Rust 编译器/解释器。
- **G4 语义统一**：高级函数运行在与普通程序**同一个**值、内存、调用、错误、来源映射和权限体系下，没有“标准库专用魔法规则”。
- **G5 兼容当前 Markdown 到程序的映射**：按 RFC-LANG-0002，执行由 `@exec` 模式与载荷关键字决定；链接/表格保持惰性；函数与模块寻址、执行顺序不被重写。
- **G6 工程品质**：至少能构造可扩展容器、网络字节协议、异构表格、持续交互工具，同时明确性能瓶颈和安全保证。
- **G7 明确安全边界**：内部虚拟内存权限与资源访问权限分别验证；不能把“运行在 Rust 内”误写为“自动内存安全”，也不能把普通 Worker 等同于 OS 隔离。

### 1.2 明确不做

- 不新增面向业务的 Markdown 关键字，例如 `PRINT/ALLOC/HTTP/TENSOR/GPU`。
- 不以“任何 Rust 实现都是语法糖”为判断依据；整数、浮点、GC/容器表示、文件描述符管理当然必须有底层实现。
- 不将 GFM Table 直接等价于物理 RAM、也不让编辑源码的表格自动修改运行时堆。
- 不为与 OS 交互提供无限制的 `syscall(number, raw_address)` 默认后门。
- 不强行让已有 `Value::List`、`Value::Map`、`Value::Table` 全部搬到线性内存，避免将高级常用值退化为只能手工处理的裸字节。
- 不假定提供读写内存就能自然获得原子同步、时钟、密码学安全随机数、TLS、GPU 或 QPU。
- 不先开发自己的 JIT、CPU 寄存器、异步事件循环或巨型调度框架。

### 1.3 跨整个迁移期都必须成立的不变量

| ID | 不变量 | 解释 |
|---|---|---|
| INV-01 | Markdown AST 是结构语义的唯一权威 | 不在文本扫描中偷偷执行表格、链接、代码围栏 |
| INV-02 | 普通数值和 List/Map/Table 原有可观察语义不变 | 别名行为变化需要新的显式抽象/ADR |
| INV-03 | Memory 引用有身份，数字偏移不具有全局权限 | 不能用整数伪造另一区域地址 |
| INV-04 | 任何虚拟内存访问均在有效区域内 | 大整数转换、地址加法、长度计算须检查溢出 |
| INV-05 | 函数与标准库共享同一解释执行机制 | 不为标准库增加隐藏的 Rust 快捷调度 |
| INV-06 | 纯计算不会自动获得 OS 权限 | 安装依赖、调用函数、拥有路径字符串都不构成授权 |
| INV-07 | 操作系统交互必须能被识别为外部效应 | 权限、审计、失败、部分读写均可见 |
| INV-08 | Resource/Memory 句柄不能从不可信 JSON 伪造 | 不可伪造身份、generation、所有权/权限检查 |
| INV-09 | 输入/输出数据不隐式混入诊断通道 | stdout、stderr、JSON/trace 按原规范保持隔离 |
| INV-10 | 不承诺未实现的安全或硬件能力 | strict/worker/设备/并行都按实际支持矩阵陈述 |
| INV-11 | 高层功能可在独立 stdlib 迭代 | 不修改 CLI 源码、无需重新链接 Rust 运行时 |
| INV-12 | 被替换的内置功能必须有兼容性黄金测试 | 不接受“通常一样”或仅观察几条 happy path |

---

## 2. 现有仓库审查：应该保留什么、必须改变什么

本节来自固定提交 `a113d894...` 的实际代码，**不是本 RFC 的拟议能力**。

| 源码/规范 | 当前事实 | 架构影响 |
|---|---|---|
| `crates/marqdo-value/src/value.rs` | `Value` 含 `Unit/None/Bool/Int/Float/Text/List/Map/Table/Formula/Code/FunctionRef/PluginHandle`，GFM `Table { header: Vec<String>, rows: Vec<Vec<Value>> }` | 缺通用可寻址字节区域；Table 强绑定具体 Rust 行存储 |
| `crates/marqdo-expression/src/ast.rs` | `StatementKind::Assign` 左侧只有 identifier；成员/索引目前可读 | 内存区域写入需要 `Place` 目标语义 |
| `crates/marqdo-runtime/src/interpreter.rs` | `Frame.locals` 存 `BTreeMap<String, Value>`；赋值重绑定本地槽 | 保持现有值语义；Memory 另有引用身份 |
| `crates/marqdo-runtime/src/builtins_impl.rs` | `打印` 转发 `HostOp::Print`；`table()` 在 Rust 校验 headers/rows；`文本(str)` **仅接受 Text/Int/Float/Bool/None/Unit** | `打印` 迁移前不具备由 Marqdo 泛化格式化全部 Value 的必要观察接口 |
| `crates/marqdo-host/src/host.rs` | `op_print` 经 `Value::display()` 格式化，追加换行、检查 console grant 和 output budget、写入 Rust 输出缓冲区 | 迁移必须把“值格式化”和“真正字节输出”拆开；不能丢掉权限及预算 |
| `crates/marqdo-host/src/capability.rs` | `CapabilityBroker::execute` 只对部分能力提供真实执行，其他可返回 unavailable | 不能把“接口已声明”当作“端点已实现” |
| `spec/evaluation.md` | Builtin 的稳定名字包括 `打印/print`、`长度`、`类型`、`文本`、`整数`、`小数`、`表`、`ref`、math/foreign/fs | 需要明确公开 API 兼容表及迁移阶段 |
| `spec/mapping.md` | GFM Table cells 惰性；Markdown 链接纯引用、不会因为渲染执行 | 不得让表格直接成为低层命令/指针操作 |
| `spec/security.md` | `pure/project/strict` 分层；Linux strict L1 有探测与限定支持，其他情况 fail closed；console 输出特殊允许 | 新 I/O 端点不能自动在 strict 里开放；旧 stdout 行为要保留 |
| `spec/plugin-abi.md` | C ABI v1、worker、capability request、签发的 PluginHandle | 先评估复用而不是另造 ABI；普通子进程不是安全沙箱 |
| `spec/package-format.md` | path/Git+revision、lock v3；Manifest 中部分 runtime 范围约束尚未真正强制 | stdlib 分仓与离线分发要补上可验证版本契约 |

当前存在一个**容易被忽视的闭包缺口**：`Value::display()` 在 Rust 中能打印列表/表格等复杂值，但现有 Marqdo 的 `文本()` 不能转换这些值，且 Table 没有完整的公开通用遍历/构造 API。**直接删掉 `HostOp::Print` 并在 `.mq.md` 写 `打印` 是做不到无损迁移的。** 必须先补足对所有值类型的受控观察与组合能力。

### 2.1 迁移工作的实际代码冲击面

```text
语言与解析：crates/marqdo-expression, crates/marqdo-frontend, crates/marqdo-ir
值与执行：crates/marqdo-value, crates/marqdo-runtime
系统边界：crates/marqdo-host, crates/marqdo-sandbox, 当前 Plugin ABI
包分发：  crates/marqdo-package, crates/cli
文档规范：spec/evaluation.md, spec/expression.ebnf,
         spec/mapping.md, spec/references.md, spec/security.md, spec/value-wire.md
回归：    AST goldens, conformance, CLI, platform, security, lockfile, workload
```

除非某个需求确实突破了现有抽象，不能因为“准备标准库迁移”就在上述每个 crate 中引入一套独立规则。

---

## 3. 不可再分解的计算内核：**不是最少函数数量，而是最小充分语义**

### 3.1 三个语义领域

**Managed Value**：`Int/Float/Bool/Text/List/Map/Table/...` 的可观察值语义，由 Runtime 管理存储，用户不能据此直接获得宿主进程地址。`Text` 是 Unicode 字符序列语义，不与 UTF-8 字节数组混淆。

**Addressable Region**：多个不同身份、独立限额、可增长、按字节索引的存储区域；句柄复制表示共享同一块区域，偏移是区域内部的整数。用户可以在其中构造分配器、数据结构和自定义二进制布局。

**Capability Resource**：由可信环境授予的不透明外部对象，例如 stdin/stdout、文件目录权限、连接、设备。其 read/write 可能涉及不可重放的外部效应，不等同于普通 Memory。

此三分法刻意不包括“真实 CPU 寄存器”。局部变量/临时值已经是抽象执行槽；硬件寄存器分配属于编译器后端。GPU/QPU 硬件内存属于今后的有约束资源或后端，不作为普通可任意读写的 Region 伪装。

### 3.2 计算闭包：内核应提供的不可省略能力

| 分类 | 最小语言机制 | 为什么不可简单只靠库补齐 | 归属 |
|---|---|---|---|
| 控制 | 函数、参数、调用、条件、循环、递归、返回 | 语言自身执行与调用语义 | 维持 Core |
| 数值 | Bool、精确 Int、Float、基本算术/比较 | 构成一般计算和地址运算 | Core；位操作优先在库实现/测成本再定原语 |
| 变量/位置 | 普通绑定重置、索引读写、必要时字段路径写入 | 无位置写入则共享存储不可编程 | Core，少量 grammar 扩展 |
| 动态存储 | 区域创建、大小、增长、有界字节读写 | 程序不能从自身获得新内存；必须由 Runtime 执行 | Core，**非 HostOp** |
| 通用值构造 | 可动态构造、访问、增补 List/Map/Text/表格结构 | 若能读不能构造，`format/table/json` 无法独立实现 | Core 的结构原语 + Stdlib 算法 |
| 可观察类型 | 类型判定/长度/枚举结构成员/安全读取元数据 | 没有足够反射就无法由 Marqdo 格式化全部 Value | 极小结构契约 |
| 错误控制 | 可识别、可传播、可恢复的错误结果 | I/O 和解析器正常失败不能都触发进程终止 | Core 可恢复结果语义（形态需 ADR） |
| 外部资源 | 资源获取、带缓冲区读写、关闭、结构化错误 | 用户态内存不能凭空实现 OS 系统调用 | 受权系统边界 |

**特别说明：** “无位置赋值也可以用不可变列表的递归重建模拟”在理论上成立，但现有 Marqdo 缺乏动态 List 构造的普遍闭环。不能用这种理论可能性掩盖实际标准库无法写出的事实。计算内核是否充分，应以用户语言的**有效构造能力**为判定标准。

### 3.3 内存区域规范（继承 RFC-0001，补充细则）

`MemoryHandle = (run_id, region_id, generation)` 为运行时创建、不可被用户伪造的句柄；句柄的字节表示不属于用户可观察 API。`Memory` 对外可按索引读写非负偏移的字节。不同区域互不别名；同一区域的句柄复制与传参保留身份。区域内容和长度由 Runtime 内部管理，逻辑偏移不是 Rust 指针。

**拟议接口（不是新语法关键字）：**

```text
memory.new(initial_pages, max_pages) -> Memory
memory.size(memory) -> page_count
memory.grow(memory, pages) -> old_page_count or recoverable error
memory[offset] -> Int(0..255)
memory[offset] = Int(0..255)
```

- 初始页为零填充；推荐先采用 64 KiB 逻辑页，但是否冻结为规范常量须 ADR；`memory.size` 和 `grow` 不能出现页/字节混用。
- `grow` 只能增加；失败时原区域状态不变；旧偏移继续有效；请求超过单区域/总内存预算必须失败。
- 巨大 BigInt、负偏移、非法字节、无效/失效句柄都必须是稳定、可定位的错误，不得 panic。
- 分配块 `malloc/free` 属于标准库，管理的是**区域内部子区间**；不等于调用方可以释放 Runtime 的整个区域。区域生存期首版由运行/引用可达性管理，至少执行结束必定释放。
- `m[i] = x` 必须检查赋值左侧，不能让 `list[i] = x` 在未定义 List 语义时静默地变为共享可变对象。
- 所有资源限额应可测量：区域总数、已提交字节、逻辑最大容量、复制/增长预算、运行时间和 fuel；内存分配失败不得误报为成功。

### 3.4 Place/索引写入：最小 AST 变化

现有 `assignment = identifier '=' expression` 扩展为候选形式：

```ebnf
assignment = assign_target, "=", expression ;
assign_target = identifier, { "[", expression, "]" | ".", identifier } ;
```

上式是**允许将来统一的语法形态**；首期语义仅接受 `Local(name)` 和 `Index(memory,index)`。对于尚未定义的目标（如 Map 成员更新、嵌套 List 更新），必须给出明确诊断，不能悄悄套用 Rust 的可变引用规则。

赋值左侧先求 base，再求 index，再求 RHS（精确求值顺序须写入 `spec/evaluation.md`）；若参数检查不通过，操作不提交。`Memory` 区域更新是共享的；普通 `List/Map/Table` 仍按既定值语义处理。

### 3.5 一项必须补齐的核心能力：动态复合值构造与结构观察

只有按字节的虚拟内存，的确可以**从零编码任何数据结构**；但如果普通 `.mq.md` 程序无法动态构造新的 List、Map 或遍历 Table，就会迫使所有高级库把 Unicode 字符串和异构数据都退化为自定义字节内存布局，工程上反而不可接受。

因此建议在**同一套值访问语义**下补齐最少的结构操作，而非给每种业务对象加新的类型：

- `List`：访问长度、按序读取、动态构造/追加、明确的值更新（可用返回新列表的操作实现）；
- `Map`：查询、枚举键、创建/关联/删除键，不暴露 Rust `BTreeMap` 接口作为唯一调用方式；
- `Text`：长度、Unicode 标量遍历、拼接；UTF-8 编码/解码可在 `.mq.md` 实现，但应允许通过 Memory 生成字节；
- `Table`：公开列名、行数、行/单元格读取的逻辑接口；如有变化操作，遵守值语义；
- 特殊文档值 `Formula/Code/FunctionRef`：只暴露合法的**可观察元信息**，不能未经授权展开私密资源内容；
- `Memory/Resource`：只暴露类别、有限状态及受权操作，**不提供完整原始地址或可伪造 ID**。

这些可以作为**少量统一的结构原语**，其对外名称由 ADR 设计；`collections/` 标准库仍负责排序、过滤、连接、分组、图遍历、表格变换等真正的算法。新增一个标准库的 `table.join` 不应触发新的内置操作。

**判据**：构造 JSON parser/formatter、GFM Table 变换、任意尺寸 List、泛型 `print` 时，不能发现“在 Rust 里可以观察/创建，Marqdo 里完全做不到”。若做不到，就说明计算核心缺乏**通用结构观察或构造机制**。

### 3.6 可恢复错误：别让标准库只能让进程中止

传统错误如 `unknown symbol`、程序越界可当语言诊断；但文件不存在、网络被关闭、UTF-8 非法序列、解析失败、存储不足等往往是**可预期的运行时条件**，用户必须能够在 Marqdo 中判断和恢复。

推荐在语言中支持标准的 `Result` 数据契约（可以用现有 Map/Record 值编码；不必增加 `try/catch` 关键字），区分：

- `Ok(value)` / `Err(code, message, metadata)`：业务可恢复错误，函数直接返回并让上层用普通分支处理；
- 违反语言不变量/用户错误或预算超限：保留稳定诊断，必要时停止当前执行；
- 外部资源可能的 `EOF / partial / pending / canceled`：须显式区分，不把 EOF 当成临时无数据或错误。

错误格式、传播约定及能否从 `memory.grow` 失败恢复须用 ADR 冻结。优先用现有 `Map`、函数和条件组合，不要默认增加新异常语法。

---
## 4. 系统交互不是“宿主函数集合”，而是稳定的资源操作边界

### 4.1 一次性澄清“彻底不依赖 Rust”的目标

**可以达到**：`print`、`input`、`read_line`、UTF-8、CSV/JSON、排序、哈希表、文件复制、HTTP/1.1 解析等能力由 `.mq.md` 组合实现，并能用**未重新编译**的 CLI 执行。其核心算法不得依赖 Rust 提供专门的对应 API。

**有前提地达到**：网络客户端、进程管理、目录遍历、定时操作等：当**已授权且已有的资源提供者**暴露了相应的不可再分操作，Marqdo 可自行组合高级功能。若目标环境没有 `socket connect`、`directory enumerate` 或时钟/熵源，任意次数内存 load/store 都不能凭空获得这些能力。

**不可能按字面实现**：从虚拟内存中的字节计算“推导”出所有操作系统/硬件功能，或在所有平台上不经机器指令、系统 ABI 与设备驱动就完成实际 I/O。即使未来自举成本机二进制，系统边界仍然存在，只是不一定经过 Rust 解释器。

**本 RFC 的真实目标不是“没有 ABI”，而是“**`ABI` 的扩张速率与标准库功能数量脱钩**”**。普通函数增长只增加 `.mq.md`；新增真正的底层设备能力最多增加独立、可注册的端点实现，而不污染语言语法、值计算和普通模块解析。

### 4.2 资源不是 Memory，设备寄存器也不是普通 byte[]

执行状态中保留 `ResourceTable`，其句柄由运行时/可信 Provider 签发，带实例、generation、实际权限和关闭状态。资源表条目必须覆盖：资源类别、Provider 身份、协议版本、已授予操作、生命周期、预算绑定、审计上下文。

逻辑上仅需要三种动作：**取得已授权资源 → 调用其承诺的操作 → 释放资源**。实际可支持的操作由该资源实现的**版本化契约**列举；不能把 `resource.call(handle, "anything", bytes)` 当成权限绕过的万能后门。

通用流类资源的契约可描述为：

```text
Capability.lookup(name, request) -> GrantedResource | Denied | Unavailable
Stream.read(handle, memory, offset, length) -> {count, eof} | Error
Stream.write(handle, memory, offset, length) -> {count} | Error
Resource.close(handle) -> Ok | Error
```

**这些是接口契约，不要求 `Stream`、`Resource` 成为语言关键字。** 按兼容策略，可以用普通模块函数包装有权限的系统端点。Provider 通过已存在的 callable 解析和受控注册进入程序；具体底层调用不通过用户伪造字符串/句柄获得新权限。

- 读：`count` 可为 `0..length`，`eof` 与 `count=0` 不必等价；非阻塞资源可报告 `would_block/pending`。
- 写：可能仅写入前缀，调用者循环处理余量；中途错误可能发生在**已经完成部分外部效应**之后，不能假装事务回滚。
- Memory 是传输缓冲区，但 Provider 只能在**本次调用期间**借用受验证的缓冲区，不能持有可随时被 `grow` 搬迁的宿主地址；首版可采用安全复制以降低复杂度。
- 关闭后 generation 必须失效；活动资源的复制/传参规则明确为共享句柄身份，但“谁负责 close”应定义为资源所有权规则，不能被运行时 `Value::clone` 意外复制底层 OS fd。
- 设备专有接口可注册独立的命名协议（如 `device.control.v1`），但不应伪装为可随意解引用的内存。MMIO、DMA、QPU 需要专门的权限和排序/一致性约束。
- 基本 stdout/stdin/stderr 不是任意可构造路径，须由运行环境作为授予的预打开资源显式提供；防止应用拿整数 1 就伪造 stdout 权能。

### 4.3 避免“只有 open/read/write 就宣称一切系统能力具备”的错误

资源类的基本接口是**模式**，不是所有系统能力的完整清单：

| 外部原始能力 | 可否仅从已授权字节流组合？ | 语言/平台的明确边界 |
|---|---|---|
| 行输入、UTF-8、缓冲、文本打印 | **可** | 需要 stdin/stdout 流端点；解析由 Marqdo 实现 |
| 文件内二进制格式 | **可** | 需要文件流读取/写入 |
| 任意文件打开、目录列举、重命名、元数据、文件映射 | **不可完全由单一流推导** | 需要有权限的文件系统端点与对应操作 |
| TCP 报文构建、HTTP/1.1 解析 | **可** | 需要已连接 TCP 字节流；建立连接本身需网络端点 |
| DNS 解析、网络连接、监听、TLS | **不由普通文件流自然产生** | DNS/Socket/安全熵/加密、证书信任等由服务/库/端点组合；安全 TLS 不宜未经审计自行实现 |
| 时间、计时器、安全随机源 | **不可** | 必须由可信平台端点提供可信时钟/熵源 |
| 进程创建、信号、环境变量、终端属性 | **不可** | 需要明确平台能力，权限逐项授权 |
| 共享内存原子操作、内存顺序 | **普通字节读写不充分** | 并行计算语义另行 ADR，不允许在无同步下假设原子性 |
| GPU 计算、QPU 任务 | **不可** | 设备执行后端/驱动契约；Qubit 不等于普通内存指针 |

**审查规则**：开发者申请增加高级函数时，先展示它调用的既有低层端点和 `.mq.md` 算法；只有**缺失真正不可由现有原语获得的外部效应**，才允许提出平台 Provider 扩展。一个新的算法不得绕过此规则要求新的 Rust Builtin。

### 4.4 Provider 扩展：复用现有 ABI/调用解析，不再造第二个生态

当前存在 `CallableRegistry`、外部可调用描述符、C ABI v1、Worker、`PluginHandle`、CapabilityBroker。它们应被利用为**可注册端点的技术基础**。本 RFC 提议增补其资源语义和能力化调用约束，而非新建平行的 `SystemProviderRegistryV2`、`SyscallHost`、`NativeToolKernel` 三套分派。

关键设计要求：

1. Provider 发布一个**可枚举的、版本化的能力协议**，包括资源类型、参数/返回形态、需要的 grant、最大传输大小、错误码、关闭语义。
2. 导入 `.mq.md` 包不会隐式加载/授权设备 Provider；加载与授权分开。
3. 普通 Marqdo 模块调用在解析时先解析模块函数，再到受控外部描述符；对兼容保留的旧 Builtin 单独处理名称优先级。
4. 端点 ABI 不得直接跨边界传递 Rust `Vec<Value>` 的内存地址，也不能通过可伪造 JSON 生成活句柄；字节传输与 handle generation 需有明确编码与所有权。
5. 若当前 ABI 的 JSON Wire 无法高效承载 Memory→Stream 数据，应允许**向现有 ABI 版本化扩展数据缓冲区传输协议**或使用复制路径；不要求第一天零拷贝，也不允许未经审查使用裸宿主指针。
6. Native 插件只有明确可信时才能在进程内运行；Worker 隔离崩溃不等于 OS 安全沙箱；现有 strict L1 的拒绝策略不因注册了新端点而自动改变。
7. 新端点实现在适配器/Provider 仓库升级即可上线，**不需要修改表达式解析器、`Value` 主类型或普通 Runtime 调用分派**（前提是 Resource ABI 版本可兼容）。

**重要的实际限制**：新设备/OS 原语有时仍需编写 Rust/C/系统 SDK 代码；若希望连这些原语也用 `.mq.md` 实现，未来只能通过受控的原生编译/FFI 系统接口探索。那不再是“纯计算沙盒不调用外部”，且要接受平台专用、安全与可验证性的代价。

### 4.5 安全模式和 I/O 兼容规则

旧 `pure` 模式已有 console 输出例外（在当前版本中为明确有权限的 stdout/缓冲输出），不应在迁移过程中暗中撤销。目标安全行为：

| 模式 | 目标支持 | 强制约束 |
|---|---|---|
| pure | 纯计算、受资源预算的 Memory/值、兼容的已授权输出缓冲 | 不提供任意文件/网络/进程/插件权限；已有 stdout 的兼容例外单独标记 |
| project | 明确授权范围内的标准文件、控制台及经过支持矩阵列出的网络资源 | grant 与实际 Provider 操作相交；项目根目录保护，拒绝路径/域名等越权 |
| strict | 保留已验证的隔离功能 | 未验证的资源端点均 fail-closed，严禁把 project 的新功能直接当作 strict 可用 |

为了验收“print 已迁移”，不仅要打印正常，还须验证不授予 console 的权限拒绝、输出预算、trace、stderr/stdout 分离、CRLF/换行兼容、部分输出和中文 UTF-8 字节一致性。

---

## 5. 内置能力分类与迁移总表

### 5.1 分类规则（以功能性质，而不是实现语言）

- **C / Computational Core**：没有它，`.mq.md` 无法表达基本状态转换或构造相应运行时值；其语义在语言规范中冻结。
- **S / Standard Library**：仅使用 C、现有 Marqdo 模块和已授权 Resource 端点即可实现，发布独立 `.mq.md` 代码。
- **E / Environment Endpoint**：不可由程序纯计算产生的外部效应，交给 Resource/Provider 端点；其**适配器代码**可用 Rust/C/平台 SDK 实现。
- **O / Optional Accelerator**：语义上有可用的 Marqdo 参考实现，但经真实测量证明性能不足，才允许可替换优化实现；不得成为唯一实现。
- **D / Developer Tooling**：编译器、诊断、源码导航、代码检查和包安装工具的宿主实现，不属于运行时标准库函数，不要求迁入 `.mq.md`。

分类时必须先展示：输入/输出/错误、是否可观测外部效应、依赖哪个 C 原语、能否实现同语义 `.mq.md` 版本、性能与兼容的验收方法。

### 5.2 现有能力迁移矩阵

| 现有/相关能力 | 目标类别 | 迁移决策 | 必要条件及注意事项 |
|---|---|---|---|
| 整数/浮点/布尔运算、比较、`and/or`、函数/循环 | C | **保留** | 解释器自身计算，不外包给标准库 |
| `len/长度` | C + S | 基础集合长度观察留 C；高级 `count`/统计放 S | 内核不得泄露内部 Rust 容器作为 API |
| `type/类型` | C | 保留最小类型标识 | 避免不可信反射到真实宿主对象 |
| `str/文本` | C 基本值转换 + S 复合值格式化 | 保留现有基本值转换的兼容语义；新增标准库 `format` | 现有 `str(List/Map/Table)` **不是**可用接口，不能用它替代 `Value::display()` |
| `int/整数`, `float/小数` | C/经验证可 S 化的解析 | 数值语义和精确舍入留 C；高阶解析/format 放 S | Unicode、整数精度、NaN/Inf、负零、超大数兼容 |
| `print/打印` | S，兼容入口 | **迁至标准库**；Legacy Builtin 过渡期代理 | 必须先具备全类型格式化、UTF-8、stdout 流调用和模块自动加载 |
| `table()/表()` | 值语义 C + 表格构造/验证 S | 分阶段迁移；避免一次删除 `Value::Table` | 必须保留表头/行宽、空表、格式化与相等性的兼容语义 |
| `fs.read` | S + E | 高级文本读入迁至 `fs.read_text`；文件流打开/字节读保留 E | EOF、部分读、编码错误、安全路径和句柄关闭 |
| `fs.write` | S + E | 高级文本写入迁至 `fs.write_text`；文件流字节写保留 E | 原子文件替换语义需额外资源操作，不能仅凭流承诺 |
| `ref()` | C / 文档运行时 | **保留**，不等于内存指针 | 静态知识地址与动态 Memory 身份不同 |
| `math.parse/eval/substitute` | S + O/保留兼容端点 | 分阶段实施 `.mq.md` 可用数学表达和替换；原生路径可留优化 | 不得假定复杂符号计算很快即可迁入；以语义覆盖优先 |
| `math.simplify/differentiate` | S 算法 + 可选 O | 不应把未实现功能变成默认 Rust 承诺 | 当前部分操作标记 unsupported，迁移要以真实通过用例为准 |
| `foreign.run/call` | E / 兼容互操作 | **保留为可选能力**，不得成为默认标准库算法必需品 | pure 与 strict 权限限制仍生效 |
| `plugin.invoke` | E / 资源扩展 | 保留为兼容/受控 Provider 入口，长期统一资源契约 | 不新增逐函数 HostOp；把注册与授权分开 |
| `List` 操作、`Map` 访问 | C 基础结构 + S 算法 | 补齐构造/读取/更新的闭包；排序/filter/merge 等迁 S | 不能靠每新增算法就新增 builtin |
| GFM Table 解析/AST | D + 语言数据契约 | 保留在 Markdown parser；不能退化为 stdlib 自行猜 AST | 标准库只处理解析后逻辑 Table 的运算 |
| GFM Table 查询/关联/过滤/渲染 | S | **迁至独立库** | 使用公共 Table 结构观察与构造能力 |
| 字节缓冲区、分配器、`malloc/free` | S | **Marqdo 自己编写** | 基于 Region，提供可追踪块元数据与空间复用 |
| UTF-8/UTF-16/CSV/JSON/二进制协议 | S（必要时 O） | **Marqdo 自己编写** | 输入字节、输出字节、错误恢复、增长写入均已闭合 |
| 控制台、文件、Socket 字节流 | E | 由可信环境提供端点 | 没有这些端点不能完成物理 I/O |
| 时钟、熵、OS 进程、设备操作 | E（按需） | 独立授权 Provider | 不应伪装成通过内存/寄存器“自然涌现” |
| 编译器/包管理/`inspect`/`check`/Trace | D/C 边界 | 不强制改写为 `.mq.md` | 改工具架构需另立目标，勿混进语言核心迁移 |

### 5.3 防止重复定义：内核与 stdlib 的公开名称策略

内核描述的是**基础操作语义**，标准库通过普通可导入模块定义 `std.io.print`、`std.text.utf8_encode`、`std.table.join` 等。文档中的模块名属于例子，不要求增加命名空间语法。

现有 `print/打印` 已是稳定的 Builtin 名称且在 `eval_call` 有优先级，迁入标准库之前需要决定：

1. 新代码推荐通过 `import` 使用 `std.io.打印` / `std.io.print`（用现有导入映射与模块函数解析）。
2. 保留 `打印(...)` / `print(...)` 作为兼容别名；解析器由内置分派**转向标准库声明/函数引用**，不得继续执行旧 Rust `Value::display()` 路线。
3. 如果语法解析优先匹配 Builtin，必须调整**名称解析的兼容分派**，确保不出现同一个名称随机解析到两个实现。记录覆盖/遮蔽规则的 ADR。
4. 标准库函数必须普通可检查、可导航、可单测，执行追踪需要记录其 `.mq.md` 源 span，而不是全部伪装成黑盒 `HostOp::Print`。
5. 在完整兼容窗口关闭前，旧 Builtin 可以作为受测试的回退路径，但默认必须优先执行 stdlib，且两条路径的输出比较必须通过。

### 5.4 打印：第一个完整迁移的具体拆分

当前行为（源码事实）：`Builtin 打印`→`HostOp::Print`→`Value::display()`→追加 `\n`→输出预算→缓冲 stdout；当前 `str()` 不支持 `List/Map/Table/Formula/Code/FunctionRef/PluginHandle`。

目标行为：

```text
std.io.print(value)
   ├─ std.format.display(value)         # 按语言值语义递归格式化
   │     ├─ type / len / index / keys  # 最小结构观察（核心）
   │     └─ std.numeric.format         # 数值与特殊值的稳定字符串表示
   ├─ std.text.utf8_encode(text + "\n")
   └─ std.io.write_all(stdout_handle, bytes)
          └─ Stream.write(handle, region, offset, count) # 唯一外部动作
```

**迁移不能跳过的细节：**

- `Value::display()` 旧行为是兼容基准：Int 任意精度、浮点格式（NaN/Inf/负零）、Bool/None/Unit、Text、List、Map、Table、Formula/Code/FunctionRef/PluginHandle。需要逐类型 golden；不允许悄悄泄露 Resource/Memory 真实内部 ID。
- `std.format.display()` 在 Marqdo 中必须**真能遍历复合值**，不能偷偷调用一个隐藏的 Rust `format_anything()` 将整件事移回 Runtime。
- 对于 `Float` 的精确十进制最短往返表示，如果完整用 `.mq.md` 编写极其昂贵，允许把**格式转换所需的最小数值原语**留在 C 类，并说明它仅负责数值表示规则、并非 `print()` 本身。应首先有经规范化的可观测结果。
- `write_all` 处理短写和失败，注意重复重试可能重复输出或产生已写前缀；输出预算应在实际写入数量上计费，且不能借包装函数绕过。对 console 格式化的燃料/内存也必须计费。
- 标准输出内容与编译器日志、trace、`--json` 通道保持隔离。
- 首版可支持保持 `print` 完全一致，同时引入 `write_text` / `write_bytes` 作为标准库独立能力；用户未授权写标准输出时，仍必须拒绝。

示例（**目标 `.mq.md`，按 RFC-LANG-0002 书写、非已实现语法，`std` 对应正常模块导入**）：

````markdown
## @exec 打印

先将任何可打印值格式化为文本，再编码为 UTF-8 字节。

`显示文字 = format.display(值)`

`数据 = text.utf8_encode(显示文字 + "\n")`

`io.write_all(io.stdout(), 数据)`
````

这些 `format.display`、`text.utf8_encode` 和 `io.write_all` 必须有真实的 `.mq.md` 定义；其最终字节写入依赖已有授权 `Stream.write` 而不是新的 `HostOp::PrintJson` 等。

---
## 6. GFM 表格重构：不是降级为字节，而是从物理表示中解放语义

### 6.1 三层必须分开

当前 `Table { header: Vec<String>, rows: Vec<Vec<Value>> }` 的优点是简单、正确地把表格作为 Runtime 值；缺点是将结构和实现绑在一起。新方案要分开：

| 层 | 应维持的内容 | 允许改变的内容 |
|---|---|---|
| **GFM 源文档** | canonical Markdown AST `table/row/cell`、单元格边界、源码 span、渲染与惰性规则 | parser 内部缓存/索引实现；不能改 GFM 的含义 |
| **Table 逻辑值** | 有序表头、行的有序性、单元格值与可见类型、行宽、相等性/格式化/错误规则 | 如何查询/更新、是否支持共享持久内部结构，由 ADR 决定 |
| **Table 物理存储** | 与逻辑值一致的可观察结果 | `Vec<Vec<Value>>`、持久向量、列式数值缓冲、按需视图、Memory-backed 数值列 |

**绝不能**把 Markdown 表格的 `|` 分隔符理解为一个字节内存地址。GFM 源单元格与 Runtime `Memory` 的 byte index 不是同一类地址；纯数字列可以优化为 Memory 背后字节块，但不强迫文本/公式/结构化值也采用该布局。

### 6.2 新的 Table 对外抽象（语义，不冻结具体类型名）

建议保持 `Value::Table` 作为独立的逻辑值类型，以免破坏现有程序的 `type`、`len`、format、错误类别与 wire 结构。其公开接口允许：

```text
row_count(table)            -> Int
column_count(table)         -> Int
column_name(table, index)   -> Text
cell(table, row, col)       -> Value
set_cell(table, row, col, v)-> Table  # 产出新逻辑值；原表不被隐式修改
append_row(table, row)      -> Table
```

**注意：** 以上是逻辑操作而不是要求六个 Rust Builtin。`row_count/column_name/cell` 属于有必要的最小结构观察；`set_cell/append_row` 应优先用可用的通用 List/Map 值重建规则在 `.mq.md` 中实现。若 `Table` 的内部表示变为封闭、不可遍历的 Rust 结构，就必须在核心提供足够的公开投影操作，供标准库重建逻辑表格。

重构必须支持：

- 零行/零列（按当前规范兼容）、重复列名处理（遵照当前规则，不能随意变更）、Unicode header、行宽验证、单元格类型检查。
- 表格复制保持旧值语义：`b=a` 后逻辑上修改 `b`，不能让 `a` 在未经显式共享授权的情况下发生可见改变。
- 表格计算可用 `filter/map/join/group/sort` 等 `.mq.md` 标准库函数组合，不因为每增加一种表格操作就修改 `builtins_impl.rs`。
- 原始 Markdown 中诸如 `**公式**`、链接和反引号在单元格内仍属于**惰性文档内容**。任何“解析某单元格为可执行表达式”的行为必须在显式调用的解析函数中发生，并遵循独立安全边界。
- 必须保留源文档的可寻址单位与审计来源；Table 逻辑值的快照与原始 Markdown 文本不是同一件事，不能无授权回写源文件。

### 6.3 物理存储的演进步骤

**T0 保留 Rust `Vec<Vec<Value>>` 实现**，先让 Marqdo 程序能够通用地访问、构建、变换 Table。此时即使内部仍是 Rust 的 `Vec`，计算功能已经不再依赖新写专门算法的 Rust。

**T1 将 Table 从直接字段访问改为抽象的受控存储接口**（Runtime 内部）。为逻辑 `Table` 定义稳定的访问/构造语义；内部可用共享不可变数据结构或写时复制减少复制成本，不能改变用户可观察语义。

**T2 可选 Column Store**：只有已验证的大型数值表场景，才尝试让连续列数据存储在 Memory/Buffer 中。列必须有明确元素类型、编码、行数与缺失值规则；异构列及来源信息仍可由托管值保存。

**T3 可选 View**：对列切片、行过滤、投影提供只读逻辑视图；若存在共享可变源存储，必须明确版本/快照/引用有效性，不能无声明就让普通 `Table` 变成共享可变容器。

**否决**：为每一种表格表现形式增加 `DataFrame/TableView/ColumnarTable/GFMTable/MemoryTable` 五个并行内置类。只允许明确的逻辑 Table 值与可替换存储实现；专门优化作为标准库/后端能力。

### 6.4 表格迁移的验收样例

输入 GFM 文档：

```markdown
| 名称 | 数值 | 说明 |
| --- | ---: | --- |
| 甲 | 10 | `x + 1` |
| 乙 | 20 | **不执行** |
```

验收断言：

1. `marqdo check` 和渲染结果与原文兼容；单元格中 `**不执行**` 仍不执行。
2. Table 逻辑行数为 2，列顺序固定，`说明` 列内容按当前 GFM 数据值映射规范表示，而非误执行代码。
3. `.mq.md` 的 `table.filter` 能选择 `数值` 大于某阈值的行，不触发新的 Rust Builtin；后续 `table.join/group` 同理。
4. 表格值复制与变换不使原表发生副作用；大数据优化后仍保持同样结果。
5. 对 GFM 表格的来源引用、错误 span、序列化与 `Value::display()` 有 golden 比较，避免丢失文档原本的知识联系。

---

## 7. 不仅要“能算”，还要证明**可组合性在复杂程序中闭合**

### 7.1 闭包测试的基本定义

设 `K` 为核准后的 Marqdo Core 计算能力，`E` 为已经存在且获授权的 OS Resource 端点，`L(K,E)` 为**仅以 `.mq.md` 编写**、允许调用 `K/E` 的所有高层算法及应用。

对于高层能力 `F`，要求提供明确构造：

```text
F ∈ L(K,E)
```

这不是“理论图灵完备”的宣称，而是要求**真实程序可运行**且达到：可恢复错误、足够的结构访问/构造、可检查的权限、可控的内存/时间开销和离线可重现。

如 `F` 无法实现，必须归因于以下之一：

- **K 缺口**：无法组合出所需的计算/存储能力，考虑扩展最小通用原语；
- **E 缺口**：OS 没有向程序授予所需原始能力，新增**独立 Provider/端点**而非新增算法 Builtin；
- **性能缺口**：语义可以实现但实用性能不足，先分析 VM 开销和优化编译器，允许**可替换**的原生优化；
- **表达/人体工程缺口**：需要大量重复样板但语义已足够，优先用 `.mq.md` 高层封装，而非新增关键字；
- **规范/安全缺口**：语义不明确或权限不足，先规定边界而非偷开后门。

### 7.2 标准闭包任务矩阵

| 测试 | `.mq.md` 必须实现的主体 | 需要的最小 K | 需要的 E | 禁止取巧方式 |
|---|---|---|---|---|
| C01 多种分配器 | bump、first-fit、free-list 复用、合并 | 区域大小/增长/字节索引、整数、循环 | 无 | 用 `std::alloc` 为每块建立特殊 HostOp |
| C02 链表/二叉树/图 | 节点布局、插入/删改/遍历、循环引用 | Region、整数地址、函数/循环 | 无 | 直接调用 Rust `LinkedList`/`petgraph` |
| C03 二进制编解码 | LE/BE u16/u32/u64、变长整数、位域 | 算术、字节读写、可恢复错误 | 无 | 将 `pack/unpack` 全部设为 Rust 内置 |
| C04 Unicode/文本 | UTF-8 编码、解码、非法序列策略 | Text 观察、Memory、整数及文本构造 | 无 | Python codecs/隐藏 Rust UTF8 parser 为唯一实现 |
| C05 JSON/CSV | 动态输入、解析、构造、格式化、错误定位 | 动态 List/Map/Text 构造、索引、错误返回 | 无 | `serde_json` 作为唯一实现 |
| C06 GFM Table 算法 | filter/map/join/sort/group/列投影 | Table 逻辑访问、动态 List/Map 构造 | 无 | 每新操作增加 `HostOp::TableX` |
| C07 自举打印 | 任意允许的 Value→规范文本→UTF-8→输出 | `type`/结构观察/格式化/内存 | 已授权 stdout Stream | 仍用 HostOp::Print 执行 `Value::display()` |
| C08 多轮交互 CLI | 提示、读一行、解析用户输入、循环输出 | 文本/字节/错误、控制流 | stdin、stdout | 特增 Rust `input()` |
| C09 文件复制与格式化 | 分块复制、短读短写、文件格式解析 | Buffer/Memory、循环、结果错误 | 授权文件资源流 | 特增 Rust `copy_file()` |
| C10 HTTP/1.1 客户端核心 | 请求构造、流处理、首部/chunked、连接关闭 | Memory/Text/错误/集合 | 已连接 TCP Stream | Rust HTTP 客户端做全部报文解析 |
| C11 自编简单字节码机 | 虚拟寄存器、指令解码、内存、分支、堆栈 | Memory、整数/位/条件、动态函数 | 无 | 在 Rust 中写整台 VM，Marqdo 只调用 |
| C12 科学计算准备 | 矩阵/稀疏矩阵布局、切片索引、数值迭代 | Region/数值/循环、精确索引 | 无 | 先要求 NumPy/Python 内核才能运行 |

**加严的反规避规则**：C01–C06、C11–C12 在测试环境中必须禁用 Python、Node、Foreign、动态原生插件以及所有与任务等价的专用 Host Builtin；C07–C10 仅保留声明过的 Resource 底层端点，不得允许平台提供整套高级任务实现。

### 7.3 不属于“纯计算闭包”的功能：必须另外审计

| 能力 | 为何无法靠 K 单独获得 | 基础交付要求 |
|---|---|---|
| `read_line()` 里的实际按键输入 | 真实数据来自 OS/stdin | stdin 资源端点，用户授权和 EOF 语义 |
| 文件/目录、网络、进程 | OS 对象不在用户的虚拟 Memory 内 | 细粒度的权限资源及平台支持矩阵 |
| HTTPS | TCP 只是字节流，TLS 要协议、安全随机数、证书信任和安全实现 | 可用受审计 TLS Provider 或安全实现，不能以裸 HTTP 解析冒充 |
| 高精度时间与熵 | 不是确定性计算结果 | 时钟/随机资源端点，不允许把伪随机当作加密安全熵 |
| CPU/GPU 大规模并行 | 普通 load/store 不定义数据竞争、原子性与设备执行 | 独立调度/同步或异构后端，另行语义批准 |
| QPU | 量子态不可被普通字节地址任意读取/复制 | 正式量子设备端点及资源限制 |

### 7.4 “不需要继续写 Rust”的强制证明方式

以 **三类不修改核心** 测试作为工程门禁：

- **扩展算法**：在一个固定的 `marqdo` 可执行文件上，安装新 `.mq.md` `std.table.join`、`std.json.parse`、`std.memory.buddy`，不得重编译 CLI，不调用 Foreign。
- **扩展系统高层功能**：已有文件、TCP 流端点时，安装新的 `.mq.md` 文件格式解析器、网络协议解析器，只增加标准库，不增加 Provider。
- **扩展真正系统原始功能**：引入新的测试 Resource Provider（如具备 `read/write` 的虚拟终端设备），不得改 Runtime/Expression/HostOp；**允许**单独实现和安装 Provider。其权限与版本协议必须能被现有 CLI 识别。

前两项失败意味着内核/标准库尚不充分。第三项失败意味着系统扩展架构不稳固。三者不是一个问题，不得用某一项通过代替另外两项。

### 7.5 与 Python/Rust 实现的性能比较应如何解读

该 RFC 不承诺初版字节解释器的吞吐能比肩优化后的 Rust/C。性能验收需要分成两个层次：

**语义与正确性优先门禁**：同样的测试输入、确定性结果、正确内存边界、正确错误/输出与算法复杂度表现；不得用“Rust 版本很快”代替 Marqdo 版本通过。

**工程性能评估**：记录 `wall_time / peak_RSS / CPU / fuel / bytes_moved / allocations / copy_count`，与 Rust 原生和其他参考语言比较。初期不设无依据的“必须达到 80% 原生速度”的硬指标；若性能瓶颈来自逐字节 Rust↔Marqdo 调用/隐藏复制，先优化执行路径、批量传输、字节码/JIT，再讨论原生加速。任何 O(1) 原语若退化为 O(n) 隐式全表复制，必须单独报告。

### 7.6 三种验收样例（按 RFC-LANG-0002 书写的目标伪代码，非现有可运行程序）

**A. 用 Marqdo 实现堆分配器而不调用 Rust `malloc` 算法**：

````markdown
## @exec 分配

内存管理器保存 region、next、free-list 等元数据。

`if 存在足够的空闲块`

`复用空闲块并更新元数据`

`else`

`需要时增长(region)`

`更新区域中的尾指针`

`end`

`return 分配得到的偏移量`
````

**B. 打印与读取由标准库完成**：

````markdown
## @exec main

`输入 = io.read_line()`

`输出 = "收到：" + 输入`

`io.print(输出)`
````

**C. 同一个 GFM 表格运行不同的 `.mq.md` 算法**：

````markdown
## @exec 分析数据

| 名称 | 分数 |
| --- | ---: |
| 甲 | 87 |
| 乙 | 92 |

表格是数据，不在渲染时执行；函数通过显式数据引用取得逻辑 Table。

`结果 = table.filter(输入表, 过滤函数)`

`return 结果`
````

B/C 中的函数名、输入参数和文档表格访问均仅为目标接口示意，须在真正实现时遵守冻结的 `## @exec` 函数声明、参数推断/导入和文档资源寻址规则。**不能为了示例好看就把尚不存在的语法当成现实。**

---
## 8. 重构目标架构、仓库组织与实际代码改动

### 8.1 独立标准库的仓库和发行方式

推荐另建 `cflmy/marqdo-stdlib`，**分仓开发**，但**官方发行版必须搭载可离线使用的兼容版本**。避免用户刚安装 CLI 就因为网络不可用而无法调用 `打印`。

```text
marqdo-cli/                        # 语言、执行器、系统边界与工具
  crates/marqdo-markdown/           # GFM AST
  crates/marqdo-expression/         # expression / Place
  crates/marqdo-value/              # Value、MemoryHandle、ResourceHandle
  crates/marqdo-ir/                 # 语义 IR 与 callable 描述
  crates/marqdo-runtime/            # MemoryStore、解释器、预算/诊断
  crates/marqdo-host/               # Capability 与底层流/资源适配
  crates/marqdo-plugin-abi/         # 版本化 Provider ABI（复用现有）
  crates/marqdo-package/            # 锁定的标准库依赖/版本兼容
  crates/cli/                       # run/call/check/inspect 与打包
  spec/                              # 语义、错误、权限、wire 的规范
  docs/architecture/                # RFC-CM-0001/0002 + ADR

marqdo-stdlib/                       # 全新独立仓库（尚不存在，拟建）
  core/                              # Result、通用结构处理
  memory/                            # allocator、指针编码、arena
  collections/                       # list、map、tree、graph、queue
  numeric/                           # 整数位操作/数值算法
  text/                              # UTF-8、格式化、缓冲、parser
  io/                                # stdin/out、read_line、write_all
  fs/                                # 文件流读写与文件格式辅助
  data/                              # csv、json、GFM Table 变换
  net/                               # 可用 socket 上的字节协议算法
  examples/
  tests/
  compatibility/                     # Runtime 核心能力要求和版本约束
```

标准库中的 `core` 指**标准库基础算法**，不是第二个 Runtime；不能定义与 CLI 冲突的 `Value`、`MemoryHandle` 或安全许可规则。

#### 发行契约

- CLI 发布包随附 `marqdo-stdlib` 的固定 digest/版本，可离线解析，不需要联网、Git 可用或隐式权限。
- 锁文件记录 stdlib 实例、内容摘要和要求的核心能力版本；不能只校验“模块路径存在”。
- 分仓主干可以更快演进，但官方发行组合由兼容 CI 验证，升级库不强迫重编译 CLI；库若要求新的核心原语，版本范围必须准确拒绝旧运行时。
- 用户自定义标准库版本可采用显式导入/锁定；默认兼容别名解析可使用随发行包固定的标准库，不得让外部项目文件偷偷覆盖 `std.io.print`。
- 严格保留文档模块的源文件与 span，`inspect`/调试器能看到标准库真实 `.mq.md` 调用堆栈。

### 8.2 crate 级最小改动计划

| 改动批次 | 源码位置 | 允许修改的内容 | 明确禁止 |
|---|---|---|---|
| A | `marqdo-expression` | Place 左值语法/AST、span 与错误定位 | 为 `print/table/malloc` 增加关键字 |
| B | `marqdo-frontend`, `marqdo-ir` | 映射 `AssignTarget`，声明基础内存操作、受控资源操作元数据 | 新的 Markdown 特殊块解释器 |
| C | `marqdo-value` | `MemoryHandle`、`ResourceHandle` 的语言值外壳；逻辑 Table API | 公开宿主 `*mut u8` 或 `Vec` 地址 |
| D | `marqdo-runtime` | `MemoryStore`、有界索引读写、增长、调用/返回值生命周期、异常/预算/trace；最小结构操作 | 链表、CSV、JSON、HTTP、完整 malloc 等专项算法 |
| E | `marqdo-host` | 将 OS 字节 I/O 从 `Print` 分离；能力检查与 Stream 适配；复用 Provider | 每新增一个高层 API 增加一个 `HostOp` |
| F | `marqdo-plugin-abi` / `CallableRegistry` | 在确有需要时增补资源协议、缓冲区交换及版本元数据 | 平行重建第二套插件系统 |
| G | `marqdo-package`, `cli` | bundled stdlib、模块解析优先级、离线发版、兼容链接与显示 | 在编译/导入时默默授予额外 OS 权限 |
| H | `spec/*`、tests | 冻结新核心语义、能力矩阵、wire、AST golden、跨平台证据 | 只改实现不改规范与测试 |

### 8.3 Runtime 的抽象状态与资源预算

提议以一次 `RunContext` 表示执行范围：

```text
RunContext {
    program_ir
    frames                 # 现有调用帧、局部变量与中间值
    memory_store           # region_id -> {length, max_length, bytes, generation}
    resource_table         # resource_id -> {provider, rights, state, generation}
    capabilities           # 有效 grant（非 manifest 的请求本身）
    budgets                # fuel, memory, stack, output, I/O, deadlines
    trace_context          # source spans、语义事件、资源调用审计
}
```

以上为逻辑组织方式，不要求把当前 Rust 代码改成一个巨大 `RunContext` 结构体。重点是所有功能在执行时**必须共享这些约束**，不能让标准库通过另一条内部通道绕过预算或权限。

内存预算至少区分虚拟区域**声明的最大容量**和**已提交字节**；对于物理稀疏页的实现要明示计费规则。程序要求增长时，任何预算失败都不能产生半增长状态。

### 8.4 需要重新设计的 `HostOp` 兼容窗口

迁移不能从一开始就删除 `HostOp::Print` / `FsRead` / `FsWrite`。建议：

```text
旧 Marqdo 程序             新 Marqdo 程序
  print(value)              std.io.print(value)
       │                           │
       │（compat alias）           │
       └─────────────┬─────────────┘
                     ▼
              标准库函数实现
                     ▼
           resource.stdout / Stream.write
                     ▼
              CapabilityHost/OS
```

与 Rust 路线并存时要显式测试以下阶段：

- **Compat-A**：旧 Builtin 仍是默认，新的 `std.io.print` 通过显式导入调用；运行差异比对。
- **Compat-B**：旧 `print` 别名路由到 stdlib（可通过调用注册表映射到模块函数），旧 Rust 实现只用于受限对照测试与迁移回退；确保签名、返回 `Unit`、异常与权限兼容。
- **Compat-C**：所有端到端测试通过后，从公开 Builtin 实现中移除 `Print` 特例（是否保留过渡兼容名由 ADR/版本策略决定）。

**审查红线**：最终的 `打印` 调用栈不得再落到 Rust `Value::display()` 全量格式化；stdout 的物理字节写入保留在系统边界，这是正常且必要的。

### 8.5 异常和输出的执行语义

新的标准库会调用大量可能失败的功能。应明示：

- `read` 可能短读、EOF、暂不可读；`write` 可能短写，部分写入不能假装没有发生。
- `close` 的失败和二次关闭如何表现、是否自动释放、程序退出如何关闭未关闭资源。
- 错误由底层端点返回到 `.mq.md` 的结构化值，用户能在普通分支里处理；禁止“Rust 遇到 OS 错误自动终止整个解释器”成为常态。
- 输出预算在底层 Stream 中计费，不能因为换成 `std.io.write_all` 就被绕过；错误与权限审计记录在独立通道。
- 宿主 I/O 引起的非确定性不能被误标记为纯计算可重放；需要 record/replay 时应记录输入字节摘要/授权信息与实际读取结果，遵守敏感数据日志政策。

---

## 9. 实施路线：每个阶段都有阻断门禁，不允许无限制扩张

整个迁移**采用纵向闭环**。只完成“抽象层设计”而没有真实 `.mq.md` 端到端应用，不算完成。

### P0：冻结旧语义与精确基线

**工作**：

- 在最新代码提交上确认旧 AST golden、数值和 GFM Table 兼容用例、 `print` 全类型输出、Host 权限/预算/Trace、std::fs 行为、安全矩阵和锁文件。
- 对本 RFC 与 RFC-CM-0001 的所有用户可观察变化开 ADR，明确哪些是向后兼容、哪些必须升级语言语义版本。
- 确保现有 CI/发布候选状态不被未完成的计算重构污染；新功能独立分支，稳定发布线路独立维护。

**通过**：可运行基准套件有可复制命令、固定 SHA、平台、预期结果和失败输出；规范中所有修改点列出。

### P1：Memory 与位置赋值的单一闭环

**工作**：

- `Memory` 有身份、创建/大小/增长、单字节读取和写入；Memory 值共享而 List/Map/Table 不改变既有值语义。
- `AssignTarget::Index` 仅接受合法的 Memory；其他目标失败；大型/负地址/溢出、0 字节区域、边界、增长失败明确诊断。
- Memory 不跨线协议成为可伪造句柄；运行时负责释放与配额；trace 来源保留。

**通过**：区域隔离、别名、内存增长、回收、序列化拒绝、fuel/预算/跨函数传递及旧索引语义测试全部通过；不存在新增高级数据结构 HostOp。

### P2：证明通用值与存储共同形成计算闭包

**工作**：

- 用 `.mq.md` 写 bump allocator → free-list allocator → 链表/树/图、字节 codec。
- 补齐最小的 List/Map/Text/Table 结构观察、动态构造和可恢复错误；不得为了实现 JSON、Table 或 print 每次再增加一个任务特化 Rust 函数。
- 用 `.mq.md` 实现 UTF-8、JSON/CSV 的至少一种完整解析/生成路径，并能处理恶意/无效输入。

**通过**：C01–C06、C11 在禁止 Foreign/Plugin 的测试环境里通过；动态数组不靠固定长度源代码；没有重新编译 Rust 才能新增算法的步骤。

### P3：标准资源接口与真正的 I/O 闭环

**工作**：

- 先在 `project` 模式为已授权 stdin/stdout/文件实现有界字节流，保留原有 console 权限和输出预算。
- 明确 ResourceHandle 生命周期、参数校验、EOF/partial/error 和 Provider 注册协议；已有 ABI 能用则复用，仅在确实无法正确传递字节/句柄时升级 ABI。
- 实现 `.mq.md` `io.read_line/write_all/utf8/print` 的完整调用链，不依赖 Rust `Input`/`Print` 专用操作。

**通过**：C07–C09 在不允许 Host 高级 API 的测试环境通过；虚拟测试 Provider 不改 `HostOp` 就能实现 Stream；纯模式无新增权限，strict 对未验证功能 fail-closed。

### P4：GFM Table 的内部语义与标准库迁移

**工作**：

- 逻辑值访问/构造接口稳定化；保持 `Value::Table` 语义兼容和 GFM AST 惰性。
- `.mq.md` 实现表格过滤、投影、排序、关联、聚合、转 GFM 文本；必要时将物理行存储抽象化，但不强制使用 Memory。
- 仅在证明存在明确性能收益时试验 Column Store/Memory-backed 数值列。

**通过**：C06 完整、行列的语义/错误/格式化兼容；跨实现存储测试结果一致；零 Rust 专项 Table 算法；旧 AST golden 不变。

### P5：外部可扩展性与完整系统构造

**工作**：

- 接入经授权的 TCP Stream / 目录操作等经过支持矩阵定义的端点；用 `.mq.md` 写 HTTP 请求、响应与 chunked 解析（**不假装 HTTPS/TLS 已经安全实现**）。
- 引入第三方测试 Provider，例如 mock sensor 或虚拟流设备，证明无需修改解释器/HostOp 就可扩展。
- 在原有 package/lock 上实现 stdlib 单独版本与离线捆绑，补充模块名称遮蔽和兼容门禁。

**通过**：C10 与新增 Provider 测试通过；新高层系统功能只提交 `.mq.md` 文件、测试和文档；不修改 `crates/marqdo-runtime` / `crates/marqdo-host`。

### P6：独立性能与科学计算能力（在 P0–P5 稳定后）

**工作**：

- 收集 profile 和燃料、内存、真实吞吐证据；定位逐字节解释器开销与结构复制成本。
- 在不改变公开语义前提下尝试字节码解释器、宽度化字节操作、区域高效复制/视图、CPU 数值后端；并行原子/同步另开独立 ADR。
- 量子设备、GPU 应通过明确的资源/编译后端协议接入，不把 Qubit 当作普通 Memory，也不声称会自然获得加速。

**通过**：优化实现与参考解释器对确定性用例一致；性能和资源预算可复现实验，不能为了速度静默改语义或绕过权限。

### 9.1 为什么严格按这个顺序

- 没有 P1，无法真正用 Marqdo 构造低层结构；
- 没有 P2，直接迁移 print/table 只会让标准库重新求助 Rust；
- 没有 P3，`打印` 迁移不完整，因为它依赖真实 stdout；
- 没有 P4，Table 只是把旧内核包了一层新 API；
- 没有 P5，无法证明“新系统高级能力不修改 Host”；
- P6 过早进入，会把基础语义尚未稳定的问题掩藏在编译器和硬件后端复杂度中。

**在任一门禁失败时不向后推进并行的大规模迁移**，而是定位到底缺少 C、E、性能还是可读性，再对现有设计做**最小修正**。

---

## 10. 全量验收规范：CI 必须能自动判断“完成”

### 10.1 规范兼容（强制阻断）

| 编号 | 测试 | 硬性通过标准 |
|---|---|---|
| A01 | 原始 Markdown AST golden | 普通链接、表格、段落、标题与内联 CodeSpan 的 AST 映射不变；新增 Place 仅在合法执行负载中解析 |
| A02 | 原始值语义 | Int/Float/Text/List/Map/Table、调用、拷贝、数值错误与原规范等价 |
| A03 | Memory 区域别名 | `a=m` 与 `b=m` 改动同一内存；不同区域相同偏移互不影响 |
| A04 | Memory 增长与边界 | 成功零初始化、保留旧字节、失败原子回滚；极大 BigInt/负索引/越界拒绝 |
| A05 | 句柄与线协议安全 | 伪造/陈旧的 Memory/Resource 句柄不可使用、不可跨运行盲用，错误不 panic |
| A06 | Place 赋值合法性 | 仅目标允许的位置可写；表达式基址/索引/RHS 求值一次且顺序一致；不悄悄改变 List 值语义 |
| A07 | 错误恢复 | parser/IO 的可恢复失败可被 `.mq.md` 处理；fatal 错误仍有 span 与稳定错误类别 |

### 10.2 标准库独立性（强制阻断）

| 编号 | 测试 | 硬性通过标准 |
|---|---|---|
| B01 | 分配器 | `.mq.md` 成功实现 allocate/free/reuse，碎片/合并/重复 free 情况可测试，不需 Rust `malloc` 特例 |
| B02 | 复杂数据结构 | 链表、树、图、散列表/其他自实现容器能动态扩展、插入、删除、遍历 |
| B03 | 二进制处理 | 字节序、整数编码、畸形字节序列、增长、越界处理均在 `.mq.md` 中实现 |
| B04 | UTF-8/JSON/CSV | 复杂数据可解析与序列化，错误可被 `.mq.md` 用户程序捕获 |
| B05 | 打印格式化兼容 | `print` 的所有旧 Value golden 输出一致；新资源句柄不泄露敏感身份 |
| B06 | 表格语义/操作 | GFM 逻辑值可 filter/join/group，原表不被变更，来源与格式化无回归 |
| B07 | 真正无宿主算法 | 固定 CLI 二进制、禁用 Python/Node/Foreign/native plugin，新增 `.mq.md` 实现 C01–C06/C11/C12 的选定用例 |

### 10.3 系统交互和扩展（强制阻断）

| 编号 | 测试 | 硬性通过标准 |
|---|---|---|
| S01 | stdin/stdout 多轮交互 | 使用 `.mq.md` 读行、编码、打印，支持中文、EOF、空行、长行、无换行尾部 |
| S02 | 短读短写 | 强制每次最多传输 1–3 字节，标准库能完成任务，无丢失/重复/无限循环 |
| S03 | 文件字节流 | 授权路径可读写，拒绝路径穿越/未授权路径，关闭/异常正确 |
| S04 | Provider 扩展 | 新独立 Provider 通过现有 ABI 演进的契约进入，无须修改 Runtime/HostOp/重编 CLI |
| S05 | 无 grant 失败 | Resource 没有授权则无外部效果，明确错误及审计；manifest 声明不能等价于 grant |
| S06 | strict 安全 | 未经真实 OS 隔离验证的 I/O Provider 在 strict 中拒绝，不能静默降级到 project |
| S07 | print 真实迁移 | 通过 trace/调用信息证明确实进入 `.mq.md` format/write_all，而非隐藏旧 `HostOp::Print` 逻辑 |
| S08 | 资源预算与渠道 | 输出限额、内存限额、fuel、stdout/stderr/JSON 分离、一致的错误码有效 |

### 10.4 发布、性能与知识追踪（强制阻断）

| 编号 | 测试 | 硬性通过标准 |
|---|---|---|
| R01 | 离线标准库 | clean environment 安装官方 CLI 后，不联网可调用默认标准库，且记录 stdlib digest |
| R02 | 独立升级 | 新 `.mq.md` stdlib 在兼容的旧 CLI 二进制中运行；不兼容的版本清楚拒绝 |
| R03 | 平台矩阵 | Linux/macOS/Windows 的可用功能与拒绝路径有可复现证据；未支持的平台功能明确 unavailable |
| R04 | 跨实现等价 | Table 不同物理存储/Memory 实现的结果、错误、输出、span 与原规范一致 |
| R05 | 性能画像 | 可复现记录字节吞吐、GC/内存占用、调用次数、fuel、表格复制成本；不隐瞒严重退化 |
| R06 | 知识溯源 | `inspect`、trace、SourceSpan 显示 stdlib 文件调用及 Memory 语义事件；GFM 表格/文档链接不意外执行 |

### 10.5 CI 的“冻结宿主”实验

必须至少有一条专门流水线，其构建后先**冻结 CLI 可执行文件哈希**，之后执行：

```text
build CLI once  → checksum(Marqdo binary) = H
     │
     ├── 在独立 stdlib 仓库新增算法模块
     ├── 仅修改 .mq.md、manifest/lock、测试与文档
     ├── 禁用 foreign / native plugins / task-specific host builtins
     ├── 运行算法闭包与 I/O 测试（仅授予所需底层端点）
     └── assert checksum(Marqdo binary) == H
```

测试应同时通过静态检查（是否新增 Rust Builtin/HostOp、是否调用 Foreign）和动态审计（是否偷偷进入专用 Rust 算法路径）。不能只看最终输出是否正确；否则一个隐藏的 Rust `json.parse()` 也可能蒙混过关。

### 10.6 负面验收与安全边界

- 动态分配器可以被恶意/错误程序写坏**同一区域中的其他块**；这属于明确的裸内存风险，不能标为“被沙盒完全防护”。
- `memory[huge_integer]`、负索引、非法字节、未授权网络/文件、伪造句柄、关闭后使用：均不得崩溃宿主、越权访问其他区域或被吞成无错误结果。
- `Resource` 返回的敏感数据不能无条件完整落盘为 Trace；Trace 默认只写操作类型、授权、偏移、字节数及必要摘要。
- native 插件误配置与 Worker 的权限边界应分别验证；普通 Worker 不是 OS 隔离，未满足 strict L1 的实际隔离要求必须拒绝。
- 内存增长后 Provider 不得继续使用旧 `Vec` 的真实地址；任何可能有别名的缓冲借用必须先复制或正确固定生命周期。
- 库函数可以 fail-fast，但不能无视已经完成的部分外部效应；正常的解析失败要有可恢复结果。

---
## 11. 需要做出的正式决策（ADR 清单）

**本 RFC 是推荐架构，不应让开发者在没有决策记录的情况下自行猜测。** 下列事项需要分别冻结；这些 ADR 的目的恰恰是防止将来凭方便临时加入越来越多的 Rust Builtin。

| ADR | 待冻结问题 | 本 RFC 推荐决定 | 重要性 |
|---|---|---|---|
| CM-A01 | Memory 区域身份与生命周期 | 一次运行内不可伪造句柄，复制保留共享身份；区域运行结束释放；内部块由 stdlib 管理 | **阻断 P1** |
| CM-A02 | 内存大小、增长与预算 | 多区域、按字节寻址，64 KiB 候选逻辑页，增长失败不修改原内存 | **阻断 P1** |
| CM-A03 | 位置赋值与求值顺序 | Local 重绑定、Memory Index 可写；普通 List/Map/Table 原语义不变 | **阻断 P1** |
| CM-A04 | 复合值的最小结构契约 | List/Map/Table 的安全遍历和动态构造；支持自举 format/parser | **阻断 P2** |
| CM-A05 | 可恢复错误模型 | 以现有数据值和分支优先表达 Result；明确 EOF/partial/permission/timeout | **阻断 P2/P3** |
| CM-A06 | Stream/Resource 操作协议 | 权限签发句柄、Buffer 读写、可注册端点、短读短写、generation | **阻断 P3** |
| CM-A07 | 标准库模块命名与内置别名 | stdlib 正常模块解析，`print/打印` 有透明兼容转发 | **阻断 P3** |
| CM-A08 | GFM Table 逻辑值与物理存储 | 保留独立 Table 语义/惰性 AST，表格算法迁 stdlib，存储可替换 | **阻断 P4** |
| CM-A09 | Provider ABI 的必要最小升级 | 优先复用现有 ABI v1/Worker/CallableRegistry，按证据扩展字节资源契约 | **阻断 P3/P5** |
| CM-A10 | stdlib 的版本/锁定/离线发版 | 独立仓库，固定随包版本，兼容性与内容摘要受 CI 管理 | **阻断 P5** |
| CM-A11 | strict/proj/pure 的新资源权限 | 维持 fail-closed，不把新 I/O 自动宣布为 strict 可用 | **阻断 P3** |
| CM-A12 | 外部原生能力的未来扩展 | 不开放一般任意 syscall/raw host pointer；设备 Provider 单独授权 | 后续 |
| CM-A13 | 并行共享内存与原子操作 | 当前顺序内存为主；未有原子性规范前不允许共享写无序化 | 后续 |

**每项 ADR 的模板**应同时列出：非目标、现有行为、候选语义、反例、可组合性证明/反例、兼容策略、权限和错误、性能/预算影响、自动测试、撤回条件。

---

## 12. 为什么这种架构优于“继续增加 Rust Host 函数”

### 12.1 可扩展性与“核心稳定”的关系

采用本 RFC 后，新增 `std.table.join`、`std.memory.buddy`、`std.io.read_line`、`std.text.utf8_decode`，都只需要标准库提交，**它们的语言执行语义已经由 Core 所定义**。这些算法可在不改 CLI 的环境中升级与单测。

继续增加 `HostOp::Xxx` 的路线则恰恰相反：每个新功能需要固定枚举、静态签名、权限、Runtime 分派、Rust 实现、CLI 重发和多平台测试；它的维护成本基本随功能数量线性增长，并强化了语言是“宿主 API 的封装器”的印象。

但**最少 Rust 代码并非优化目标**：存在足以保证值、内存、字节、I/O 原子语义的 Rust 实现是正常且必要的。真正需要减少的是“每实现一项高级算法，就必须增加一项独有的宿主实现”。

### 12.2 两套存储是否破坏统一性？

保留 `List/Map/Table` 等托管值，并引入用户可编程字节 Memory，确实形成两个存储领域。这不是完全零成本：需要明确定义值语义、复制和引用、相等性、序列化、资源预算。

但强制统一为一个暴露指针的字节堆，会要求所有标准库作者手工编码字符串、任意精度整数、异构表格和垃圾回收根，且会使 Markdown 值与机器布局紧密耦合。**统一应该体现在“共享一套计算与调用语义、公共错误/预算/追踪规则”，不应强制所有东西使用相同字节布局。**

将来若需要更深层统一，可以先证明同一个抽象存储接口可以拥有不同后端，再谨慎替换实现，而不是现在推倒 `Value` 系统。

### 12.3 为什么不暴露真实寄存器或机器码

真实寄存器不是跨 CPU 平台可移植的语言值，硬件寄存器调配应该由后端完成。若程序想表达虚拟 CPU 或相关论文算法，允许在 Memory 中**自己构造寄存器文件和指令解释器**，比给语言增设 32 个寄存器关键字更符合扩展性目标。

直接注入机器码可以作为经明确授信的可选互操作手段，但它会绕过抽象 Memory 和权限模型的可验证界限，不应变成普通标准库运行的依赖。未来原生编译 Marqdo 本身，则属于后端优化/部署能力，不能反向改变计算语义。

### 12.4 面向并行与量子计算的长期优势与明确限制

**优势**：显式 Memory 区域、逻辑地址、值计算和透明资源边界，允许描述缓冲区布局、批量数据准备、重复试验、串行解释器参考执行、数据变换；可用于将来多后端的语义一致性测试。

**限制**：GPU 的并行 Kernel、设备内存同步以及 QPU 的门/测量语义不由内存读写自动导出。真正的并行共享写需要额外的原子性/调度契约，量子态不可视为可以任意 `mem[i]` 访问的普通字节；这些内容**必须以后端/资源层独立验证，不能悄悄加进 Core**。

---

## 13. 与 RFC-CM-0001 的继承、修订和增补关系

| RFC-CM-0001 结论 | RFC-CM-0002 的处理 |
|---|---|
| 托管值 + 多区域线性内存 + Resource | **完整继承**，明确各自生命周期与共同预算 |
| 内存通过普通索引访问、写入 | **继承**，补充 Place AST、求值与错误规则 |
| 自定义 `malloc/free` 与数据结构放 stdlib | **继承并强化**，以闭包测试禁止 Rust 专项算法捷径 |
| Resource ABI 提供 OS 最后一跳 | **继承并细化**，定义流端点、授权、短读短写与不可伪造句柄 |
| GFM Table 是文档值，不是内存本体 | **继承并扩展**，明确定义逻辑 Table、物理存储、迁移与兼容 |
| 核心保留普通值 | **继承并补齐**，指出 List/Map/Table 的通用构造/遍历是 `print/JSON` 自举前置条件 |
| `print/fs.read/fs.write` 仍在固定 HostOp | **新增迁移**，高层操作移入 stdlib，底层字节端点保留 OS 边界 |
| stdlib 独立仓库、CLI 随包离线提供 | **继承并细化**，建立版本/digest/CI 与兼容别名策略 |
| 验收以分配器、CLI 与系统资源为核心 | **升级为闭包验证**，增加 Table、泛型格式化、UTF-8、JSON、HTTP、虚拟 CPU 等领域 |

此 RFC 不改变 RFC-CM-0001 的安全非目标，也**不宣称已在仓库实施**。若实施中证明原方案缺少必要机制，应提出精确反例并修改 ADR，不应以“Rust 比较快”直接恢复一条高层 Builtin 后门。

---

## 14. 开发纪律：每次增加功能前必须填写的审查记录

建议把以下检查问题放进 PR 模板（并通过 lint/CI 自动核验能自动核验的部分）：

```text
新增能力名称：
使用场景与可观察语义：
是否仅涉及普通计算/数据结构？
是否需要新的 OS/设备外部效果？
已存在的 Core 原语、Resource 端点是什么？
最小 .mq.md 构造实现（文件链接与测试）：
是否新增 Markdown 节点执行语义 / Builtin / HostOp / ABI？为什么无法避免？
是否引入可以被现有功能组合代替的特化原语？
与现有 List/Map/Table、Memory、权限、Trace、错误规则的兼容性：
是否有无 Foreign/无 Native Plugin 的独立运行测试：
若提出原生优化，是否有可替换的 Marqdo 参考实现与性能数据：
平台支持与 strict 隔离证据：
版本与发行兼容：
```

**变更批准规则**：仅仅“API 很常用”“Rust 更方便”“某个第三方库已经实现了”“目前性能不太好”，都**不足以单独成为新增语言核心原语的理由**。必须说明现有底层原语的真实语义缺口、不可接受的复杂度或无法弥补的性能瓶颈，并提供可重复的证据。

**阻断性的反面信号**：

- 新增一个 parser/formatter 就修改 `crates/marqdo-runtime/src/builtins_impl.rs`；
- 新增一个业务功能就修改 `HostOp` 枚举；
- 新的“标准库”主体逻辑其实是 Rust ABI 插件，`.mq.md` 只做一行转发；
- 表格迁移后只剩下裸字节 Memory，高级 Table 值无法自然读取或组合；
- 为了方便 I/O，绕过 Resource 权限或让用户传递真实宿主指针；
- 只完成 `memory.new`，不证明用户能够用它写独立的分配器与复杂程序；
- 只有 happy path，未测试虚假句柄、错误、权限、跨平台和部分 I/O；
- 为了看起来“底层”，增加寄存器、机器码或专用硬件关键字而没有计算闭包证据。

---

## 15. 最终架构裁决建议

**建议采纳**：

1. **语言核心是少量可组合的计算机制**：已有函数/值/控制流 + 通用结构观察/构造 + 可写位置 + 多区域可增长 Memory + 可恢复错误。
2. **OS 交互通过 Resource 能力边界进行**：已授权流/文件/网络/时钟/设备资源具有版本化契约；高层功能尽量由 `.mq.md` 完成，最后一跳保留可信系统适配器。
3. **标准库真正独立**：`print/格式化/utf8/io.read_line/fs.*_text/table.*`、自定义内存分配器、JSON/CSV 和其他一般算法优先使用 `.mq.md`；随官方 CLI 捆绑固定兼容版本，同时允许独立发布。
4. **GFM Table 是逻辑数据和知识文档价值，而非存储指针**：重构内部布局、公开逻辑操作接口、迁移高级算法，但保留 Markdown 惰性和既有值语义。
5. **迁移需要可证伪的门禁**：冻结 CLI 二进制，不新增 Rust/HostOp，在独立 stdlib 中完成可增长数据结构、泛型打印、文本/表格解析、交互式 CLI、已授权网络协议解析；不满足即不能宣称“自主计算/自主系统扩展闭环完成”。

**最终边界声明**：

> **Marqdo 可以摆脱“每个新功能都依赖 Rust/ABI 现成算法”的开发方式，不能摆脱“任何真实计算都由某个执行环境执行、与操作系统交互必须跨越受控平台边界”的事实。**
>
> 优秀的计算内核不在于直接支持多少个功能，而在于以**精确、有限、可组合**的基本语义，让标准库不断生长，同时不迫使 CLI 的语法、解释器和 Host 随之无界膨胀。

本 RFC 建议作为下一阶段开发的**上位约束文档**；首次开发只推进 P0→P2（计算闭包），经验证后推进 P3→P5（系统标准库迁移）。当前 stable/release 收敛与架构试验应当分支隔离，**不得将本 RFC 的未完成代码混入已声明受支持的稳定发行版**。

---

## 附录 A. 术语表

| 术语 | 本 RFC 中的含义 |
|---|---|
| Core | Marqdo 的语言计算语义/不可由自身算法构造的基础机制 |
| Managed Value | Runtime 管理存储且按语言值规则处理的数值、文本、List/Map/Table 等 |
| Memory / Region | 有运行时签发身份、按字节寻址、可增长的虚拟存储区域 |
| Place | 在表达式语义中可被写入的位置（例如局部槽、Memory 索引） |
| Pointer | 由区域引用和偏移构成的逻辑地址，不等于宿主裸指针 |
| Resource | 由可信平台授予的外部对象与权限句柄，不是普通内存 |
| Provider | 对某个授权、版本化的低层资源接口提供具体 OS/设备实现的组件 |
| Stdlib | 用 `.mq.md` 为主编写、随发行包提供、可独立升级的高级库 |
| Computational Closure | 仅靠已批准 Core + 既有权限端点，能在 `.mq.md` 自行构造目标功能的可检验性质 |
| HostOp | 当前 Rust Host 中的固定操作枚举；迁移中保留兼容路径，最终不再承载高层标准库增长 |

## 附录 B. 依据与可追溯参考

**Marqdo 当前代码（固定提交）**：

- [`spec/evaluation.md`](https://github.com/cflmy/marqdo-cli/blob/a113d894962d8c00eb02efb42e0e73f8b084a1fa/spec/evaluation.md)
- [`spec/expression.ebnf`](https://github.com/cflmy/marqdo-cli/blob/a113d894962d8c00eb02efb42e0e73f8b084a1fa/spec/expression.ebnf)
- [`spec/mapping.md`](https://github.com/cflmy/marqdo-cli/blob/a113d894962d8c00eb02efb42e0e73f8b084a1fa/spec/mapping.md)
- [`spec/references.md`](https://github.com/cflmy/marqdo-cli/blob/a113d894962d8c00eb02efb42e0e73f8b084a1fa/spec/references.md)
- [`spec/security.md`](https://github.com/cflmy/marqdo-cli/blob/a113d894962d8c00eb02efb42e0e73f8b084a1fa/spec/security.md)
- [`spec/plugin-abi.md`](https://github.com/cflmy/marqdo-cli/blob/a113d894962d8c00eb02efb42e0e73f8b084a1fa/spec/plugin-abi.md)
- [`spec/package-format.md`](https://github.com/cflmy/marqdo-cli/blob/a113d894962d8c00eb02efb42e0e73f8b084a1fa/spec/package-format.md)
- [`crates/marqdo-value/src/value.rs`](https://github.com/cflmy/marqdo-cli/blob/a113d894962d8c00eb02efb42e0e73f8b084a1fa/crates/marqdo-value/src/value.rs)
- [`crates/marqdo-runtime/src/builtins_impl.rs`](https://github.com/cflmy/marqdo-cli/blob/a113d894962d8c00eb02efb42e0e73f8b084a1fa/crates/marqdo-runtime/src/builtins_impl.rs)
- [`crates/marqdo-runtime/src/interpreter.rs`](https://github.com/cflmy/marqdo-cli/blob/a113d894962d8c00eb02efb42e0e73f8b084a1fa/crates/marqdo-runtime/src/interpreter.rs)
- [`crates/marqdo-host/src/host.rs`](https://github.com/cflmy/marqdo-cli/blob/a113d894962d8c00eb02efb42e0e73f8b084a1fa/crates/marqdo-host/src/host.rs)

**对照的外部架构原则**（只用于解释取舍，不代表 Marqdo 已实现）：

- [WebAssembly Core 2 工作草案（2026-10-09）](https://www.w3.org/TR/2026/CRD-wasm-core-2-20261009/)：抽象线性内存、`memory.grow`、越界访问陷阱、内存复制及可观测执行语义。具体规范版本应在 ADR 中固定，**不要求 Marqdo 逐字复制 Wasm**。
- [Rust `core`/`alloc`/`std` 文档](https://doc.rust-lang.org/std/)：分层区分核心计算、堆分配和平台能力，不要求所有高级库均禁止原生实现。
- [WASI Component Model 官方文档](https://component-model.bytecodealliance.org/design/wit.html)：描述外部接口与资源交互的类型契约；本 RFC 只参考原则，不强制引入完整组件模型。
- [Linux 用户态 ABI](https://docs.kernel.org/admin-guide/abi.html)：真实操作系统接口与平台限制是不可消除的边界，不能当作无权限的语言内置操作。
- 上一份设计基线：`MARQDO_COMPUTE_MACHINE_RFC_0001.md`（本次对话已创建的原始计算模型 RFC）。

---

**状态总结**：本文件已完成架构论证、组件划分、迁移分类、复杂场景闭包矩阵、逐阶段实施门禁和自动验收设计；**尚未修改仓库代码、尚未创建标准库仓库、尚未运行提议中的新功能测试、尚未批准任何 ADR**。
