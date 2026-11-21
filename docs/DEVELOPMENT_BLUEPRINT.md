# Marqdo CLI：语言规范、架构与开发总纲

> **状态**：架构基线提案 M0.5 / 新仓库实施蓝图（不是已实现功能清单）  
> **面向**：语言设计者、编译器/运行时开发者、插件开发者、贡献者及自动化编码智能体  
> **日期**：2026-11-20  
> **建议仓库位置**：`docs/DEVELOPMENT_BLUEPRINT.md`  
> **优先阅读**：本文件 → `spec/` 形式规范 → `tests/conformance/` 金样 → 相关模块源码  
> **约定**：**[D]** 已确定的设计方向；**[P]** 本文提出的可实现细节，须经形式化测试和 ADR 批准后冻结；**[F]** 后期能力，不应阻塞首个独立运行版本。

> **语言映射状态（2026-11-21，必读）**：本文档原本是 M0.5/M0.6 期的架构蓝图，其中关于 Markdown 程序角色的描述（旧映射：`Strong`=执行、`Emph`=返回、有序列表=分支、无序列表=循环、引用块=调用、首个独立 CodeSpan=签名）**已被废止，仅作为历史设计记录**。现行唯一语言语义权威是
> [`docs/architecture/RFC-LANG-0002-unified-semantics.md`](architecture/RFC-LANG-0002-unified-semantics.md)（由 [ADR-0052](adr/0052-unified-markdown-semantics.md) 采纳），其规范摘要为 [`spec/mapping.md`](../spec/mapping.md) 与 [`spec/expression.ebnf`](../spec/expression.ebnf)。
> 本文 §0.1、§1.2–1.4、§2.2、§2.4、§3.1–3.2、§4.1、§5.2、§6.2、§8.1、§13、§14、§16.3、§17 已按现行映射改写或标注历史；其余章节（IR、ABI、安全、包管理、发布）仍是架构参考。
> **实现状态**：RFC 与 `spec/` 领先于 Rust 实现。当前前端、表达式解析器与标准库仍实现旧映射，执行源码的测试预期为红；见 [`docs/STATUS.md`](STATUS.md) 与 [`docs/migration/README.md`](migration/README.md)。不要把本蓝图或 RFC 示例当作现有 CLI 已经支持的证据。
> **示例标注**：本文未明确标注为“历史”的 Markdown/`.mq.md` 示例均按 RFC-LANG-0002 目标语法书写，属于示意片段，前端迁移前不能原样运行；标注为历史的片段保留旧映射，仅供决策考古。

**这不是对 `cflmy/marqdo` 的兼容性重构。本项目必须从语言目标出发，独立建立 Marqdo 工具链。** 旧仓库只作为经验、算法、测试和可能的代码复用来源；旧语法、旧 ABI、旧插件体系不因名称相同而自动成为新版本的兼容契约。

> **规划文档关系（2026-11-21 更新）**：语言语义只以 RFC-LANG-0002 与 `spec/` 为准。下一阶段工作与发布门槛以 [`docs/STABLE_RELEASE_IMPLEMENTATION.md`](STABLE_RELEASE_IMPLEMENTATION.md) 为准；`docs/CORE_STABILIZATION_AND_ECOSYSTEM_GATE.md`（审查基线 `0adebe4`）、`docs/NEXT_PHASE_IMPLEMENTATION.md`（M0.6）与 `docs/NEXT_RELEASE_CI_CONVERGENCE.md` 是历史规划输入，不表示已实现，也不能覆盖 RFC 的语言映射。当前执行状态见 [`docs/STATUS.md`](STATUS.md)。

---

## 0. 开发者进入项目后首先应理解的十个决定

1. **[D] Markdown 是语言的语法，而不只是程序的注释格式。** Marqdo 的合法源码是选定 Markdown 方言的严格子集；不要求任意 `.md` 自动变成可运行程序。
2. **[D] 标准 Markdown AST 是唯一的文档结构事实来源。** 使用成熟解析器；不用正则表达式重新识别 Markdown 块/强调/列表。可以构造程序 IR，但不能平行维护另一套“私有 Markdown AST”。
3. **[D] Markdown 节点的程序角色是确定的。** 标题上的 `@exec`/`@knowledge` 声明（或从最近祖先标题继承）决定该区域是执行还是知识；执行区域内 `Strong`、`Emph`、行内 `Code` 是等价的单语句载荷，`if`/`else`/`while`/`end`/`return`/`params` 决定控制与签名，列表、引用块和段落只是容器。解析失败即诊断；禁止“像代码就执行，否则当文本”的启发式。现行权威为 RFC-LANG-0002；旧“`Strong`=执行、`Emph`=返回、列表=分支/循环、引用块=调用”映射已废止。
4. **[D] 自然语言可与执行节点同处一个段落或分支。** 叙述永远不触发执行，任何中文/英文词语都不能被猜成控制关键字。
5. **[D] Marqdo 自主执行，不翻译成 Python 作为正式后端。** 用 Rust 实现表达式求值、词法作用域、函数调用、返回、分支、循环和运行时错误；初期 tree-walk，后期可加字节码。
6. **[D] 表格、数学公式、代码围栏是原生文档值。** 公式计算和外部语言执行是 CLI 内置能力，但读取或渲染文档不会自动求值/执行它们。
7. **[D] 函数级引用是最小知识基础设施。** 文档链接、模块依赖、函数调用、公式引用是不同类型的边；链接绝不隐式执行。
8. **[D] 保留 YAML frontmatter 的显式导入。** 项目包管理采用项目清单、锁文件、环境同步；可以静态推断依赖图，不默认猜测缺失导入。
9. **[D] 安全和扩展性从第一天进入架构。** 能力式 Host API、资源预算、进程隔离、第三方插件 ABI 与 SDK 均是正式工具链的组成部分。
10. **[D] 先证明最小闭环，再扩展知识编译/智能体/IDE。** 高级知识证明、网页前端和自动化智能体不能阻塞语言运行时。

### 0.1 设计理念与取舍

Marqdo 想要实现的不是 Python 的 Markdown 版本，也不是 Jupyter Notebook 的另一种封装，而是**让标准 Markdown 结构直接承担程序作用域与载荷承载**：标题用 `@exec`/`@knowledge` 声明语义模式并沿祖先继承，只有 Exec H2 是可调用函数；执行区域内粗体、斜体与行内代码是等价的单语句载荷，程序操作由载荷中的 `if`/`else`/`while`/`end`/`return`/`params` 关键字决定；有序/无序列表与引用块只组织阅读顺序，不产生指令。普通 Markdown 阅读器仍能渲染源文件。（上述为现行 RFC-LANG-0002 映射；本蓝图早期“粗体承载执行、斜体承载返回、有序列表承载分支、无序列表承载循环、引用块承载调用”的表述属于历史设计，已废止。）

**不追求**“任意人类叙述皆可编译”；**不追求**“每种 Markdown 强调方式都仍有普通文档原义”；**不追求**“每一个 Markdown 节点都必须执行”；**不追求**“复杂程序完全不需要学习计算表达式”。

成功标准有四个：形式语义确定、通用计算能力、源码与渲染结果可读、生态与安全可工程化。

### 0.2 当前仓库关系与授权边界

- 当前已存在的 `cflmy/marqdo` 有值得参考的模块：`src/formula/mod.rs`、`src/foreign/*`、`src/host/foreign*`、`doc/design/module-namespace.md`、`doc/design/ambient-parameters-evaluation.md`、`include/marqdo_abi.h`、`crates/marqdo-plugin-sdk/`。
- 这些模块的历史实现与文档反映了已有经验，**不代表新设计已经通过测试**；不得复制旧的启发式解析、权限绕过、隐式命名空间合并或不稳定语义。
- 未经仓库所有者明确要求，不修改、合并、发版旧仓库。新仓库的语义版本与插件 ABI 单独编号。

---

## 1. 形式边界：复用什么，不复用什么

### 1.1 Markdown 方言

**[P] 基线**：CommonMark 0.31.2 + 固定启用的 GFM 表格、删除线、任务列表 + 明确配置的数学公式扩展。解析器首选 Rust `comrak`（备选 `pulldown-cmark`，选择必须由 AST 保真、源码定位、数学扩展和行为测试决定）。可渲染的 LaTeX 不等于可计算的公式。

- `Document`、`Heading`、`Paragraph`、`Text`、`Strong`、`Emph`、`Code`、`List`、`Item`、`BlockQuote`、`Link`、`Table`、`CodeBlock`、`Math` 等节点由选定解析器产生。
- **结构等价性要求**：如果两个输入在我们确定的 Markdown AST 视图中相同，除源位置/字面量格式无关的元信息外，其运行语义必须相同。`**x**` 与 `__x__` 不能被赋予不同的指令意义。
- `YAML frontmatter` **不是 CommonMark 内置节点**。仅允许在文档字节零位置（可容忍 BOM 的规则需测试）按已规定界线解析一次 YAML 元数据，再将剩余正文交给 Markdown 解析器；保留源位置偏移。不能让一个普通文档分隔线被误认为元数据。
- `$$` 数学节点是特定解析器扩展，必须在 `spec/markdown-profile.md` 锁定完整的开启选项；不允许随解析器升级隐式变更。
- 单个解析器可能在 AST 中合并相邻引用行、把 Setext/ATX 标题统一为 `Heading`、把不同项目符号统一为 `BulletList`。语言语义针对**AST 节点**，规范书写使用固定符号，但不能依赖丢失的源码标记。
- 不从渲染后的 HTML 反推程序。输入 Markdown AST 是唯一解析来源，源位置信息应被保留用于 IDE 和诊断。

### 1.2 文档区域与执行区域（RFC-LANG-0002）

**[D] 标题模式**：H1–H6 的首个纯 Text 子节点可以以 `@exec` 或 `@knowledge` 加空白与非空名称开头，声明该标题范围的模式；未声明的标题继承最近祖先标题的模式，文档根为 Knowledge。模式声明不属于语义名称，`@exec`/`@knowledge` 不进入可调用名称。`# @exec 名称` 建立执行命名空间，`## @exec 名称` 建立**唯一可调用函数**；H2 Knowledge 只是知识单元，H3–H6 只是分节，永不创建可调用函数。

**[P] 执行上下文限定**：只有 Exec 范围（继承或显式）中的 `Strong`、`Emph`、行内 `Code` 才是载荷候选；每个闭合载体承载**恰好一条语句**，载体种类不决定操作。标题文字、链接标签与 URL、图片 alt、表格数据单元、代码围栏、数学、HTML、front matter 等惰性叶节点中的内容绝不执行。列表、引用块和段落只决定载荷的文档顺序，不赋予 Branch/Loop/Call 角色。

**[D] 函数范围与入口**：只有 Exec H2 产生函数；其直接正文与后代 Exec H3–H6 分节按源序拼接为同一函数体，并在每个标题边界重置并要求控制栈为空。`main` 必须是 Exec H2 才能作为 `marqdo run` 默认入口；`marqdo call file.mq.md 函数名` 选择明确目标。无任何显式 `return` 的函数自然结束返回 `Unit`，不等于 `None`。

### 1.3 核心 Markdown AST 映射（RFC-LANG-0002；强制无歧义）

| Markdown AST 节点 | 当前程序意义 | 必须满足的条件 | 非法时 |
|---|---|---|---|
| `Document` | 模块，根模式为 Knowledge | 文档根 | 编译错误 |
| `Heading(level=1)` | 命名空间/顶层单元（声明或继承模式） | 非空规范名称 | 编译错误 |
| `Heading(level=2, mode=Exec)` | **唯一可调用函数** | 非空可寻址名称；作用域唯一 | 编译错误 |
| `Heading(level=2, mode=Knowledge)` | 惰性知识单元 | 非空名称 | 不执行 |
| `Heading(level>=3)` | 所属函数/知识单元的分节（可本地覆盖模式） | 必须位于合法祖先范围 | 不创建函数 |
| `Text` | 叙述 | 不参与求值 | 不报错 |
| `Strong` @ Exec | 单语句载荷载体 | 直接 Text/Code 子节点内容恰好一条语句；嵌套载体/复杂子节点报错 | **硬错误，不降级为强调** |
| `Emph` @ Exec | 单语句载荷载体 | 与 `Strong` 完全等价 | **硬错误** |
| `Code` 行内 @ Exec | 单语句载荷载体 | 与 `Strong`/`Emph` 完全等价；不享有签名特权 | 载荷非法时报错 |
| `Code` 行内 @ Knowledge/惰性区 | 文档代码文本 | 不执行 | 不报错 |
| `OrderedList` / `BulletList` | 纯文档容器 | 按源序递归提取其中的合法载荷；列表属性不影响指令 | **不产生隐含分支/循环** |
| `Paragraph` | 纯文档容器/载荷载体宿主 | 按源序提取合法载荷；纯叙述零指令 | 不报错 |
| `BlockQuote` | 纯文档容器 | 按源序递归提取合法载荷；引用本身不是调用 | **不产生隐含调用** |
| `Table` | 结构化值 | GFM 合法数据表；单元格内容不执行 | 类型/约束错误 |
| `CodeBlock` | `Code` 值 | 识别语言标记；未知语言允许存档 | 请求运行未知语言时报错 |
| `Math` | `Formula` 值 | 可渲染，不承诺均可符号解析 | 请求计算不受支持时诊断 |
| `Link` | 静态知识引用 | 可解析内外资源；普通外链不自动加载 | 按引用策略诊断 |
| `Image` / `Footnote` | 文档资源/来源 | 惰性 | 不执行 |
| `ThematicBreak` | 视觉分隔 | 惰性 | 不执行 |

**关于保留标记的代价（RFC-LANG-0002）**：在 Exec 范围内 `**自然语言强调**`、`*普通斜体*` 和 `` `任意代码` `` 都会被当作一条语句载荷；若无法解析为合法语句，就是硬诊断，不会被猜成排版。需要纯视觉强调时，放进 `@knowledge` 分节或其他惰性区域。列表和引用块不再被强制解释为分支/循环/调用，普通文档结构可以在函数内自由使用。

**关键消歧约束**：不要采用“在 `Strong` 中能解析成语句才是执行，否则不执行”的规则。AST 节点类型及其模式/位置决定它是否是载荷，载荷必须按唯一文法完整解析；控制操作只由载荷内的关键字决定，不能由载体种类或容器种类决定。错误必须立即报告。旧的 Strong=执行/Emph=返回/List=分支/Quote=调用规则**不得**再作为现行设计。

### 1.4 Markdown 结构对齐的困难问题（必须写进测试；RFC-LANG-0002）

1. `Strong`/`Emph`/行内 `Code` 在 Exec 范围内都是单语句载体，彼此等价；必须提取完整外壳节点且只提取一次。嵌套 `Strong`/`Emph`/`Link`/数学等复杂子节点报 `sem.payload_nested`，不得部分执行或静默拼接。
2. `OrderedList`/`BulletList` 只是容器，`ordered`/`start`/`tight`/任务框选等属性不影响指令；编译器按源序递归提取其中载荷，没有条件头、没有隐含分支或循环。空投影（零语句）不等于产生 `List([])` 值。
3. `BlockQuote` 只是容器，没有调用语义；其中合法的独立载荷照常执行，纯引用正文零指令。旧 `>` 自动调用规则已废止。
4. 标题模式声明只在标题首个纯 Text 子节点以精确 `@exec`/`@knowledge` 开头时生效；大小写变体、拼错、冲突声明、空名称都是硬错误，不可退回祖先模式。Setext 标题只继承模式，不携带声明。
5. 标题内部的 `Strong`、表格单元格中的 `Strong`、链接标签中的载荷绝不执行；实现必须基于上下文访问器而不是无条件遍历全树。
6. Markdown `*`/`**` 仍是强调界定符。表达式中的乘方 `**` 只在载荷已提取后具有运算符意义；为避免 Markdown 吃掉运算符或把载荷切碎，推荐把表达式放进行内 CodeSpan（例如 `` `n = n ** 2` ``）。检查器必须直接验证最终 AST，不能靠原始正则。
7. CommonMark 的“紧凑/松散列表”、换行、嵌套列表及标点有复杂规则；定义的是**AST 的结构匹配**，而不是缩进空格个数。提供 `marqdo inspect --ast` 展示实际节点。
8. 数学公式与代码围栏**不因为位于 Exec 标题内部而执行**。行内 `Code` 是载荷载体，围栏代码块是惰性 `Code` 值，两者不可混同。
9. 控制块（`if`/`while`）只能在同一标题的直接正文内闭合；跨 H3/H4 或跨越 Knowledge 分节的 `end` 必须分别报 `sem.unclosed_control` 与 `sem.unmatched_end`，不得跨边界“修复”。

---

## 2. 函数契约：标题、形式参数、调用、返回与生命周期

### 2.1 标题与名称

**[P]** 函数签名与标题脱钩。标题推荐 `## 计算圆面积`，不推荐 `## 计算圆面积(r)`，以免 API 参数调整影响标准 Markdown 锚点。

- 标题允许 Unicode（NFC 规范化）、中文及空格，保留对人可读的名称；空名、同一作用域规范名称冲突报错。
- **普通直接调用**仅允许表达式标识符可表示的名称，例如 `面积(r=2)`、`math.计算面积(r=2)`。带空格的自然标题通过 Markdown Link 目标引用，不能把空格当成实参分隔。
- 符号 ID 是 `模块规范身份 + 标题路径 + 函数名`，同时记录对应 GitHub 风格标题锚点用于 Markdown 导航。不要以单独的 slug 作为内部唯一标识，尤其重复名称或 Unicode 归一化时。对含同层重名的公共可引用单元首版直接报错；不要偷偷用 `-1`、`-2` 改名。
- 标题换名可能破坏外部链接，属接口变更；需 `check --refs` 与后续 `rename` 工具。参数改动不应改变标题锚点。Stable-ID/显式锚点未来可扩展，但不能依赖隐形注释猜测身份。

### 2.2 可选形式参数声明

**[D]** 默认允许从执行表达式静态推断参数，**不能**从自然语言、行内代码提及、标题措辞推断。**[P]** 若需要稳定的公开 API，在函数体第一条直接执行载荷写 `params(...)` 声明（RFC-LANG-0002）：

```markdown
## @exec 圆面积

`params(r: Number, precision: Int = 2)`

按照圆面积公式计算。

`结果 = pi * r * r`

`return 结果`
```

**固定规则（RFC-LANG-0002）**：`params(...)` 只能作为 Exec H2 的**第一条直接执行载荷**、最多一次、且不得位于列表或引用块内部；它是编译期元数据，不是运行时调用。其他位置的 `params` 报 `sem.signature_position`。允许必选形参、类型注解、编译期常量默认值；首版不支持可变参数、解构形参、运行时默认表达式。旧“函数标题后的独立 CodeSpan 即签名”规则已废止。

### 2.3 静态参数推断（必须区分于动态作用域）

候选形参来自对执行表达式的**自由读取**，排除已知内置、当前导入绑定、模块常量和已确定局部变量。实现必须经过函数 CFG 的数据流分析，不能只扫描文本或由 AST 访问顺序决定接口。

- 初期区分 `Read(x)`, `Assign(x)`, `Call(f)`，计算入口可达的自由读取集与确定赋值集；对 `x=x+1`，若 `x` 在入口没有绑定，允许推断 `x` 为参数。
- 变量在一些路径上已赋值、另一些路径上未赋值，若其身份无法唯一决定，**报告歧义**，不偷偷补一个隐式形参。
- 已经推断的参数按**规范符号名排序形成稳定 ABI/检查元数据**，但推断签名的函数**仅允许具名实参**，不允许依赖排序使用位置实参；显式签名可按声明顺序使用位置实参。
- 未绑定的自由读取名称可能是拼写错误；必须通过 `inspect --signature` 可见，`check` 对导出函数推断签名变动和可疑新参数发出诊断；CI 可通过 `interface.lock` 进行 ABI/API 兼容性校验。
- 导入后不得根据某库是否“刚好有同名变量”改变本地函数推断结果。模块引用解析先于参数推断，并由显式 YAML 声明。
- 不采用动态作用域；被调用函数不能自动访问调用者的同名局部变量。

### 2.4 调用、实参、返回

统一调用形式：`函数(...)`；零参数仍写 `函数()`；位置参数只对显式签名开放，具名实参允许覆盖声明顺序，禁止重复实参、未知实参、多余位置参数。参数表达式**按源码出现顺序、恰好求值一次**，再绑定到被调用函数的形参槽；调用者帧和被调用者帧隔离。默认实参值仅在缺失时使用，显式 `None` 不等于未提供。参数绑定失败必须按规范传播错误。

- `> **打印("你好")`：调用来自载荷内的表达式，引用块本身不是调用；执行后结果丢弃，必要时由执行记录保留。
- `**面积 = 计算圆面积(r=3)**`：在粗体载荷中的表达式调用，保存返回结果；也可用 `` `面积 = 计算圆面积(r=3)` ``。
- `[计算面积](./math.mq.md#计算面积)`：只是静态引用；跨模块调用必须在显式导入/锁定闭包内以表达式调用完成，需对 Link 目标与后缀参数进行 AST 级匹配，不按行字符串拼接。
- `**return 面积**`、`*return 面积*` 或 `` `return 面积` ``：显式 `return` 退出整个 H2 函数，嵌套在分支或循环中也向外传播到函数帧；斜体/粗体本身不再表示返回。
- 函数自然结束返回 `Unit`；`None` 是显式空值，`Unit` 是无结果，二者必须区分。
- 初期**不支持同名重载**；默认值、命名空间和语言类型均不参与模糊候选选取。支持函数递归与互递归。

### 2.5 函数值与可调用对象 [P]

函数是可以取得引用的一等值，但初期不强制开放匿名 lambda 语法。`CallableRef` 可指向模块中已经命名的函数，支持作为 `map`/`fold` 等高阶函数实参。悬空或跨版本失效引用报错；跨模块函数值保留源模块词法环境，不捕获调用者局部变量。闭包与匿名函数推迟到完成核心参考解释器后评估。

---

## 3. 执行语义：表达式、控制流、错误与通用计算

### 3.1 表达式是小型独立文法，不是第二套 Markdown

**[D]** Markdown AST 确定“哪里可以执行”；每条执行载荷的文本使用一个**独立、严格、很小的语句/表达式解析器**（现行文法见 `spec/expression.ebnf`，权威为 RFC-LANG-0002）。不得把整个 `.mq.md` 转译成 Python。载荷必须恰好是一条语句并被完整消费：

```ebnf
payload     = statement , EOF ;
statement   = if_stmt | else_stmt | while_stmt | end_stmt
            | return_stmt | params_stmt | assignment | expression ;
if_stmt     = "if" , expression ;
else_stmt   = "else" ;
while_stmt  = "while" , expression ;
end_stmt    = "end" ;
return_stmt = "return" , expression ;
params_stmt = "params" , "(" , [ parameter_list ] , ")" ;
assignment  = assign_target , "=" , expression ;
```

- `if`/`while` 由显式 `end` 闭合，`else` 可选且最多一次；控制配对由前端独立控制栈完成，不属于表达式文法。没有 `elif`/`for`/`break`/`continue`/`try`/`end if`/分号多语句/行尾冒号。`if`/`while`/`return` 需要非空表达式并消费整个载荷。
- 表达式部分沿用既有优先级、字面量、调用、索引、赋值与 `Memory` 位置写入；`**` 只在已提取的载荷内是乘方符号，Markdown 强调边界仍由 Comrak AST 决定，必要时用行内 CodeSpan 保证载荷保真。
- 数值初期选择 `Int`（任意精度或严格有界的语义必须明确，**推荐任意精度 BigInt**）、`Float64`、`Bool`、`Text`、`None`、`Unit`、`List`、`Map`、`Table`、`Formula`、`Code`、`FunctionRef`、`Result`、`PluginHandle` 等。跨 ABI/JSON 边界保留类型标签和大整数的字符串形式，禁止 silently 精度损失。
- 只有 `Bool` 可用于条件。`and`/`or` 短路；数值除零、类型错误、未定义变量、越界均是结构化错误；禁止隐式字符串数字转换。字典访问和数组索引的越界语义必须标准化。
- 赋值为当前局部绑定更新，不隐式写入外层作用域；模块导入绑定只读。

### 3.2 控制流与程序区域（RFC-LANG-0002）

**载荷关键字，不是 Markdown 类型**：每条 Exec 载荷是一条语句；`if <expr>` 打开 `Branch`，`else` 切换最近尚未关闭的 `if` 的 else 分支，`while <expr>` 打开 `Loop`，`end` 关闭最近的 `if`/`while`，`return <expr>` 产生 `Return` 并退出整个 H2 函数。`else`/`end` 不产生运行时指令。控制栈在标题边界必须为空，`if`/`while` 不得跨标题闭合。

**容器无语义**：`Paragraph`、有序/无序 `List`、`BlockQuote` 只是按源序递归提取合法载荷；`1.` 与 `-` 的替换、`>` 的包裹、列表紧/松属性都不改变指令。空容器投影为零语句，不是 `List([])` 值。

**求值顺序**：同级块按 Markdown AST 文档顺序；段内载荷按源序；分支只执行命中臂；循环条件每轮重新求值；参数按源码出现顺序求值一次；函数帧隔离；导入模块无副作用初始化。错误传播停止当前控制流并保留调用栈/源码位置。

**基本示例**（RFC-LANG-0002 目标语法；前端尚未迁移，不能原样运行）：

````markdown
# @exec 数学

## @exec 阶乘

数学定义为：

$$
n! = \begin{cases}1,& n\le1\\n(n-1)!,& n>1\end{cases}
$$

`if n <= 1`

`return 1`

`else`

`return n * 阶乘(n=n-1)`

`end`

## @exec 倒计时

`n = 3`

`while n > 0`

`打印(n)`

`n = n - 1`

`end`

`return n`
````

上述阶乘函数通过自由变量推断形参 `n`（公开 API 推荐用 `params(n: Int)` 显式声明）；倒计时的 `n` 在入口赋值后才读取，属于局部变量，不应被推断成参数。旧版“列表条件头 + 斜体返回”的示例属于历史设计，见 RFC-LANG-0002 §1.4 的迁移对照。

### 3.3 终止与图灵完备性

- **理论模型**：可变存储、条件选择、while 循环（或递归）和理论上无界的整数/容器足以表达通用计算。需要给出到二计数器机或等价模型的编码证明，不能把“可以运行 Python 围栏”作为语言本身图灵完备的证据。
- **工程模型**：真实执行受 fuel、栈深、内存与时间限制；资源限制不会取消抽象语言的计算表达能力，必须如实区分理论与实现。
- 引用解析、插件调用、外部数学库均**不是**证明图灵完备性的前置条件。

### 3.4 错误模型与类型

统一 `Diagnostic { code, severity, span, message, related, hints }` 与 `RuntimeError { category, span, frames, cause, effect_trace }`。分类：`markdown.invalid_construct`, `expr.parse`, `symbol.unknown`, `symbol.uninitialized`, `call.arity`, `call.type`, `module.unresolved`, `ref.unresolved`, `math.unsupported`, `kernel.unavailable`, `permission.denied`, `plugin.abi_mismatch`, `resource.exhausted` 等。CLI 保证纯文本/JSON 两种输出，CI 可依赖机器可读错误代码而非错误措辞。

---

## 4. 引用与最小知识索引：优先保证可寻址，不先许诺自动证明

### 4.1 三类边要分开

| 边 | 来源 | 何时解析 | 是否执行 |
|---|---|---|---|
| `Import` | YAML `import` | 模块加载/静态编译 | 否，模块只注册定义 |
| `Reference` | 普通 Markdown `[说明](目标)` | 文档索引阶段 | 否 |
| `Call` | Exec 载荷中的表达式调用 | 静态解析并在执行时调用 | 是（函数有运行时行为） |

`[牛顿第二定律](#牛顿第二定律)` 只是引用；明确调用必须在 Exec 载荷中写出表达式，例如 `` `print(计算作用力(m=2, a=3))` ``。引用块本身不再是调用，`> [计算作用力](#计算作用力)(m=2,a=3)` 若不处于 Exec 载荷内则只是文档引用。跨模块调用须在固定的模块/文件权限下解析，外网 HTTP 链接绝不通过导入自动下载和执行。外部文档链接可以纳入可选的网络知识索引，但**不得**改变本地程序求值。

### 4.2 规范身份与兼容

- 内部 `ModuleId`：项目根相对的规范路径、所属包身份与锁定版本/内容哈希。
- 内部 `UnitId`：模块 + 规范标题路径 + 标题规范名；链接原文及 GitHub/渲染器用的片段 slug 单独维护。
- `Reference` 边记录 `source_unit`, `target_uri`, `target_unit?`, `source_span`, `kind`, `resolved`, `source_hash`。
- 同作用域重复标题、规范化后重名、循环文件导入须诊断。正文标题重命名可能造成断链，`check --refs` 必须检测；M0.5 不冒充永不变化的 UUID 系统。
- URI 解析路径必须防止穿越项目根；依赖包内路径仅能访问该包已安装根，绝不从 Markdown 链接自动跨越权限边界。
- 知识引用图可用于 `inspect --graph`、IDE 导航、增量检查和未来知识编译，但 `Reference` 不隐式生成形式证明义务。

### 4.3 形式知识单元的扩展边界

M0.5 的“知识单元”只等价于**可寻址标题范围 + 其正文 AST + 可选类型化资源 + 引用关系**；暂不承担命题逻辑、自动推理或证明证书。后续如果引入知识证明，需要新的独立规范、明确的证明/假设边界与可信检查器，而非把文本看起来像定理就视为已证明。

---

## 5. 原生 Math：让公式可保存、可引用、可计算

### 5.1 三个层次

1. `Math` AST：渲染层，允许选定数学扩展支持的任意排版公式，绝不自动执行。
2. `Formula` value：结构化原文、来源、变量及可选符号 AST；只有处于可解析的数学子集才获得 `parsed` 状态。
3. 数学内核：`parse`, `substitute`, `simplify`, `eval`, `differentiate`, `solve`, `matrix`（逐项声明能力、前置条件与误差承诺）；不承诺支持所有 LaTeX/定理证明。

**关键数学语义**：`F=ma` 是 `Equation(lhs,rhs)`，不是赋值；不能仅因等式左侧有 `F` 就更新 Marqdo 变量。求值必须指明目标变量或函数/解算方向。支持精确有理数与浮点结果的区别、数值误差说明、计算限制和异常边界。

### 5.2 示例（候选 API）

````markdown
## @knowledge 圆面积公式

$$
A = \pi r^2
$$

## @exec 计算圆面积

依据[圆面积公式](#圆面积公式)代入半径。

`params(r: Number)`

`面积 = math.eval(ref("#圆面积公式"), {"r": r}, target="A")`

`return 面积`
````

`Formula` 可以按函数标题范围寻址。一个范围存在多段公式时，默认对象选择必须是明确规则（例如禁止无选择器，要求 `math.formulas(ref(...))` 列出候选；不选“第一条”）。数学可符号计算子集应该单独版本化。高级张量/量子数学可通过插件提供，不进入不可控的内核表面。

### 5.3 参考旧实现

现有 `src/formula/mod.rs` 有基础 AST、解析、化简、微分、求值、求解和矩阵处理经验；新的 Math 内核可以复用算法与测试，但应重新验证精度、语法边界、求解正确性和运行成本。尤其禁止把数值近似解宣称为形式定理证明。

---

## 6. 原生 Foreign Code：围栏是值，运行必须显式

### 6.1 类型与执行方式

`Code { language, source, source_span, owning_unit, content_hash }` 为一等值。Python、JavaScript 等标准围栏由 Markdown parser 原生识别；不是将 Marqdo 翻译成这些语言。

提供两种运行方式：

- `foreign.run(ref, input?)`：一次性隔离运行，结果包含 `status, stdout, stderr, exit_code, duration, value?, error?`；标准输出不是默认返回值 JSON。
- `foreign.call(ref, input)`：严格约定输入/输出协议，输入来自受控序列化，输出必须是协议定义的类型化结果；非法返回协议报错。

后期支持命名会话 `foreign.session`，Python/Node 内核中只允许**显式**传入或取回状态；不能默认共享 Marqdo 局部变量。执行不可自动改变 `.mq.md`，写回必须由显式操作和授权触发。

### 6.2 例子（候选 API）

````markdown
## @knowledge Python 校验

```python
import json, math, sys
args = json.load(sys.stdin)
print(json.dumps({"area": math.pi * args["r"] ** 2}))
```

## @exec main

`params(r: Number)`

`结果 = foreign.call(ref("#python-校验"), {"r": r})`

`打印(结果.area)`
````

不能因为这个文件里存在 Python 代码围栏就自动创建 Python 进程；当 `foreign.call` 被执行且权限通过后才创建或复用受控内核。

### 6.3 Foreign Environment

- Marqdo 自有解释器环境与 Python/Node 环境相互独立；可以检测 `uv`/`python`、`node`，但未安装时错误应可修复。
- Python 项目虚拟环境可由 `uv` 或 Python 标准工具创建，所需版本/锁文件独立记录；不要自研 PyPI 依赖求解器。
- 资源限制必须在 Foreign 子进程层执行；任何 `eval` / `exec`、Python 子进程及原生库都是**不可信执行入口**，不能依赖 cwd 隔离。

---

## 7. Rust 独立解释器：IR、值系统与执行内核

### 7.1 正式执行链

```text
.mq.md
  -> frontmatter extraction [metadata extension]
  -> CommonMark/GFM/Math AST [standard parser]
  -> UnitIndex/RefIndex [source-aware, immutable]
  -> SemanticMapper [AST structure -> named operations]
  -> ExpressionParser [only designated execution payloads]
  -> Resolver/TypeChecker/CFG/DataFlow
  -> Marqdo IR [Assign, Eval, Call, Return, Branch, Loop, ResourceRef]
  -> Rust Tree-Walk Interpreter [reference implementation]
  -> CapabilityHost [Math, Foreign, FS, Net, Plugin]
  -> Trace/Result/Diagnostics
```

编译器能生成内部的 IR 并不违背“只使用 Markdown AST”：**禁止的是重新发明 Markdown 文档语法，允许、且必须有自己的程序语义 IR**。Markdown AST 保存原始结构、内容及源码位置；IR 是已确定语义的执行计划，它应携带指回 Markdown 节点的 `NodeId`/`Span`。解释器不得直接遍历源码字符串猜测 Markdown 标记。

### 7.2 解释器内核

- `Value` 可用 enum 表示；`Frame { function, locals, parent_lexical_scope, return_address }`；`EvalOutcome { Value, Return(Value), Error, Cancel }`（示意，不能与用户层 Value 混淆）。
- 控制节点的求值实现由 Rust 自己维护，不把 AST 翻译成 Python AST，不使用 Python 的 `eval`。
- 递归调用受栈深预算；循环每次迭代消耗 fuel；复杂表达式的求值计入预算；取消与超时传递到 Host 调用。
- 有意引入 Bytecode 时实现第二后端并对同一规范金样做差分测试；优化改变性能，不能改变可观察语义。
- 纯函数/纯数学与 I/O/进程/插件的效应分类分开，能力检查在**实际 Host 调用处**执行，静态分析仅提供预警，不能替代运行时检查。
- `Run` 语义与未来 `View` 交互共享同一 IR/Runtime；IDE、Notebook 和 Agent 不得有第二套解释器。

### 7.3 最小运行时接口

```rust
// 示意，不是冻结的 Rust public API。
trait Host {
    fn invoke(&mut self, op: HostOperation, args: &[Value], ctx: &ExecutionContext)
        -> Result<Value, RuntimeError>;
}

struct ExecutionContext {
    /* source_span, module_id, trace_id, capability_token,
       resource_budget, cancellation, task_id ... */
}
```

宿主功能只能从 `Host` 入口进入，插件和外部内核不能持有任意未受控的宿主对象句柄。用户层错误使用独立 `Result` 或明确抛出机制，外部进程失败必须保留原因并可被捕获，不得直接导致解释器进程崩溃（进程内 native 插件除外，其内存安全风险见 §10）。

---

## 8. 项目、包管理、虚拟环境与依赖锁定

### 8.1 三种不同的文件/关系

**`marqdo.yaml` 项目清单**：包名、版本、Marqdo 运行时约束、依赖来源、特性、插件需求、项目请求的权限。**`marqdo.lock` 锁文件**：精确来源、版本、commit、哈希、传递依赖、平台制品哈希。**文档 frontmatter**：文件级 `import` 别名与模块地址。

````yaml
# marqdo.yaml
project:
  name: example
  version: 0.1.0
  marqdo: ">=1.0,<2.0"
dependencies:
  geometry:
    path: ./packages/geometry
  quantum:
    git: https://github.com/example/quantum-mq
    rev: 0123456789abcdef0123456789abcdef01234567
plugins:
  solver:
    version: "^1.0"
capabilities:
  fs:
    read: ["./data/**"]
    write: ["./output/**"]
  net:
    hosts: []
  process: false
````

````markdown
---
import:
  geometry: "@geometry"
  quantum: "@quantum"
---

# @exec 示例模块

## @exec main

`面积 = geometry.圆面积(r=3)`
````

`capabilities` 是**项目提出的权限请求**，不等于宿主已批准的权限；导入和安装不能自动赋权。

### 8.2 包管理规则

- 项目根发现：从入口文件目录向上寻找唯一的 `marqdo.yaml`；找到多个项目边界时取最近且明确诊断；支持 `--project` 指定。
- `marqdo init`, `add`, `remove`, `lock`, `sync`, `tree`, `update`, `vendor`（后期）等命令。在 CI 中 `--locked`、`--offline` 必须可用。`run` 不能静默升级锁定版本；`sync` 只能把环境对齐锁文件。
- 首期依赖来源：local path、固定 Git rev；注册表/包发布生态为后期独立项目。库导入始终绑定别名，禁止扁平合并所有命名空间与相同名称的静默覆盖。
- 模块/包在加载期只注册静态定义；禁止任意安装脚本和无授权模块初始化副作用。包签名、可信发布者、供应链扫描与撤销策略后期增强，但内容哈希、来源完整性检查是首版底线。
- 生成的 `marqdo.lock` 与依赖图必须是可重复的规范序列化。不要在锁文件里写入开发者机器绝对路径或不可重现的时间戳。
- 依赖解析与导入身份是同一模块系统的两层，不要出现“frontmatter 导入按路径解析，但包运行时按名称再猜测”之类双轨行为。

### 8.3 项目环境

```
project/
  marqdo.yaml
  marqdo.lock
  index.mq.md
  .marqdo/
    env/           # 项目已锁定依赖视图
    cache/         # 编译/解析缓存，可重建
    kernels/       # 可选 Python/Node 环境
    sessions/      # 本地运行记录，默认 gitignore
```

用户无需手动 activate；CLI 自动识别项目环境。全局内容寻址缓存可以去重复，但不能泄露可变的全局模块状态。虚拟环境解决依赖隔离，不解决安全；包管理/沙箱是两个不同的工程模块。

---

## 9. 安全：能力权限模型、资源预算与真正的沙箱

### 9.1 原则

1. **不可信 `.mq.md` 默认无外部副作用权限。** `check/inspect` 严禁执行文档或模块导入的副作用。
2. 项目清单只能**请求**权限；实际权限为宿主上限 ∩ 用户授予 ∩ 当前任务能力，默认拒绝。
3. 所有原生 Host API、Foreign 内核、插件调用都要在实际执行点检查能力；不能只在解析/安装时检查一次。
4. 限额覆盖 fuel、CPU 墙钟时间、内存、递归深度、外部进程数、输出字节、网络带宽、文件访问数；严格模式要在 OS 层 enforce 而不是仅依靠合作式检查。
5. 跨任务和跨模块权限只能传递更小或相等的能力，不应因导入库或插件请求而升级。
6. 不信任插件 JSON 或 ABI 元数据；限制输入长度、深度和输出大小；秘密默认脱敏且不可进入诊断/Trace。

### 9.2 模式与平台失败策略

| 模式 | 目标 | 安全保证（原则） |
|---|---|---|
| `pure` | 纯 Marqdo/Math | 禁止外部 I/O；只使用可信的内存计算与预算控制 |
| `project` | 用户批准的项目资源 | CapabilityHost 授权 + 有界资源；对 native 组件仍须正确标注风险 |
| `strict` | 未审查文档/代码/插件 | 独立受限进程、OS 级资源与系统调用/文件/网络限制；不可用则**拒绝运行** |

**关键安全事实**：cwd、路径规范化、`venv`、C ABI、Rust 语言本身都**不是**恶意原生代码的安全沙箱。Linux 可考虑 namespace + seccomp + cgroup 等，Windows 可考虑 Job Object 和适当的应用隔离策略；macOS 需要可用且可验证的相应隔离技术。平台能力不足时不能谎称 strict 安全。

**Wasm/WASI** 是运行第三方扩展的候选隔离形式，必须限制资源、宿主导入和 WASI 权限；不意味着任意 Python 进程或 native `.so` 立即安全。

### 9.3 授权与日志

CLI 可使用 `--sandbox strict --allow-read ./data --allow-write ./output` 等**操作级权限请求**，但用户提供的 CLI 标志是否可越过系统管理员策略取决于宿主授权上限。交互批准需有明确 TTY/Host 授权；CI/Agent 默认不得弹出允许全部权限的对话。每次 Host 操作记录来源 `ModuleId/Span`、能力、参数脱敏摘要、耗时、结果/错误及可复现条件。

---

## 10. 插件 ABI：必须让第三方开发者独立实现，但不可把 ABI 当沙箱

> **[D] 关键新增要求**：插件 ABI 和开发 SDK 是一等发行接口，而不是内置函数的私有回调；Rust、C/C++、Go、其他语言开发者能够编写独立插件。插件在新仓库具有自己的**独立 ABI 世代**，不能直接宣称兼容旧 `MARQDO_ABI_VERSION=2`。

### 10.1 两级插件模型

| 类型 | 首期支持 | 部署/调用 | 安全边界 |
|---|---|---|---|
| **C ABI native plugin** | **必须** | `.so` / `.dylib` / `.dll`，经稳定 C ABI 调用；受信任时可进程内运行 | 进程内不隔离内存，发生 UB/崩溃可带崩 Host |
| **Isolated plugin worker** | **必须至少实现基础原型** | 同样的插件 API，经独立受限 worker + 长度前缀 IPC 调用 | 依靠 OS 进程隔离，受平台能力限制 |
| **Wasm Component + WIT** | [F] 推荐方向 | WIT 定义跨语言接口，由 Wasmtime 实例化 | 更适合按 host import 授权、限额，仍需审计 |

不要只提供在主进程内 `dlopen` 任意第三方动态库的唯一入口；这会直接破坏 strict sandbox 的承诺。**默认第三方插件在隔离 worker 中运行**；进程内高速模式要求显式受信任授权。调用路径由同一个高层插件函数注册接口驱动，程序不必知道底层 transport。

### 10.2 新 ABI 的基本契约（建议 v1）

独立版本化三种东西：**语言版本（syntax/semantics）**、**插件 ABI major/minor**、**插件包 SemVer**。不同的版本号不能混用。ABI 主版本不匹配必须加载失败；次版本仅在能力协商后使用。

C ABI 仅允许稳定布局类型（整数、固定大小字段、长度+指针、不透明句柄和 `extern "C"` 函数指针）。**严禁**将 `Rust String/Vec/trait object/Result`、Rust panic、C++ exception 或 Go 对象指针直接跨 ABI 边界传递。异步调用首版不要求任意线程回调；严格限制重入与并发。

下列头文件是**规范骨架，不是已经编译测试的完整 SDK**：

```c
#ifndef MARQDO_PLUGIN_ABI_V1_H
#define MARQDO_PLUGIN_ABI_V1_H
#include <stdint.h>
#include <stddef.h>

#define MQ_ABI_MAJOR 1u
#define MQ_ABI_MINOR 0u

typedef struct {
    const uint8_t *data;
    size_t len;
} MqBytes;

typedef struct {
    uint8_t *data;
    size_t len;
} MqOwnedBytes;

typedef struct MqCallContext MqCallContext; /* opaque; host-owned */

typedef int32_t (*MqPluginInvoke)(
    MqCallContext *ctx,
    MqBytes args_wire,
    MqOwnedBytes *result_wire,
    MqOwnedBytes *error_wire
);

typedef struct {
    uint32_t struct_size;
    uint32_t abi_major;
    uint32_t abi_minor;
    void *host_data;
    /* host-owned alloc/free, buffer ownership documented separately */
    void *(*alloc)(void *host_data, size_t);
    void (*free)(void *host_data, void *);
    int32_t (*register_function)(
        void *host_data, MqBytes export_name,
        MqBytes signature_wire, MqPluginInvoke invoke);
    int32_t (*request_capability)(
        MqCallContext *ctx, MqBytes name,
        MqBytes request_wire, MqOwnedBytes *reply_wire);
} MqHostApi;

typedef struct {
    uint32_t struct_size;
    uint32_t abi_major;
    uint32_t abi_minor;
    MqBytes plugin_metadata_wire;
} MqPluginApi;

/* exported symbol; 0 = success */
int32_t mq_plugin_entry_v1(const MqHostApi *host,
                           MqPluginApi *plugin);
#endif
```

**这个草图中必须在首个 SDK 提交前补全的细节**：指针有效期、Host/Plugin 分配和释放所有权、输出缓冲区大小上限、出错时输出字段是否为 NULL、ABI 表向后兼容规则、线程模型、卸载/终止规则、回调取消、OOM 语义、数据表示和 endian/字长限制。`size_t` 使 ABI 依赖目标架构，包与插件制品必须按 target triple 发布；不可跨位宽直接加载。

### 10.3 参数与结果的 wire format

**[P] v1 选择**：在 ABI 与 worker IPC 中使用一个明确版本化的 UTF-8 JSON 包装协议作为最小通用交换形式（可借鉴旧 ABI 的 JSON 桥，但必须修复类型精度和扩展问题）。规范化格式示意：

```json
{
  "wire_version": 1,
  "kind": "request",
  "function": "solver.solve",
  "args": {"a": {"type": "int", "decimal": "12345678901234567890"}},
  "context": {"trace_id": "...", "capability_handle": "opaque"}
}
```

- `None`, `Unit`, `Bool`, `Int`, `Float`, `Text`, `List`, `Map`, `Table`, `Formula` 等必须逐个定义映射、循环引用拒绝规则、深度/大小限制。64 位以外整数不能简单变为 JS 不精确 Number；浮点 NaN/Infinity 和负零等特殊值使用显式标签或禁止。
- 密钥、文件句柄、OS 进程、内存指针不能序列化成可在别的进程任意重建的对象；只提供有范围/生命周期的不可伪造句柄。
- Host 能力请求必须由**宿主执行**；worker 不能拿到宿主真实 FS/Net 句柄之外的任意系统资源。
- 调用方接收结构化 `PluginError { kind, code, message, recoverable?, detail? }`；错误输出不得包含敏感数据；调用语义在进程内与 worker 之间保持一致。

### 10.4 插件清单与分发

````yaml
# plugin.yaml（示意）
plugin:
  name: qdqc.solver
  version: 1.0.0
  abi: "^1.0"
  kind: native
  exports:
    - name: solver.solve
      signature: "(equation: Formula) -> Result"
  requires:
    - math.compute
  artifacts:
    - target: x86_64-unknown-linux-gnu
      file: solver.so
      sha256: "<内容哈希>"
````

- 发行包包含清单、目标架构制品、内容哈希、许可证、SDK/ABI 声明和文档；安装不等于加载，更不等于授予权限。
- 导出函数名放进已绑定的插件命名空间，必须与普通函数调用和静态类型检查兼容；禁止静默覆盖内置函数、库函数或其他插件。
- 包管理需要能够处理缺失制品、平台不匹配、ABI 不兼容、能力不满足等清晰错误。
- 附带一个官方 Rust SDK（宏+安全封装）、C SDK/头文件、至少一个 C 示例与一个 Rust 插件示例；Go 等其他语言通过固定 C ABI 或隔离 worker 协议开发，不强求首期全部提供 SDK。

### 10.5 插件生命周期、版本与 host API

生命周期建议：`discover -> validate manifest -> resolve artifact -> verify hash -> negotiate ABI -> register exports -> authorize -> instantiate/load -> invoke -> shutdown`。只能在 `invoke` 时拥有有效的 `MqCallContext`，不可长期缓存用户权限令牌。卸载必须解决悬空函数指针、运行中调用、注册表清理等问题；首期**允许不支持进程内热卸载**，以宿主/worker 生命周期管理替代。

Host 能力 API 采取**白名单**，如 `log`, `math.compute`, `fs.read`, `fs.write`, `net.request`, `clock`, `random`, `artifact.record`；插件不能按任意字符串查询整个 Host 内部状态，不能绕过能力检查读取源码、密钥和用户工作区。跨进程 worker 采用同一逻辑接口但不同传输实现；不能让插件从 `MqHostApi` 取得裸 Rust 对象。

### 10.6 与旧 ABI 的关系

旧仓库 `include/marqdo_abi.h` 使用 ABI v2、`marqdo_plugin_init`/`shutdown`/`abi_version` 三个导出、JSON 调用以及 `register_fn/alloc/free/host_query`，并有 `marqdo-plugin-sdk`。它是可参考的实验依据，**但本蓝图提出独立的新 v1 接口**。只有经过兼容测试的显式 adapter 可以支持旧插件；不得在新宿主里直接把旧 ABI v2 当成新 ABI v1。

### 10.7 ABI 验收门槛

- C 与 Rust 编译器分别构建插件，宿主跨编译环境成功加载；Windows/Linux/macOS 至少 CI 覆盖目标平台的核心组合。
- 无效 ABI major、缺少符号、错位结构体、不受支持版本、错误缓冲区、超大 JSON、panic/异常、崩溃 worker、超时、循环依赖、权限不足等必须有确定结果。
- FFI fuzz/sanitizer 通过；native worker 崩溃不带崩 CLI；主进程内受信任插件崩溃属于已明确的信任风险。
- SDK 更新不能改变已冻结的 struct layout 和导出符号；提供 ABI 兼容矩阵、旧 SDK 构建插件回归测试。

---

## 11. 建议的仓库结构和模块边界

```
marqdo-cli/
├── README.md
├── AGENTS.md
├── Cargo.toml                 # Rust workspace
├── marqdo-cli/               # binary package, or crates/cli
├── crates/
│   ├── markdown/             # 选定 Markdown AST、source maps
│   ├── frontend/             # 标题范围、语义映射、表达式 parser
│   ├── sema/                 # 符号/作用域/参数推断/引用/CFG
│   ├── ir/                   # 独立语言执行 IR
│   ├── runtime/              # Rust tree-walk interpreter
│   ├── value/                # 类型、序列化、错误
│   ├── host/                 # capability dispatch, effect tracing
│   ├── math/                 # Formula parser + numerical/symbolic
│   ├── foreign/              # Python/Node kernel/bridge
│   ├── package/              # resolver, manifest, lockfile
│   ├── environment/          # project env and cache
│   ├── sandbox/              # worker/OS isolation adapters
│   ├── plugin-abi/           # Rust host bindings to C ABI
│   ├── plugin-sdk/           # third-party developer SDK
│   └── cli/                  # flags/command routing
├── include/
│   └── marqdo_plugin_abi_v1.h
├── spec/
│   ├── markdown-profile.md
│   ├── mapping.md
│   ├── expression.ebnf
│   ├── evaluation.md
│   ├── references.md
│   ├── value-wire.md
│   ├── package-format.md
│   ├── plugin-abi.md
│   └── security.md
├── docs/
│   ├── DEVELOPMENT_BLUEPRINT.md
│   ├── adr/
│   └── contributor-guide.md
├── tests/
│   ├── conformance/
│   ├── negative/
│   ├── properties/
│   ├── security/
│   ├── abi/
│   └── integration/
├── examples/
│   ├── hello.mq.md
│   ├── factorial.mq.md
│   ├── references.mq.md
│   ├── math.mq.md
│   ├── python-foreign.mq.md
│   └── plugin-demo/
└── .github/workflows/
```

**注意**：这是职责划分，并不要求第一天就拆成十多个可独立发布的 crate。最初可以是 4–6 个 crate：`syntax`, `core`, `runtime`, `host`, `cli`, `sdk`，待 API 稳定再细分。所有单元通过公共数据契约通信，不允许 CLI 直接修改解析器内部树，也不允许插件绕过 Host。

### 11.1 依赖方向

- `markdown ->` 零其他 Marqdo crate；负责标准 AST 的稳定视图。
- `frontend -> markdown, expression`；产生索引/IR，不能访问 Host。
- `sema -> frontend, ir`；静态分析与诊断，不能发起网络和进程。
- `runtime -> ir, value, host API abstraction`；不依赖 CLI、Markdown 解析器或具体 Python 实现。
- `host -> value, capability`；承载数学/外联/插件桥的请求分发；沙箱实现可替换。
- `package -> manifest/lock`；能解析包但不能运行源文件。
- `cli` 是组合根，负责命令行/配置/执行入口；不允许成为新的“巨型解释器”。

Rust crate 的实际依赖可以通过抽象接口和反向依赖避免环，最终以 `cargo tree` 检验。严禁 `runtime` 反向依赖 `cli`。

---

## 12. CLI 用户契约与工程体验

| 命令 | 第一期含义 | 核心保证 |
|---|---|---|
| `marqdo init` | 初始化项目 | 创建最小清单与入口 |
| `marqdo run [file]` | 独立执行入口 | 默认 `index.mq.md`，无 Python 依赖 |
| `marqdo call file function [--arg k=v]` | 调用指定函数 | 具名参数/类型校验、明确退出码 |
| `marqdo check [file]` | 静态检查 | 不执行文档、不加载第三方代码 |
| `marqdo inspect file` | 显示 AST/函数/参数/依赖/引用 | `--ast --signature --ir --refs --graph` |
| `marqdo fmt [file]` | 规范 AST 等价排版 | 不能改变可观察语义 |
| `marqdo test` | 运行测试夹具 | 测试进程权限最小化 |
| `marqdo add/remove` | 更新项目依赖 | 清单、lock 变更可审查 |
| `marqdo lock/sync/tree` | 锁定、同步、依赖树 | 可重复、离线可验证 |
| `marqdo plugin inspect` | 检查插件制品/ABI/能力 | 默认不加载执行插件 |
| `marqdo plugin add` | 安装插件依赖 | 安装不等于授权与加载 |
| `marqdo kernel add/sync` | 可选外部语言环境 | 使用外部包生态工具 |

所有命令提供 `--json` 输出（定义版本化 schema）、非零错误退出码、`--offline` 语义和可预期帮助信息。`run` 的 stdout 保留程序的标准输出，诊断默认写 stderr；JSON 输出须与程序 stdout 分通道，避免混杂解析失败。

### 12.1 核心命令试运行路径

```bash
cargo build -p marqdo-cli
cargo test --workspace
marqdo init demo
cd demo
marqdo check index.mq.md
marqdo inspect index.mq.md --ast
marqdo inspect index.mq.md --signature
marqdo run index.mq.md
marqdo call index.mq.md main
marqdo lock --locked
marqdo sync --locked
```

以上为**最终期望的 CLI 契约**，不代表现有可执行文件已经接受这些参数。实际 `cargo package` 名称可按 workspace 确定，改变命令需先更新本规范与 gold tests。

---

## 13. 分阶段实施路线：每个阶段必须有可检验的完成定义

### P0：形式规范与解析金样（优先，不能省略）

- 固定 Markdown 解析器、扩展配置及版本；枚举节点级映射、惰性区域、表达式载荷抽取算法。
- 写 `spec/markdown-profile.md`, `spec/mapping.md`, `spec/expression.ebnf`, `spec/evaluation.md`。
- 先写 ≥100 个 AST 金样和负例：`Strong`/`Emph`/行内 `Code` 载荷等价源码、标题模式声明与继承、嵌套列表/引用容器（应为零指令或合法载荷）、控制块跨标题边界、Markdown 中乘号/下划线/反引号、无效载荷必须硬错误。
- 交付最小 `inspect --ast`，能输出节点、span 和解析来源。
- **验收**：AST 结构语义无歧义；任何执行位错误不静默退化为文本；渲染前后 Markdown 正常。

### P1：函数、表达式、控制流与通用计算

- 标题/函数索引、签名声明、自由变量推断、局部作用域、执行/返回/调用/分支/循环；结构化错误、可取消 tree-walk。
- 抽象数值语义、纯函数测试、递归与 while，给出图灵完备构造证明草案。
- **验收**：阶乘、欧几里得、Fibonacci、状态机、复杂嵌套程序运行；无需 Python。文档叙述改写不改变执行 IR。

### P2：模块、引用、CLI 规范体验

- YAML 导入、ModuleId/UnitId、静态引用图、跨文档调用、入口与 CLI `run/call/check/inspect/fmt`。
- **验收**：多模块同名函数无冲突、循环导入诊断、失效链接定位、标题改名诊断、接口变动诊断；`fmt` 语义保持。

### P3：项目环境与包管理

- `marqdo.yaml`, `marqdo.lock`、path/Git 固定来源、增量缓存、环境同步、`add/sync/tree`。
- **验收**：离线锁定恢复、跨项目版本隔离、锁文件不静默升级、依赖校验和错误可复现。

### P4：安全核心与隔离执行

- 所有 Host API 的能力检查已在接口层准备；实现 `pure/project/strict` 的边界，worker、资源预算、平台能力检测与受限执行。
- **验收**：未授权 FS/Net/Process 被拒绝；外部进程、worker 超时退出；Strict 无法提供隔离则报错。此阶段前只能在明确受信任环境开发 Foreign/插件，不宣传未实现的沙箱保障。

### P5：Math 与 Foreign

- 内置 Formula 类型、有限符号引擎、Code 值；外联一次性运行/JSON 协议；可选 Python/Node 内核、Python 环境管理。
- **验收**：同一 `.mq.md` 中公式、程序叙述、控制流与外联 Python 可组合执行；代码/公式不调用就不运行；数值误差和不支持公式有明确诊断。

### P6：ABI 插件与第三方开发体验

- C ABI v1、Rust SDK、插件清单、独立 worker 传输、包安装与加载权限、SDK 文档。
- **验收**：一个 C 插件、一个 Rust 插件独立于 CLI 编译；隔离执行和 ABI 版本不匹配测试通过；函数从 `.mq.md` 明确调用、权限无扩大。

### P7：性能优化、稳定发布、后续能力

- 评估缓存与字节码，基准对比 tree-walk；安全审计、Windows/macOS/Linux CI、供应链检查、签名包发布。知识图、形式证明、Notebook View、Agent 属于独立 RFC，按已有能力成熟度分批加入。
- **验收**：可重复构建、固定 ABI/语义版本、完整文档和示例；不靠“示例能跑”冒充语言已完成。

**依赖注意**：安全的 API 与权限模型必须在 P1 先设计，P4 只是强隔离实现与验收；插件 ABI 规范应在 P1/P2 制定，P6 才全面实现开发生态。不能在早期使用不受控的第三方插件完成“临时功能”。

---

## 14. 测试体系：把审美诉求变成可检验的工程约束

### 14.1 必备测试

| 类别 | 目标 | 核心样例 |
|---|---|---|
| AST 金样 | Markdown AST 不被私有解析取代 | 等价粗斜体、混合段落、嵌套列表、引用合并、`@exec`/`@knowledge` 前缀 |
| 语义正例 | 正确程序可执行 | 参数/`params`、递归、`if`/`else`/`while`/`end`、`return` 值 |
| 语义反例 | 不进行启发式兜底 | 非法载荷、悬空 `else`、缺失 `end`、非布尔条件、`params` 位置错误 |
| 不变性 | 叙述修改不影响程序 | 只改 Text/非执行资源，IR 与输出保持 |
| AST 等价性 | 等价 Markdown 结构语义相同 | `**a**`/`*a*`/`` `a` `` 载荷等价、列表/引用容器替换不改变指令 |
| 参数数据流 | 推断正确和诊断准确 | 自增输入、分支部分赋值、同名全局、错别字 |
| 引用/导入 | 可寻址、可复现 | 重名、断链、循环导入、模块版本冲突 |
| 数学 | 求值子集准确、超界报错 | 数值精度、导数、非法 LaTeX、不可解方程 |
| Foreign | 外部执行隔离 | JSON 非法、超时、stdout 污染、状态桥 |
| 插件 ABI | 跨编译器、跨进程安全 | C/Rust 示例，ABI mismatch，worker crash |
| 包管理 | 可重复和离线 | 锁文件、哈希变更、传递依赖冲突 |
| 安全 | 权限无意外扩张 | 路径穿越、符号链接、网络、进程、秘密泄漏 |
| 性能 | 解析/编译/运行基线 | 文档增长、递归、跨模块调用、插件 IPC |

### 14.2 不变性判定（核心研究价值）

定义 `ExecutableProjection(MarkdownAST)` 为经过规范映射、表达式解析和引用解析后的执行 IR。对于所有不触及执行结构和被执行资源/引用的纯叙述修改 `T`，应满足：

`ExecutableProjection(parse(T(D))) == ExecutableProjection(parse(D))`

**前提不能省略**：如果修改的叙述恰好改变 Markdown 块结构（例如产生新标题、列表、强调、链接或引用）、更改标题身份，或改变被明确读取的文本/Code/Math 资源，那么这不是纯叙述变换，不能要求 IR 不变。该测试规则必须用 AST 差分而不是简单的“编辑的是中文句子”判断。

### 14.3 可读性验收

至少用三种领域程序检查源码和普通 Markdown 渲染：算法（递归/复杂控制流）、科研（公式/外联/验证）、真实工具工作流（跨模块 I/O/错误）。评估：是否需要专用渲染器、执行标记密度、自然叙述连贯性、嵌套列表可读性、与等价 Python 的理解与修改负担。不能仅用阶乘展示成功。

### 14.4 质量门槛

- `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test --workspace`。
- 安全关键 FFI 使用 sanitizers/fuzz、操作系统级隔离集成测试；被跳过的跨平台测试必须报告。
- 改语法先更新 spec/ADR，再更新解析/IR，最后更新 gold tests/示例；禁止修改测试以掩盖既有行为退化。
- 在首个稳定版本之前，将语义变更显式记录在 changelog；稳定后必须遵守 SemVer 与语法版本协商策略。

---

## 15. 应参考的项目、只借鉴什么、绝不能照搬什么

| 项目/规范 | 借鉴点 | 不应照搬 |
|---|---|---|
| **CommonMark**（`spec.commonmark.org`） | 标准块/行内解析、容器与嵌套优先级、等价语法 | 不要求普通 Markdown 的所有自由写法都可运行 |
| **GFM**（`github.github.com/gfm/`） | 表格、任务列表及 GitHub 文档兼容 | 不把任意数据表自动识别为程序指令 |
| **Comrak**（`comrak.ee`） | Rust AST、源码位置、数学/GFM 扩展、渲染 | 不依赖私有渲染格式决定语义 |
| **Rust/Cargo** | 模块/错误设计、清单/锁文件、可重复构建、工作区测试 | 不照抄 Rust 表面语言或 Cargo 文件格式为唯一方案 |
| **Python/uv** | 项目环境管理、依赖安装、锁定同步的用户体验 | 不把 Marqdo 翻译成 Python，不自研 PyPI |
| **Wasmtime/WIT/Component Model** | 宿主/扩展接口、隔离、资源限制、跨语言组件契约 | 不把 Wasm 视为所有 native/Python 进程的安全保障 |
| **C ABI + Rust SDK** | 稳定二进制布局、跨编译器加载、类型桥与插件样例 | 不暴露 Rust 不稳定 ABI，不让 native 插件默认获得宿主全部权限 |
| **abi_stable（Rust）** | Rust 动态库 ABI 安全类型与加载检查的经验 | 不强制所有第三方插件用 Rust，基础 ABI 保持 C 兼容 |
| **Jupyter** | 文档/计算并行、交互内核、输出与执行日志 | 不让 Marqdo 程序退化成 notebook cell 顺序和隐藏状态 |
| **SymPy/成熟数学内核** | 符号算法、测试与数值限制的参考 | 不宣称任意 LaTeX 可解释为可证命题 |
| **现有 cflmy/marqdo** | Math、Foreign、ABI、调用绑定、测试素材 | 不自动继承旧语法和插件 ABI 的所有历史行为 |

### 官方参考链接（开发者开发前请核对版本）

- CommonMark: <https://spec.commonmark.org/0.31.2/>；GFM: <https://github.github.io/gfm/>。
- Comrak: <https://comrak.ee/>；源码：<https://github.com/kivikakk/comrak>。
- Cargo 依赖解析：<https://doc.rust-lang.org/cargo/reference/resolver.html>；Cargo Book：<https://doc.rust-lang.org/cargo/>。
- uv：<https://docs.astral.sh/uv/>。
- Wasmtime component API: <https://docs.wasmtime.dev/api/wasmtime/component/index.html>；Component Model/WIT: <https://component-model.bytecodealliance.org/>。
- Rust `abi_stable`：<https://docs.rs/abi_stable/>。
- 旧代码参考：<https://github.com/cflmy/marqdo/tree/main/src/formula>；<https://github.com/cflmy/marqdo/tree/main/src/foreign>；<https://github.com/cflmy/marqdo/blob/main/include/marqdo_abi.h>；<https://github.com/cflmy/marqdo/blob/main/doc/design/module-namespace.md>；<https://github.com/cflmy/marqdo/blob/main/doc/design/ambient-parameters-evaluation.md>。

---

## 16. 贡献者指南与 ADR 工作方式

### 16.1 新开发者如何开始

1. 先读 `README.md`, `AGENTS.md`, 本文件第 0/1/11/13/14 节，再读自己负责模块对应的 `spec/` 形式规范。
2. 先新增一个能复现需求的 `.mq.md` gold 或 negative test；明确其 Markdown AST 的预期结构与执行 IR，**再**修改实现。
3. 改动必须保持**标准 Markdown 解析器唯一**、**执行角色由 AST 决定**、**Rust 自主运行**、**所有副作用经 Host**、**第三方插件经 ABI/能力检查**。
4. 提交前跑相关 `cargo fmt`, `clippy`, `test`，更新 CLI 文档、用户示例及错误诊断，说明不同平台的测试覆盖。
5. 任何改变语言可观察语义、ABI 结构、权限默认值、锁文件格式或依赖求解规则的 PR 必须先有 ADR，注明迁移风险和向后兼容策略。

### 16.2 ADR 模板

```
# ADR-NNN: Title
Status: Proposed | Accepted | Rejected | Superseded
Date:
Context: what problem, with failing example?
Decision: exact semantic/ABI/security contract
Alternatives: at least 2, with trade-offs
Markdown AST shape: what nodes are produced?
Compatibility: source, semantic, ABI, data/lockfile impact
Security: new capabilities/attack surface?
Testing: positive, negative, property, fuzz
Migration & rollout:
```

### 16.3 不允许的实现捷径

- 解析失败便把 Exec 载荷或容器当成普通 Markdown 并静默跳过；
- 用自然语言词（“若/执行/返回/对”等）决定是否启动控制节点；
- 为解决语法问题写大量文本级 regex 来覆盖 Markdown AST；
- 把 Marqdo 编译为 Python 作为唯一官方运行语义；
- 执行/安装插件时默认授予外网、任意进程或全部磁盘权限；
- 在 `check/inspect` 模式执行用户 Python 或第三方 `.so`；
- 各自实现第二套独立的 Notebook/CLI 解释器；
- 在缺少显式 `import` 时静默搜索依赖并改变解析结果；
- 未经版本协商便把旧 ABI v2 插件当新 ABI v1 加载；
- 用“有缓存/虚拟环境”作为安全隔离的证据；
- 把 `Formula` 中的数学陈述自动标记为已证明。

---

## 17. 首批仍需通过 ADR 冻结的设计问题（不是阻止开发的借口）

> **状态更新（2026-11-21）**：本节 S-01/S-02/S-03/S-06 描述的旧映射冲突已由 [RFC-LANG-0002](architecture/RFC-LANG-0002-unified-semantics.md) 与 [ADR-0052](adr/0052-unified-markdown-semantics.md) 解决并废止；下表保留原始表述作为历史记录，其中“本文默认立场”不再是现行设计。仍未冻结的语义问题（参数推断策略、未来 `elif`/`for`/`break`、知识锚点等）需要单独的 Accepted ADR。

下列问题涉及语义精确性。团队可以按本文暂定规则编写参考实现和测试，但**不得在对外声称语言已稳定时留下空白**。

| ID | 问题 | 本文默认立场（历史） | 冻结前需要的证据 / 现行状态 |
|---|---|---|---|
| S-01 | 直接运行 `Strong` 中的数学 `*` 与 CommonMark 强调冲突 | 强调强制执行，允许行内代码无损载荷 | 已由 RFC-LANG-0002 解决：载荷统一、建议用行内 CodeSpan；需 AST 金样 |
| S-02 | 普通斜体与返回冲突 | 程序体中的 Emph 永远是返回，文档约束使用者 | 已由 RFC-LANG-0002 解决：`return` 是关键字，斜体只是等价载体 |
| S-03 | `OrderedList`/`BulletList` 在无前缀 `#` 的顶层语义 | 只在函数体内执行；其他区域按文档处理 | 已由 RFC-LANG-0002 解决：列表只是容器，无隐含指令 |
| S-04 | 参数推断在部分赋值路径中的歧义 | 静态数据流；不确定时报错 | 仍需 ADR；CFG 案例与 ABI 变更测试 |
| S-05 | 中文函数名与英文点号/括号表达式 | Unicode 标识符、固定优先级；含空格走链接引用 | 仍需词法与调用金样 |
| S-06 | 用 Markdown Link + 参数列表作为调用 | BlockQuote 内接受 `Link + suffix`，固定 AST 形式 | 已废止：BlockQuote 不再调用；跨文件调用须显式导入/表达式调用，另有 ADR |
| S-07 | 多公式/代码围栏在同一章节下如何寻址 | 不自动选第一条；显式 selector/API | 引用稳定性与用户体验测试 |
| S-08 | 任意精度数值与跨语言/插件 JSON 边界 | BigInt 为精确整数、wire 有类型标签 | 数值和跨 ABI 测试 |
| S-09 | strict sandbox 不同平台的保障 | 不能落实就拒绝 strict | Linux/Win/macOS 安全功能矩阵 |
| S-10 | ABI 完整内存所有权与版本协商 | 独立 C ABI v1，host-owned buffers | C/Rust SDK、fuzz、兼容性测试 |
| S-11 | 项目内容和导入函数命名空间重叠 | 显式别名且静态报错 | 多模块复杂命名测试 |
| S-12 | 平台 Markdown 渲染锚点与内部 UnitId | 分离两种身份，维护检查映射 | Unicode/重复标题/改名回归 |

**规则**：ADR 一旦 Accepted，应同步修改 `spec/` 对应文件并加入机器可运行的金样；本蓝图中的候选细节与 Accepted ADR 冲突时，以新的 Accepted ADR 和更新后的 `spec/` 为准。所有版本均可追溯，避免多轮讨论重新引入已经否决的推断行为。

---

## 18. 首个发布版本的 Definition of Done

只有满足以下全部条件，才能对外声称 **Marqdo CLI Alpha** 完成：

- 无需 Python，可通过 `run/call` 执行非平凡递归、分支、循环、函数调用与参数推断程序。
- `.mq.md` 在指定 CommonMark/GFM/Math 渲染配置下可读且标准 AST 行为由金样覆盖。
- 执行语义无启发式降级；所有非法执行节点给出准确源码位置。
- `check/inspect` 无副作用；导入、调用和静态引用在多文件中可验证。
- 项目清单、锁文件、path/Git 依赖和环境同步具有最小可用实现。
- 所有宿主副作用受能力授权；不可信外部代码与 native 插件不能靠不受限的 in-process 模式冒充 strict sandbox。
- Formula 的可计算子集明确且可测试；Foreign 的输入/输出、超时与权限行为明确。
- 插件 C ABI v1、Rust SDK、C 示例、隔离 worker 基本通路及 ABI 兼容错误测试通过。
- CI 具有至少一个全流程科研实例：Markdown 函数 → Math → Foreign → 插件 → 受控输出。
- 示例、README、AGENTS 和贡献者说明能让新开发者在不依赖旧仓库历史语境的情况下完成首次功能修改。

这份蓝图的目标不是先把所有功能做大，而是让后来加入的开发者明确**哪些是语言的不可动摇原则、哪些是已选的接口、哪些是要用测试裁决的候选、哪里应该扩展、哪里绝对不能新增隐式行为**。

> **最后的设计箴言**：让 Markdown 承担结构，让少量严格表达式承担计算，让 Rust 自主承担执行，让依赖与引用显式可审查，让插件以稳定 ABI 扩展能力，让所有副作用接受 Host 与沙箱约束。**程序是一份文档，但不因此变成一段需要机器猜测的自然语言。**
