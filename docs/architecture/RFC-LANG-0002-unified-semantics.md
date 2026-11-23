# Marqdo CLI 下一代统一语义：破坏性重构、完整语法规范与全仓库迁移实施手册

> **仓库采纳记录（2026-11-21）**：本文件是当前**唯一**的语言语义权威规范，由
> [ADR-0052](../adr/0052-unified-markdown-semantics.md) 采纳并取代此前所有
> Strong/Emph/List/Quote 映射描述。`spec/mapping.md` 与 `spec/expression.ebnf`
> 已原位改写为本规范的可执行摘要。本规范已由前端、`marqdo-expression` 语句
> 解析器、IR 降级、运行时与同仓库标准库**完整落地**；`ffd1cc9` 上 workspace
> 测试全绿（详见 [`../STATUS.md`](../STATUS.md)、
> [`../evidence/2026-11-21-semantics-v2-green.md`](../evidence/2026-11-21-semantics-v2-green.md)
> 与 [`../migration/semantic-rewrite-inventory.tsv`](../migration/semantic-rewrite-inventory.tsv)）。
> 下文保留原始 RFC 的状态行作为设计历史，不改变其技术内容。

> **文档定位**：本文件是**无需任何聊天背景即可独立实施的、单一现行语法开发规范**：先讲完整语言使用方法，再说明相较旧语法的破坏性变化，然后规定实现架构、全仓库迁移和阻断性验收。不能只阅读 RFC 标题就开始编码。  
> **状态**：已采纳并实现（2026-11-21，见顶部采纳记录与 ADR-0052）。**本规范一次性替换原有语义**，不提供 V1/V2 双编译、不按文件选择语法、不保留旧 Mapper 和运行时兼容分支。  
> **仓库**：[`cflmy/marqdo-cli`](https://github.com/cflmy/marqdo-cli)，源码审查 SHA `7fbe069196e61d06a00d64fe8d65049c327373eb`。  
> **语法名称**：下文称“新语法”或“新语义”，此名称用于规范和发布说明，**不是可切换的运行模式或 Front matter 选项**。  
> **Markdown 层**：继续使用当前 Comrak/CommonMark/GFM AST；若扩展设置不变，Markdown 解析器 Profile 可保持原身份。变更的是唯一的 Markdown→Program 语义映射。  
> **规范所有权**：实施后 `spec/mapping.md`、`spec/expression.ebnf` 等现行文件**原位替换**为新规则；旧文本仅保存在 Git 历史或带显著废止说明的历史目录，不再作为第二份规范。  
> **唯一合法标题模式**：`@exec` 和 `@knowledge`。`@knowledg`、`@kongwledge`、`@Exec` 均非合法别名；在模式声明位置出现时应诊断。  
> **最终条件**：整个仓库（含 `stdlib/`、`examples/`、README、spec、测试数据、CLI 帮助文本）迁移完成，**仓库内不存在任何活跃的 V1 语义入口**后才允许合并为可发布分支。

---

# 第 0 章：这门语言是什么？先把语法讲明白

**Marqdo 是一门以 Markdown 为源文件的、具有独立表达式语法和运行时的编程语言。** 源文件仍是 `.mq.md`。Markdown 负责标题、段落、列表、引用、表格、公式和普通说明的文档组织；Marqdo 负责在**明确声明为执行的标题区域中**解释程序载荷。用户不需要为算法调用 Python 或 Rust，已有内存、值、函数、I/O 资源等能力仍由现有 CLI Runtime 支撑；本 RFC 只改变**怎样书写和确定性地识别程序**。

**旧语法与新语法的一句话对比**：

- **旧语法**：`**...**` 是执行、`*...*` 是返回、`1.` 是条件分支、`-` 是循环、`>` 是调用；Markdown 的排版类型直接决定指令。
- **唯一新语法**：标题中写 `@exec` 或 `@knowledge` 明确执行/知识模式；`**...**`、`*...*`、`` `...` `` 都只是**等价的程序语句载体**；**载荷里面**的 `if`、`else`、`while`、`end`、`return` 决定控制流；`1.`、`-`、`>` 仅组织显示，不再自动产生程序指令。

## 0.1 30 秒掌握的第一份新语法程序

以下是 **唯一目标语法**，在重构完成之前不能用当前 CLI 原样运行；重构完成后它就是唯一合法语法。

````markdown
# @knowledge 数学演示

这是普通文档。这一行**加粗**只会显示为加粗，不执行。

## @exec main

**x = 5**
`y = x * x`
**return y**
````

**执行结果：** `marqdo run index.mq.md` 返回整数 `25`（按现有 CLI 的返回值展示方式输出）。

**你必须看懂的六件事：**

1. **没有语义版本开关**：所有 `.mq.md` 均按同一套新规则编译，文件无须 `marqdo_semantics` 字段。旧程序必须改写，不允许默认回退旧语法。
2. `# @knowledge 数学演示`：H1 命名空间 / 文档单元，模式 Knowledge；普通 Markdown 完全合法。
3. `## @exec main`：H2 函数 `main`，覆盖父级模式；`@exec` 不属于可调用名称。
4. `**x = 5**`：闭合载荷，内部是赋值语句；外部加粗只影响 Markdown 显示。
5. `` `y = x * x` ``：也是相同的执行载荷；独立 CodeSpan 可避免 `*` 运算被 Markdown 识别为强调。
6. `**return y**`：函数退出来自显式关键字 `return`，不再由斜体自动触发。

## 0.2 函数、说明、条件分支在一份文件里

````markdown
# @exec 计算

## main

`x = 10`

1. **if x == 10**

   > `print("正确")`

2. *else*

   > **print("错误")**

**end**
`return x`

### @knowledge 关于判断条件

这部分可以正常写**加粗**、*斜体*、`if` 示例、列表与 GFM 表格。

| 条件 | 说明 |
| --- | --- |
| x == 10 | 正确 |
| 其他 | 错误 |
````

**执行结果：** 在允许控制台输出的环境中输出 `正确` 并返回整数 `10`（`print` 的具体换行等仍遵守现有标准库）。

注意：`# @exec 计算` 将执行模式传给未标记的 `## main`；`### @knowledge` 则只在自己的小节内覆盖为知识模式。**列表序号 `1.`/`2.` 和引用符 `>` 都没有指令含义**。如果把列表全部改成 `-`，或者把 `print` 移出 `>`，控制语义仍应相同（仅来源 Span 和 Markdown 结构不同）。

## 0.3 一个普通文档，完全无需执行

````markdown
# @knowledge 我的笔记

## 本章目标

1. 阅读文献
2. 设计实验
3. 撰写论文

这是**重点**；这是*提醒*；这是`x = 10`的代码例子。

> 这只是引用，不是函数调用。
````

**结果：** 编译时保留这些 Markdown 结构，不生成可执行语句，也不会报“列表缺条件”。运行 `main` 会报告没有入口函数，因为这里并未定义函数。这不是 `[]` 空列表值，也不是一次返回 `Unit` 的函数执行，而是**没有可执行入口**。

## 0.4 最小语法速查表（初学者和审查者先看这里）

| 想实现的意图 | 新写法（例） | 确定性解释 |
|---|---|---|
| 语法选择 | **没有语法选择开关** | 全部 `.mq.md` 仅按当前新语法编译 |
| 声明知识区域 | `# @knowledge 说明` | 该标题的本节及继承子节只产生文档节点 |
| 声明执行区域 | `## @exec main` | H2 定义函数，允许其中的载荷执行 |
| 继承模式 | `### 子节` | 使用最近祖先标题的模式 |
| 局部切换模式 | `### @knowledge 原理` / `### @exec 计算` | 在该标题范围覆盖祖先模式 |
| 显式参数 | `` `params(n: Int)` `` | H2 Exec 函数第一个直接载荷，只作为编译期声明 |
| 赋值 | `**x = 1**` / `` `x = 1` `` | 两者完全相同 |
| 一般计算 | `*print(x)*` / `` `print(x)` `` | 调用/求值语句；斜体不再是返回 |
| 返回 | `**return x**` / `` `return x` `` | 返回整个当前函数 |
| 条件开始 | `` `if x > 0` `` | 进入 if then 分支 |
| 否则 | `*else*` | 分割最近一个尚未结束的 if |
| 分支结束 | `**end**` | 关闭最近未结束的控制结构 |
| 循环开始 | `` `while i < n` `` | 循环条件，每轮重新求值 |
| 普通列表 | `1. 说明` / `- 说明` | **没有**自动条件/循环语义 |
| 普通引用 | `> 说明` | **没有**自动调用语义 |
| 函数调用 | `> **print(x)**` 或 `**print(x)**` | 调用来源于载荷表达式，与引用符无关 |
| 惰性代码示例 | 围栏 ` ```python ... ``` ` | 不执行；不同于行内 CodeSpan |
| 章节内强调但不执行 | 在 `@knowledge` 子节使用 `**重点**` | 正常 Markdown 强调 |

**最危险的常见错误**：在 `@exec` 段落中写 `**重要**`，如果被提取为独立闭合载荷，解释器会把 `重要` 当成一个程序表达式。V2 **不**通过自然语言判断它是不是想强调。需要普通强调时，放在 `@knowledge` 章节或规范定义的惰性数据区域中。这里有意保持“已进入执行模式就严格解析”的确定性，而非猜测。

---

# 第 1 章：V1 实际语法、为什么要改、V2 改了什么

## 1.1 重构前真实规则（审查基线已实现，重构后废止）

审查基线上的旧仓库 `spec/mapping.md` 和 `crates/marqdo-frontend/src/map.rs` 采用下面的映射（重构完成后这些规则必须废止）：

| 旧结构 | 旧含义 | 典型例子 |
|---|---|---|
| H1 `# 标题` | 命名空间 | `# 数学` |
| H2 `## 标题` | 函数 | `## 平方` |
| H3–H6 | 函数内章节 | `### 算法步骤` |
| Strong `**...**` | 语句（赋值/表达式） | `**x = 1**` |
| Emphasis `*...*` | 返回语句 | `*x*` |
| OrderedList `1.` | 互斥条件分支 | 第一段必须以 CodeSpan 条件开头 |
| BulletList `-` | 独立 while 循环 | 第一段必须以 CodeSpan 条件开头 |
| BlockQuote `>` | 函数调用 | 整个引用块必须满足调用格式 |
| 普通段落 | 普通文字；但其中 Strong/Emph 会按上述规则执行 | `这是说明。**x = 1**` |
| GFM 表格 / 数学 / 围栏代码 | 通常为惰性文档资源 | `|a|b|`、`$x$`、代码围栏 |

**旧语法例子（当前可编译的语法形式）**：

````markdown
## 分类

1. `n < 0`

   *-1*

2. `n == 0`

   *0*

3. `n > 0`

   *1*
````

这个程序靠列表决定“分支”，靠斜体决定“返回”；普通列表无法在函数内自由出现。虽然是严格确定性规则，却将普通 Markdown 的排版类型与程序指令过度绑定。

## 1.2 新语法的四项正式设计原则

**原则 A：所有级别的标题可以声明语义模式。** 从 `#` 到 `######` 都能精确使用 `@exec` 或 `@knowledge`，默认从结构祖先继承，不读取正文猜测。

**原则 B：闭合标记的计算意义统一。** `Strong`、`Emph`、行内 `Code` 在同一 Exec 范围内，只是同一种“单语句载荷”的不同 Markdown 显示形式，不再对应固定执行/返回角色。原始 AST 类型仍保留用于渲染和 source span。

**原则 C：载荷文本由明确的程序关键字决定控制操作。** `if`、`else`、`while`、`end`、`return`，以及 `params` 声明、赋值、调用与已有表达式语法；列表和引用块仅作容器。程序错误必须报错，不能回退为文档。

**原则 D：知识编译不在本轮开发。** 本轮保留结构、身份、跨度、文档资源，为将来可寻址知识、证明和依赖分析留接口；不开发全节点持久锚点、图谱和证明系统。

## 1.3 旧语法 → 新语法逐项迁移对照（开发者必须能据此改代码）

| 语义需求 | V1 源码 | V2 源码 | 新语法实际变化 |
|---|---|---|---|
| 函数 | `## main` | `## @exec main`（或继承 H1 Exec） | 默认知识模式；Exec H2 才可调用 |
| 命名空间 | `# 算法` | `# @knowledge 算法` / `# @exec 算法` | 允许设置后代默认模式 |
| 普通加粗 | 函数体内经常触发执行 | 在 Knowledge 内正常显示 | 视觉排版不再全局等于执行 |
| 赋值 | `**x = 1**` | `**x = 1**` 或 `*x = 1*` 或 `` `x = 1` `` | 闭合载荷统一 |
| 返回 | `*x*` | `*return x*` 或 `**return x**` | 返回由关键字表意 |
| 条件分支 | `1. \`cond\`` + 子项代码 | `**if cond**` ... `**end**` | 不受列表类型限制 |
| else 分支 | V1 用互斥列表多个条件表达 | `*else*` | 显式 else，不能当作一条独立的 if |
| 循环 | `- \`cond\`` + 子项代码 | `` `while cond` `` ... `` `end` `` | 不受 bullet 绑定 |
| 调用 | `> print(x)` 或 `**print(x)**` | `> **print(x)**` 或 `**print(x)**` | 引用块本身不再执行 |
| 位置写入 | `**m[0] = 1**` | `` `m[0] = 1` `` 或 `**m[0] = 1**` | Memory 单字节写入语义不变 |
| 函数参数 | H2 后首个独立 CodeSpan `(x: Int)` | `**params(x: Int)**` 或 `` `params(x: Int)` `` | 无 CodeSpan 特权 |
| 普通列表 | 在函数内常报条件错误 | 普通知识/说明列表不会自动报错 | 空执行投影，不是 `List([])` |
| 普通引用 | 在函数内常报调用格式错误 | 普通引用不产生 IR | 只有其中合规载荷可执行 |
| Markdown 章节 | 标题不带模式 | 标题可显式声明或继承模式 | 模式不是函数名的一部分 |
| 标准库 / 内存 / 资源 | 原有语义 | 原有语义 | 不因 V2 重构改变 Runtime |

## 1.4 V1 同一程序迁移为 新语法的完整前后示例

**旧语法程序（迁移前，不得用新 CLI 运行）**：

````markdown
## 求和

`(n: Int)`

**i = 0**
**sum = 0**

- `i < n`

  **sum = sum + i**
  **i = i + 1**

*sum*
````

**新语法程序（完全相同的核心算法）**：

````markdown
## @exec 求和

**params(n: Int)**

`i = 0`
`sum = 0`
`while i < n`
`sum = sum + i`
`i = i + 1`
`end`
`return sum`
````

调用 `求和(n=5)` 应当返回 `10`。这体现了重构真正改变的地方：**循环和返回由语句本身描述，Markdown 只决定程序在文档中的承载与排列。**

---
# 第 2 章：新语法标题声明的完整语法

## 2.1 `@exec` / `@knowledge` 在哪里、怎么写

**只有 Markdown 解析器识别出的标题节点**（Heading，级别 H1–H6）才有资格携带模式声明。推荐写成：

````markdown
# @knowledge 一级知识单元
## @exec 函数名
### @knowledge 概念解释
### @exec 后续计算
#### @knowledge 小节资料
##### @exec 更细的计算段
###### @knowledge 注释
````

语法草案（标题自身由 CommonMark 解析；以下是其**首个纯 Text 子节点**的严格前缀语法，而不是第二个 Markdown 正则解析器）：

```ebnf
heading_mode_prefix = "@exec" | "@knowledge" ;
heading_decl        = heading_mode_prefix, whitespace, nonempty_title ;
nonempty_title      = any_nonempty_heading_content_after_mode ;
```

- `@exec` 与 `@knowledge` **大小写敏感、拼写完整、后面必须有空白和非空标题名**。
- 仅当前缀位于**标题开头、直接 Text 子节点**，且作为完整词时识别。不能搜寻标题中任意子串。
- `## @exec main` 合法；`## main @exec` **不会**启用 Exec，标题名就是 `main @exec`。
- `## @exec` 因没有标题名而失败；`## @exec @knowledge main` 因冲突声明而失败。
- `## @knowledg main`、`## @kongwledge main`、`## @Exec main` 按未知声明报错，提供 `@knowledge` / `@exec` 的拼写建议。不能自动恢复。
- `## **@exec** main` 中 `@exec` 不在第一个普通文本节点上：不算模式声明，也不扫描嵌套 Strong 猜意图。
- 原标题用于 Markdown 渲染（用户仍能看到 `@exec`），但**语义名字**必须剥离声明标记：`## @exec main` 的函数名是 `main`。
- 前缀类似 `@other` 的首词在 V2 中为未知模式声明，给出精确诊断；需要字面以 `@` 开头的标题，可选择不把它放在标题的第一个纯 Text 词，或在未来另订转义规范。**不私自引入未经约定的转义语法。**
- 正文中的 `@exec`、`@knowledge` 只是普通文本，不会改变模式。

**为什么需要这些限制？** 因为单元类型是编译期决定执行边界的显式信息。拼写错误若退化成继承模式，可能使应运行的代码静默失效，或使普通知识意外执行。

## 2.2 标题模式的继承、覆盖和结束

Markdown 标题本来就具有由层级决定的范围：H3 隶属于最近的 H2；遇到下一个同级或更高级标题，当前 H3 结束。V2 不改变这一范围。

定义：

```text
Mode(document root) = Knowledge
Mode(heading) = heading.explicit_mode             当含有效声明
Mode(heading) = Mode(nearest_ancestor_heading)     没有声明且存在祖先
Mode(heading) = Knowledge                          没有声明且无祖先
```

**“最近祖先”是标题层级栈计算结果，不是自然语言上下文推理。** 解析器以源文件 Markdown AST 的顶层 Heading 顺序、Heading level 确定这个栈。跳级（H1 直接到 H3）时仍按最近有效祖先继承，不凭“缺少 H2”猜测 H2。

完整演示：

````markdown
# @exec 项目              <!-- H1：Exec ；下面 H2 默认 Exec -->

## main                  <!-- H2：Exec 函数 main -->

`a = 1`                 <!-- 执行 -->

### @knowledge 思路       <!-- H3：Knowledge -->

**这一行只会被当作文档。**

#### 实验说明              <!-- H4：继承 Knowledge -->

*不返回*

### @exec 继续            <!-- H3：Exec，仍在 main 内 -->

`a = a + 1`
`return a`

## @knowledge 参考资料     <!-- H2：Knowledge，不是函数 -->

1. 正常列表
2. 正常说明

## 第二个知识章节          <!-- 继承 H1 Exec：是一个 Exec H2 函数 -->

`return 0`
````

**说明**：行内 HTML 注释只是本规范示意，为避免不同解析器对注释/标题的 AST 差异，在正式 golden fixture 中应删除这些注释。示例中最后一个 `## 第二个知识章节` **仍然继承 H1 的 Exec**，即使紧邻其前的 H2 是 Knowledge；兄弟节点不能互相传染模式。这是必须测试的边界。

## 2.3 标题级别与函数边界是两件不同的事

本 RFC **保留已有函数声明的层级约定**，不允许新标记把每个标题都变成函数：

| 标题 | 声明/继承模式 | 语言对象 | 能否被 `marqdo call` 直接调用 | 对执行的作用 |
|---|---|---|---|---|
| H1 | Knowledge 或 Exec | Namespace/顶层单元 | 否 | 设定后代默认模式 |
| H2 | Exec | Function + Unit | **能** | 开启函数主体 |
| H2 | Knowledge | Knowledge Unit | 否 | 普通知识章节，不创建函数 |
| H3–H6 | Exec | Function 中的 Section | 否 | 若归属于 H2 Exec，可追加语句片段 |
| H3–H6 | Knowledge | Function 或知识章节下的 Section | 否 | 保留文档，不追加语句 |

**为什么 H3 不能自动变成函数？** 目前 CLI 的 H2 绑定、模块导入、名字查找、参数推断、`run main` 约定均基于 H2。模式允许各级标题切换执行权限，并不等于打破函数层级。将两者分离有利于渐进改造。

**非法情况：没有 H2 Exec 函数却试图执行代码。**

````markdown
# @knowledge 论文
## @knowledge 第一节
### @exec 试算
`x = 10`
````

此例不能把 `x = 10` 当全局初始化，也不能偷偷创建 `试算()`。检查时报 `sem.exec_without_function`（含所属 H3 和最近 H2 的位置）。如要创建可调用的试算函数，应将父 H2 声明成 `@exec` 或新建一个 H2 Exec 函数。

同理：在 H1 Exec 的直接正文出现闭合载荷但没有 H2 函数时，属于**无所属函数的可执行载荷**，报错，而不是当作模块级表达式执行。

## 2.4 唯一语法与 Front matter 规则

**重构完成后不需要、也不允许按文件选择语法版本**。每一个 `.mq.md`、依赖模块、测试 fixture、官方标准库文件都使用同一套新语法。以下文件合法：

````markdown
---
import:
  io: "std:io"
---

# @knowledge 项目

## @exec main

`return 1`
````

- `import` 等既有 Front matter 项仍按原用途处理；**不得引入** `marqdo_semantics: 2`、`syntax: v1`、`--syntax v1`、`--legacy` 等入口。
- 对旧草案中出现的 `marqdo_semantics` 字段，目标编译器应报 `sem.unsupported_version_directive`，提示移除；不能把它当语义分支、也不能忽略未知键后造成错误自信。该字段不能用于“看似成功”的兼容运行。
- 目标语言的默认根模式是 **Knowledge**；H1–H6 的 `@exec`、`@knowledge` 采用严格 AST 前缀声明与祖先继承，**不使用正文内容做判别**。
- 无声明的旧式 `## main` 在默认 Knowledge 下不是函数；这类项目应在重构阶段批量改写为 `## @exec main` 或通过 `# @exec` 继承。
- `marqdo.md.v1` 代表当前固定的底层 Markdown 解析 Profile 时，可原样保留解析器名称；**不能继续把旧 `spec/mapping.md` 当作活跃语义规范**。新的语义不以运行时可切换的版本号呈现。
- 已安装第三方依赖如果只提供旧语法，应在加载/检查阶段明确失败，要求升级包；不能悄悄编译成旧指令。锁文件和 digest 仍应可重复、可审计。

**强制要求**：最终代码中不得存在 `map_v1`、按语义版本分派的 `match`、专用 V1 语法迁就逻辑或混合版本模块测试。历史行为仅可保留为“不再合法”的负面测试与 Git 历史。

---

# 第 3 章：统一闭合载荷——什么会执行、怎样写、怎样报错

## 3.1 “闭合标记统一”在 新语法的精确定义

以下在 **相同函数、相同 Exec 模式、相同 payload 文本**条件下编译为相同 IR：

````markdown
**x = 10**
*x = 10*
`x = 10`
````

以下亦等价：

````markdown
**return x**
*return x*
`return x`
````

三行是**三种互相替换的写法**，不是要求同一函数连续执行三次！

统一的是**计算语义**，不是 Markdown 的渲染方式：加粗仍显示为加粗，斜体仍显示为斜体，行内代码仍显示为行内代码。源码、source span、AST kind、阅读风格都被保存。

| Comrak AST kind | 是否为 V2 MVP 载荷 | 提取规则 | 附注 |
|---|---|---|---|
| `Strong` | 是 | 合并其**直接** Text/Code 载荷子节点 | 不保留“专用执行”角色 |
| `Emph` | 是 | 同 Strong | 不保留“专用返回”角色 |
| `Code`（**行内**） | 是 | 直接使用其 literal | 必须在合法提取上下文 |
| `CodeBlock`（围栏或缩进） | 否 | 作为惰性 Code 资源 | **不要**因此把所有代码围栏直接执行 |
| `Strikethrough` | 否（本期） | 惰性文档 | 将来扩展需独立 ADR |
| `Link/Image` | 否 | 链接/图片值 | 链接关系、`[]()` 保持原语义 |
| `Table/Math/HTML` | 否 | 惰性文档数据 | GFM 表格单元格不直接执行 |

**关于 `****...****` 等标记**：不能直接数 Markdown 字符决定语义。CommonMark 可能把多个星号解析为嵌套 Strong/Emph，甚至不存在一个完整的闭合 AST 节点。V2 只承认**真实生成的合法载荷 AST 节点**；如果存在受许可的嵌套载体，可按下述单次提取处理；无法满足提取规则的形态报错。严禁用正则重新扫描原始星号串并把它当新语法。

## 3.2 精确的载荷提取规则

在 `@exec` 中，Frontend 使用以下规则；不应由 Runtime 再判断：

1. 只从可执行范围下、允许遍历的 Markdown 容器提取**外层**合法载荷节点。
2. 对独立行内 CodeSpan：取 `literal`，产生**一条**程序语句的候选载荷。
3. 对 Strong/Emph：仅允许直接 Text 与 Code 子节点，按原始顺序连接为一个字符串；其内部 Code **被外层消费，不得二次执行**。
4. 如果 Strong/Emph 内包含另一个 Strong/Emph、Link、Image、Math 或其它未获许可的复杂嵌套：在 Exec 位置硬错误 `sem.payload_nested`；不能跳过错误节点，也不能递归到子节点执行半条语句。
5. 同一个载荷只允许**一条语句**。例如 `` `x = 1; return x` `` 报错误；正确做法是拆成两个独立闭合节点。
6. 载荷可以包含空格与合法表达式文本，仍必须被完整消耗至 EOF。不能忽略尾部错误字符。
7. 每个载荷必须拥有 source span；嵌套 Strong(Code) 时记录源文本片段到 payload 的片段映射，以支持精确错误定位。
8. 单独出现的 CodeSpan 与被 Strong/Emph 包裹的 CodeSpan，在规范上的计算效果一致；嵌套情况只执行一次。

**推荐写法（避免 `*` 被 CommonMark 当作强调标记）：**

````markdown
**`y = x * x`**
````

或更短地写：

````markdown
`y = x * x`
````

这两者均产生一条赋值语句。**不要**把复杂的 `**return x * x**` 当作无需 golden 验证的示例：Markdown 强调与乘法符号会相互干扰。

## 3.3 哪些 Markdown 区域绝对不递归执行？

| 区域 | 行为 | 为什么 |
|---|---|---|
| `@knowledge` 下全部正文 | 零执行语句 | 明确的知识/文档模式 |
| 标题文本、标题中的模式声明 | 不执行 | 标题只声明边界、身份与模式 |
| `[]()` 的链接标签和 URL | 不执行 | 避免“知识链接 = 函数调用”混淆 |
| 图片说明 | 不执行 | 文档数据 |
| GFM Table 单元格 | 不执行 | 表格仍是数据 |
| 公式 / MathBlock | 不执行 | 公式是知识资源；显式调用才能求值 |
| HTML | 不执行 | 避免绕过 AST 规则 |
| 围栏代码 / 缩进代码块 | 不执行 | 代码资源仍惰性；保留 Markdown 自然展示 |
| Front matter | 不执行 | 模块配置与元数据 |
| 删除线 / 不允许的扩展 Markdown 节点 | 不执行 | 未定义为执行载荷 |

**这里没有“语法解析成功才执行”一说。** 执行边界由单元模式和语法载荷类型决定；只要在合法执行位置中提取出载荷，就必须完整编译，错误不能回退为普通文字。

## 3.4 普通段落、列表、引用块中的载荷怎么处理？

以下五种形式都仅提取 `print(x)` 作为调用表达式（不改变 `print` 的功能）：

````markdown
## @exec main

**print(1)**

*print(1)*

`print(1)`

> **print(1)**

- `print(1)`
````

这段是**同一函数顺序调用五次**的示例，用于展示不同承载形式可以共存。替换任一承载形式时，该次操作的语义保持一致。`>` 不再“执行调用”，`-` 不再“执行循环”，由括号内的 `print(1)` 形成调用表达式。

**普通引用合法**：

````markdown
## @exec main

> 这是对算法的解释，不是调用。

`return 1`
````

**普通列表合法**：

````markdown
## @exec main

1. 第一步：准备数据。
2. 第二步：校验结果。

`return 1`
````

这些文档节点没有可执行载荷，因此产生零条相应指令，不因为缺少条件而报错。**空的执行投影**不同于创建一个 `[]` 值。

**普通 Markdown 排版的真实限制**：

````markdown
## @exec main

这里的 **关键思想** 是保持不变量。
````

由于 `Strong` 是统一载荷载体，`关键思想` 会被当作程序表达式。若没有该变量，产生明确的符号错误；即使恰有同名变量，也会求值（不得靠“看起来是中文”跳过）。**这是本次确定性设计主动接受的代价，而不是遗漏的优化项。** 要无歧义地表达普通强调，应使用 `@knowledge` 子章节或不会被递归执行的惰性 Markdown 区域。

---

# 第 4 章：新语法程序语句、关键字和完整控制流教程

## 4.1 关键字一览：最少词汇，足够明确的控制意图

| 关键字 | 语法 | 含义 | 是否新增计算原语 |
|---|---|---|---|
| `if` | `if <Bool expression>` | 开始条件分支 | 否，映射已有 `Branch` |
| `else if` | `else if <Bool expression>` | 添加同一分支组的条件臂 | 否，复用 `Branch` |
| `else` | `else` | 切换至当前 if 的否则分支 | 否 |
| `while` | `while <Bool expression>` | 开始循环 | 否，映射已有 `Loop` |
| `break` | `break` | 退出最近一层循环 | 否，结构化控制转移 |
| `continue` | `continue` | 重新检查最近一层循环的条件 | 否，结构化控制转移 |
| `end` | `end` | 结束最近未关闭的 if/while | 否，仅用于编译期配对 |
| `return` | `return <expression>` | 返回当前 H2 函数，终止后续执行 | 否，映射已有 `Return` |
| `params` | `params(name: Type, ...)` | H2 Exec 函数的可选参数声明 | 否，编译期元数据 |

现有 `true`、`false`、`None`、`and`、`or`、`not` 等仍具有原表达式含义。`print`、`input`、`open`、`memory`、`table` **不是**控制关键字，属于已有内置函数或标准库 API；本 RFC 不负责发明新的 I/O 函数。

**语法统一意味着**：关键字必须位于载荷文本中，而不是写在 Markdown 标记外面再让编译器猜测。

- ✅ `` `if x > 0` ``
- ✅ `**if x > 0**`
- ✅ `*return x*`
- ❌ `1. if x > 0`（只是普通文本，不是执行载荷）
- ❌ `> print(x)`（只是普通引用正文，不是调用语句）
- ❌ `if x > 0`（在普通段落中，不在闭合载荷内，只是普通文字）

## 4.2 赋值与表达式：沿用当前语言既有计算语法

````markdown
## @exec main

`a = 10`
**b = 20**
*sum = a + b*
`return sum`
````

返回 `30`。斜体 `*sum = a + b*` **不是返回**，只是第三条赋值操作。

表达式仍保持现有整数、浮点、字符串、布尔、列表、Map 记录、运算、索引与函数调用规则。当前 V1 已有 `m[0] = 1` 的 Memory 索引写入能力，V2 仅允许统一载体承载它，不因此允许对任意 List/Map 成员执行原地修改。`a.field = 1` 等当前不支持的赋值形式仍不合法。

函数调用是表达式，既可以丢弃结果：

````markdown
`print("你好")`
````

也可以接收结果：

````markdown
`x = 求和(n=5)`
````

不需要引用块 `>` 才能调用。

## 4.3 `return`：告别斜体专用返回

````markdown
## @exec double

`params(x: Int)`

**`return x * 2`**
````

- `return` 从**当前 H2 函数**返回，不只是结束某个列表项或局部 H3 章节。
- `return x` 必须带表达式；`return` 空语句在首版报错，不自动代表 `None`/`Unit`。
- 自然执行到函数末尾仍返回现有 `Unit`，与显式 `None` 不同。
- `*x*` 在 V2 Exec 区域代表求值表达式 `x` 并丢弃结果，**不再 return**。
- 若 `` `return x` `` 出现在 Knowledge 模式，保留为文档，不执行也不报变量未知。

## 4.4 `if` / `else` / `end`：可读、互斥、能嵌套

**最简单的分支：**

````markdown
## @exec main

`x = 3`
`if x > 0`
`print("positive")`
`else`
`print("non-positive")`
`end`
`return x`
````

各条件必须返回 Bool，每个条件在轮到该臂时求值一次。执行第一个为真的臂，或在所有条件为假时执行末尾的 `else`。没有匹配臂也没有 `else` 时直接继续。每组需要一个 `end`。

**将它排成有序列表也完全合法**：

````markdown
## @exec main

`x = 3`

1. **if x > 0**

   > `print("positive")`

2. *else*

   - **print("non-positive")**

**end**
`return x`
````

两段程序的控制流应相同；区别仅在文档的排版。`1.`、`2.` 与嵌套 `>`、`-` 不是运行时条件或循环指令。

**嵌套分支：**

````markdown
## @exec main

`x = 12`
`if x > 0`
  `if x > 10`
    `print("large")`
  `else`
    `print("small")`
  `end`
`else`
  `print("non-positive")`
`end`
`return x`
````

这里代码行的前导空格只是为了帮助阅读，**不能保证在 CommonMark 中形成相同的 AST**（连续 4 个空格可能变成缩进代码块而惰性）。正式可运行测试应使用规范的 Markdown 列表结构或顶格独立段落，如下：

````markdown
## @exec main

`x = 12`

`if x > 0`

`if x > 10`

`print("large")`

`else`

`print("small")`

`end`

`else`

`print("non-positive")`

`end`

`return x`
````

匹配规则：每个 `else` 只作用于**栈顶尚未结束的 `if`**；每个 `end` 关闭最近的 `if` 或 `while`。这是一套确定的编译期配对规则，不靠缩进/列表种类/解释器猜测。

多臂条件采用同一载荷内的 `else if <expr>`，允许零至多个条件臂和一个末尾
`else`。整组由一个 `end` 闭合，各条件从左到右按需求值。两个独立载荷
`else` 与 `if` 仍表示嵌套分支。详见 [RFC-CTRL-0005 的采纳记录](../adr/0067-structured-control-flow.md)。

## 4.5 `while` / `end`：不依赖 bullet List 的循环

````markdown
## @exec count

`params(n: Int)`
`i = 0`
`while i < n`
`i = i + 1`
`end`
`return i`
````

运行 `count(n=5)` 返回 `5`。

**循环语义：** 每一轮先求 Bool 条件；为 true 时执行循环体；为 false 时退出循环。与 V1 的 `Stmt::Loop` 语义一致。`end` 只在语法层关闭控制块，不额外执行任何操作。

**在引用块/有序列表/无序列表中书写也等价**，例如：

````markdown
## @exec count

`params(n: Int)`
`i = 0`

- `while i < n`

  > **i = i + 1**

`end`
`return i`
````

`-` 仅用于使循环体视觉上呈现为子项内容；它并不决定循环语义。即使将 `-` 改成 `1.`，程序应仍然按 `while` 执行。

## 4.6 `end`：为什么增加它，怎样严格约束

`end` 是当前候选规范最重要的新增**控制界定关键字**。它并不是一种新的计算能力；它只给 `if` / `while` 的作用域设置一个明确终点。

理由：如果让列表层级或 Markdown 引用块决定控制流终点，程序改成另一种排版时就可能改变执行意义。**要真正实现 `1.`、`-`、`>` 的“只是约定、不强制控制类型”，必须将控制范围从容器类型中解耦。** 显式 `end` 是一种小而可证明的解决方案。

严格规则：

- 每一个 `if` 恰有一个匹配的 `end`。可有多个 `else if` 和一个末尾 `else`。
- 每一个 `while` 恰有一个匹配的 `end`。
- `else if` 和 `else` 只能配对栈顶的 `if`。
- 独占载荷的 `break` 结束同一标题执行段中的最近循环，`continue` 跳至该循环的下一次条件检查。
- `end` 不能独立出现；语义片段末尾若仍有未闭合控制块则报错。
- 不允许控制块跨越**标题边界**（见第 6 章），也不能跨 H2 函数、不同 `@exec` 章节或知识区域。
- `if x > 0:` 带尾冒号首版不合法；规范语句不使用冒号，因为 Markdown 控制不由缩进决定。

## 4.7 `params(...)`：统一的函数参数声明

V1 曾将 H2 后“首个独立 CodeSpan `(n: Int)`”解释为函数签名，这是一个与载荷统一原则冲突的特殊规则。

V2 推荐显式声明：

````markdown
## @exec 加法

**params(a: Int, b: Int)**

`return a + b`
````

`params` 只在 H2 Exec 函数**第一条直接执行载荷**处合法，且每个函数最多一次；前面可以有普通说明文字，但不能已有赋值/调用/return，也不能把 `params` 藏在 List/BlockQuote 中。`params` 是编译期签名声明，不是运行时函数。对其外层用 `**`、`*`、CodeSpan，签名语义一致。

无 `params` 时，第一版可沿用已有的自由变量参数推断，但对公共 API 给出可配置的 lint，鼓励显式参数。类型表达与默认参数规则复用既有签名规范；不能从此处擅自承诺泛型、高阶函数或新类型系统。

## 4.8 当前不支持的写法：不要让实现者自行发明

| 写法 | 首版处理 | 正确替代 |
|---|---|---|
| `if x > 0:` | 语法错误 | `if x > 0` |
| `else x > 0` | 语法错误 | 嵌套 `if` |
| `elif x > 0` | 未定义关键字，语法错误或未知符号 | 嵌套 `if` |
| `return`（无表达式） | 语法错误 | `return None`（如果确实要返回 None） |
| `end if` | 语法错误 | `end` |
| `for x in xs` | 未纳入首版语法 | 标准库算法或 `while` |
| `try` | 不属于当前语法 | — |
| `a.b = 1` | 保留现有赋值能力限制 | 使用相应数据操作 API |
| `x = 1; return x` 单个载荷 | 语法错误 | 两个独立闭合载荷 |
| 直接写 `> print(x)` | 文档引用，不执行 | `> **print(x)**` |
| 直接写 `1. if x > 0` | 文档列表，不执行 | `1. **if x > 0**` |
| 让 `*x*` 返回 | V2 不返回 | `*return x*` |

---
# 第 5 章：精确的语法文法与确定性解释规则（实现者必须阅读）

## 5.1 两层语法、单一解析流水线（绝非双语义编译器）

编译输入依次经过：

1. **官方 Comrak/CommonMark/GFM Parser**：只做一次 Markdown 解析，得到 AST 与 UTF-8 source spans。
2. **标题模式解析器**：只使用 AST 的 Heading level 和标题首个普通 Text 子节点的精确词法位置，得到 H1–H6 模式信息。
3. **统一载荷映射器**：只在 `@exec` 且有所属 H2 Exec 函数的范围内，按 AST 顺序采集 Strong/Emph/CodeSpan。
4. **载荷语句解析器**：仅对已经明确选中的载荷文本，按正式 Marqdo 语句文法解释 `if/else/while/end/return` 等。
5. **控制结构组块器**：通过显式控制栈，将语句 token 降低到已有 IR。

严禁“先尝试当程序解析，失败就当普通文档”；严禁用普通中文说明、关键字出现频率、列表的首行是否看起来像条件，决定是否执行。

## 5.2 建议冻结的 V2 载荷文法

以下 EBNF 是**替换旧 Statement 规则的唯一新语句层**，其中 `expr`、`assignment` 和 `parameter_list` 应复用现有表达式/签名 Parser，不复制一套 Python 解析器。

```ebnf
(* 每一个独立 Markdown 载荷对应且只对应一条 statement。 *)
carrier_statement  = "if", required_space, expr, EOF
                   | "else", whitespace, "if", whitespace, Expr, EOF
                   | "else", EOF
                   | "break", EOF
                   | "continue", EOF
                   | "while", required_space, expr, EOF
                   | "end", EOF
                   | "return", required_space, expr, EOF
                   | "params", "(", parameter_list, ")", EOF
                   | assignment, EOF
                   | expr, EOF ;

(* assignment: 复用既有 assign_target '=' expr;
   函数调用仍属于 expr（作为单独载荷执行时结果丢弃）。 *)
```

**词法必须考虑完整单词边界**：`ifdef()`、`returning()` 不是 `if`、`return` 控制语句；反之，单独的 `if`、`else` 等当作标识符使用应明确报保留字错误。为了避免自相矛盾，Tokeniser 和 Parser 均应对关键字使用同一套保留字定义。

- `if`、`while` 后必须为可完整消费的表达式；表达式运行结果必须 Bool。现有条件纯度限制仍适用。
- `else if` 后必须有条件；独立的 `else` 和 `end` 后无额外词元。
- `return` 后必须是表达式；其作用范围是当前 H2 函数，不是当前段落/列表项。
- `params` 仅允许声明位置；作为普通表达式调用不得偷偷生效。
- 首版**没有**冒号、缩进结束、自动括号、换行分号语法糖。
- 空载荷 `****`、空 CodeSpan 等不得被忽略；对识别出的 Exec 载荷应报 `sem.empty_payload`。

## 5.3 从载荷序列组建结构化 IR 的严格规则

对每个 H2 Exec 函数，按各个**执行片段**的源码顺序处理。一个执行片段是标题自己的直接正文，不含下一标题的内容（H3–H6 是独立片段）。在单个片段里，先得到：

```text
TokenStream = [StatementToken(payload, span, carrier_kind, owner_unit), ...]
```

随后使用控制栈：

```text
open_if(condition):
    push IfFrame(arms=[(condition, [])], has_else=false)

on_statement(stmt):
    append stmt to the current innermost active body

on_else_if(condition):
    require stack.top == IfFrame and not frame.has_else
    append and activate a new conditional arm

on_else:
    require stack.top == IfFrame and not frame.has_else
    append and activate the final arm
    set frame.has_else

on_break_or_continue:
    require an open WhileFrame in the current segment
    append the transfer to the current innermost active body

open_while(condition):
    push WhileFrame(cond, body=[])

on_end:
    require stack not empty
    pop one frame
    lower it to Branch or Loop
    append lowered statement to enclosing frame/body

at_segment_end:
    require stack empty
    append the segment's complete IR statements to current H2 Function.body
```

该算法可以保证：

- `else if` 和 `else` 配对栈顶的 `if`，且只能出现在末尾 `else` 之前；不得跳过未关闭的 `while` 寻找更外层 `if`。
- `end` 只关闭栈顶最近的一层 if/while，没有匹配结构时报错。
- `return` 属于正在打开的 body，但运行时终止整个 H2 函数；降为现有 `Stmt::Return` IR。
- `else if` 和 `else` 是 `Branch` 的臂边界。解释器将 `break` 和 `continue` 传递到最近的循环。
- 相同载荷序列应产生相同的**归一化可执行 IR**，不因列表编号、引用层数、加粗/斜体/行内代码形态改变运算类别。

**一个实现注意点**：现有 `Branch` IR 的每个 arm 带 `condition`，没有专用可选 else；首版可以将兜底 else 降为最后一个 `true` arm，并明确保持既有“从左到右找第一匹配”的短路求值。如果未来 IR 引入更明确的 `else_body`，必须同步更改 IR schema、Trace 规范和**全部下游消费者及测试**；不能在本轮暗改行为。

## 5.4 容器的遍历：保留文档结构、但不赋予控制语义

推荐定义一个纯粹、可复用的 AST 遍历算子（伪代码）：

```text
Extract(node, mode):
  if mode == Knowledge: return []
  if node is inert subtree: return []
  if node is an allowed outer carrier: return [extract_one_payload(node)]
  if node is Paragraph, List, Item, BlockQuote:
      return concat(Extract(child, Exec) for child in source order)
  return []
```

遍历顺序必须是 **AST 文档序、子节点顺序**。禁止通过文件扫描另建“代码行”列表来重排执行顺序。

| Markdown 容器 | V2 编译器对它做什么 | 它**不**做什么 |
|---|---|---|
| 普通 Paragraph | 递归提取其中的允许载荷 | 不因为普通文字执行 |
| OrderedList | 按 ListItem 顺序提取载荷 | 不自动创建 Branch |
| BulletList | 按 ListItem 顺序提取载荷 | 不自动创建 Loop |
| BlockQuote | 按嵌套 Block 顺序提取载荷 | 不自动创建 Call |
| Heading | 切换/继承模式与片段 | 不作为语句、也不吞掉其后的 H2/H3 |
| Link | 保留引用关系 | 不从链接文字提取内部执行 |
| Table | 保留 GFM 结构 | 不把单元格代码片段当语句 |

**两个不可违反的原则**：

1. **单次拥有**：`Strong(Code("x=1"))` 仅提取外层 Strong 这一条；不能既执行 Strong 又执行 Code。
2. **不变量不丢**：同一载荷的 source span、原始 Markdown 节点类型与所属标题必须随 IR 生成后保留在可观察的分析数据中。

## 5.5 章节边界与控制流：禁止跨标题配对

如果一个 if 在某个 `### @exec` 小节打开，它的 `end` 必须写在**同一标题直接正文片段**中；不能跨到下一个 H3、H4，也不能跨到 Knowledge 子标题去关闭。

````markdown
## @exec main

`if x > 0`

### @knowledge 说明

这里是普通知识文本。

### @exec 续写

`end`
````

**预期：** 在 H2 直接正文片段末尾报 `sem.unclosed_control`；`### @exec 续写` 的 `end` 又是 `sem.unmatched_end`。即使字符串与两个节点都存在，编译器也必须拒绝“跨标题互相修复”。

为什么这样规定？标题是 Marqdo 已有的可寻址单元边界。如果 if 的有效代码跨过一个知识单元，那么在 Markdown 文档中插入或移动小节就可能无意改变控制流，严重损害局部分析和日后知识编译。**显式 `end` 不应跨单元隐式续接。**

同一 H2 函数的多个完整 Exec 片段，仍会按源码顺序进入它的函数体。例如：

````markdown
## @exec main

`a = 1`

### @knowledge 解释

这段不执行。

### @exec 第二阶段

`a = a + 1`
`return a`
````

此函数返回 `2`。知识片段没有执行效果，但也不会因为知识内容出现 `return` 一词而中断执行。

**解析片段分割的建议实现**：用顶层 Heading 序列记录每个 Heading 的 `direct_body` 范围（从该 Heading 到下一 Heading，不包括后代 Heading 本身），再根据 Heading 栈合成 H2 函数的执行片段列表；不应误把整个 H2 的 body 当成单一片段以致忽略 H3 的模式覆盖。

## 5.6 程序的显式声明与静态作用域

- H2 Exec 定义函数名、参数、返回路径；H1 可保留 Namespace。
- H3–H6 只是 Section，不改变函数名，不创建新的调用帧，不自动清空局部变量。`### @exec` 前后同一 H2 函数的本地变量可连续使用。
- 参数声明仅在 H2 的第一条直接执行载荷中合法；后续章节不得重新声明 `params`。
- `@knowledge` 中的标识符/CodeSpan 不参与 V2 参数推断，不增加符号使用，不触发运行时副作用。
- 已有导入 `import:`、包作用域、函数查找、显式/推断参数规则原则上保持；出现 V2 特例必须在规范中明确记载。
- 普通程序不应仅因源码从 Strong 换成 CodeSpan 就发生新的名称绑定、类型系统或函数调用语义。

## 5.7 可证明的确定性与其精确边界

令 `P` 为固定的 Markdown 解析器，`M` 为重构后**唯一的语义映射器**，`R` 为静态分析和 IR 构建，则：

```text
P(source, fixed_profile) = AST
M(AST, explicit_mode_rules) = IR or Diagnostics
R(IR, imports) = ExecutableProgram or Diagnostics
```

在 `source`、解析 Profile、导入图与唯一现行语法契约不变时，结果必须确定。

**不能承诺的事情**：仅凭 AST 可以知道作者有没有“想用加粗强调”。相同 Markdown AST 中的 Strong 在相同 Exec 位置只有一个既定含义——程序载荷。因此本规范保证确定性而不假装可以猜测人类意图。这个取舍必须在 README 的新语言语法介绍中直接告知开发者。

---

# 第 6 章：错误语义、真实边界和新手最容易踩的坑

## 6.1 三种完全不同的“空”

| 情况 | 意义 | 是否生成可执行语句 |
|---|---|---|
| `@knowledge` 中普通 Markdown 列表 | 文档节点 | 0 条 |
| `@exec` 中普通 Markdown 列表（没有可执行载荷） | 文档节点 | 0 条 |
| 运行函数结束但没有显式 `return` | 现有 `Unit` 结果 | 已执行已出现的语句，随后返回 Unit |

不要将上面任何一种误写成运行时的空 `List([])`；也不要因为该文件没有 `main` 就生成一个虚构的空函数。没有入口时 `run` 应报告无入口诊断。

## 6.2 常见错误与规范性诊断

| 错误文件片段（放在 Exec 中） | 必须如何处理 | 推荐诊断代码 |
|---|---|---|
| `## @knowledg main` | 未知模式声明，不能继承隐藏错误 | `sem.invalid_mode_marker` |
| `## @exec` | 缺少标题名称 | `sem.empty_unit_name` |
| `**if x >**` | 语法错误，不降级文档 | `sem.syntax` |
| `**else**`（无 if） | 无匹配分支 | `sem.unmatched_else` |
| `**else if true**`（无 if） | 无匹配分支 | `sem.unmatched_else_if` |
| `**else if true**`（位于末尾 else 后） | 条件臂位置非法 | `sem.else_if_after_else` |
| `**break**`（循环外） | 无匹配循环 | `sem.break_outside_loop` |
| `**continue**`（循环外） | 无匹配循环 | `sem.continue_outside_loop` |
| `**else**`（重复 else） | 重复，否则分支只允许一个 | `sem.duplicate_else` |
| `**end**`（无 if/while） | 不能独立结束 | `sem.unmatched_end` |
| `**if true**` 到片段结束无 end | 未闭合控制 | `sem.unclosed_control` |
| `**while 123**` | 类型非 Bool（按类型能力在静态或运行时报错） | `runtime.type` 等现有类型错误 |
| `**return**` | 需要表达式 | `sem.syntax` |
| `**return x; print(x)**` | 一载荷多语句 | `sem.syntax` |
| `**params(n: Int)**` 出现在函数中部 | 非法参数声明位置 | `sem.signature_position` |
| `### @exec 子节` 处于 Knowledge H2 之下且有 Exec 载荷 | 无合法函数归属 | `sem.exec_without_function` |
| `**重点**`（位于 Exec，未定义） | 这确实是载荷；应报未知变量 | `symbol.unknown` |
| `> print(1)`（没有闭合载荷） | 只是引用，不应报调用格式错误，也不执行 | 无错误 |
| `1. 普通事项` | 普通列表，不应报条件格式错误，也不执行 | 无错误 |
| `**`非法嵌套 Markdown`**` | 不允许在可执行载荷里非法嵌套 | `sem.payload_nested` |

代码命名是 **V2 诊断 API 的提案**；实施时可以以已有规范的 `markdown.*`、`expr.*` 命名策略形成正式名称，但必须保证 `code` 稳定、可用作自动化判断，`message` 只供人阅读。

## 6.3 错误不退化为知识，知识不自动提升为执行

以下两种程序必须始终保持不同语义：

````markdown
## @knowledge 说明

**return x +**
````

这只是文档，允许存在，因为它没有声称是程序语句。

````markdown
## @exec main

**return x +**
````

这是**显式执行区域中的非法载荷**，必须报告语法错误。绝不能试图“作者大概在写文档”而跳过。

另一个危险错误是：把标题里的 `@knowledg` 当作未知普通文字，从 H1 `@exec` 继续继承执行。这会使原本希望安全展示的代码被执行。该拼写应硬错误，避免安全意图降级。

## 6.4 新人写程序的推荐约定（风格，不是强制语义）

建议在官方例子中采用下列可读性约定：

- 使用 `# @knowledge` 为文档项目开头；在需要计算的 H2 明确写 `## @exec 函数名`，无需猜测继承。
- 对涉及 `*`、`**`、`<`、`>`、字符串的复杂表达式，优先选用独立 CodeSpan 或 Strong 包裹 CodeSpan（防止 Markdown 格式与运算符冲突）。
- 推荐独立一行一个程序载荷；`if/else/end` 各占一条载荷；虽然允许在普通段落中嵌套多个载荷，但会影响可读性与诊断。
- `1.` / `-` / `>` 用于呈现文档结构；不对它们赋予“必然分支/循环/调用”的心理预期。
- 把普通强调、算法解释、大段说明放在 `@knowledge` 章节，减少 Exec 区强调查错。
- 公共函数建议写 `params(...)`，复杂程序优先使用完整显式签名；若继续允许参数推断，必须按**新语法自己的规范**保留它，而不是因为 V1 兼容而保留旧签名特例。
- 不要写无条件的“语法糖容错”：看到 `if ...:`、错拼模式、未匹配 `end` 必须诊断，而非悄悄修复。

---
# 第 7 章：编译架构设计——具体修改到哪些 crate、哪些文件

## 7.1 技术边界：一次性替换语义映射，保留计算内核

本项目不是重新实现运行时：已有的数值计算、Memory、Resource、Capability、Plugin ABI 与托管值语义尽量原样保留。但**整个仓库所有 `.mq.md` 源码与其测试都必须同步换语法**，这一点不是可选的“后续迁移”。

```text
                        .mq.md（唯一新语法）
                                   │
                          Comrak/GFM（单解析器）
                                   │
                           原生 Markdown AST
                                   │
                  Heading/Unit + @exec/@knowledge
                         确定性模式继承与覆盖
                                   │
                    执行片段的统一闭合载荷提取
                                   │
                一套 Statement Parser（关键字）
                                   │
                 显式 if/else/while/end 组块
                                   │
                       现有 marqdo-ir
                                   │
               现有静态检查器 / Runtime / Host
                                   │
                      同仓库已迁移 stdlib/
```

**五条不可突破的边界**：

1. 只有**一套** Markdown AST、一套语义 mapper、一套语句 parser、一套 Runtime；不得创建 `map_v1.rs`、`map_v2.rs` 共存或 per-file 版本判别器。
2. **原位替换** `spec/mapping.md` 与 `spec/expression.ebnf` 的规范性内容；旧规范的历史价值由 Git 提供，而不是在 `spec/` 中继续作为“活跃可选版本”。
3. 控制关键字是语言表面语法，不是新 HostOp；映射至现有 Branch、Loop、Return/Assign/Eval IR，必要的 IR 字段变更须最小化。
4. `@exec` 不扩大资源访问权限；Resource/Host 授权和安全隔离不因这次语法重构改变。
5. 同仓库 `stdlib/` **随语法变更一起迁移并重新打包与校验摘要**，不得留下“新语法主程序 + 旧语法标准库”的过渡状态。

**集成纪律**：允许在开发分支上出现中途不通过的 commits，但不可将语义残缺的状态合并到主分支、打包、发版。最终合并门槛是一条从源文件到正式 CLI 的全仓库通过链路。

## 7.2 现有代码事实与破坏性重构修改清单

以审查 SHA `7fbe069...` 为基准。**不增加新的 crate；按职责小范围增改已有文件即可。**

| 文件/目录 | 当前职责 | 新语法落地动作 | 禁止留下的旧架构 |
|---|---|---|---|
| `crates/marqdo-markdown/src/ast.rs` | AST kinds 与字节 spans | 保持 CommonMark/GFM 结构和真实 span；增加 AST 探针测试 | 不加入第二个 Markdown lexer |
| `crates/marqdo-frontend/src/units.rs` | H1→namespace、H2→function、H3+→section | 精确剥离标题模式前缀、H1–H6 祖先继承；**只有 Exec H2 创建 callable** | 按旧 H2 无条件造函数 |
| `crates/marqdo-frontend/src/map.rs` | 按 Strong/Emph/Ordered/Bullet/Quote 解释指令 | **直接重写为唯一 mapper**：段落、列表、引用作为容器；载荷文本决定语句 | `map_v1.rs`、`map_v2.rs` 并存 |
| `crates/marqdo-frontend/src/payload.rs` | 文本/Code 源映射 | 统一 Strong/Emph/CodeSpan、单次提取、精确文件 span | 把 Emph 专门视为 Return |
| `crates/marqdo-frontend/src/signature.rs` | H2 首个纯 CodeSpan `(...)` 的特殊签名 | 移除旧特例，实现首个直接执行载荷 `params(...)` | 两套签名声明共存 |
| `crates/marqdo-frontend/src/analyze.rs` | 函数/导入/IR 装配 | 单路径连接模式索引、载荷与控制栈 | per-module 版本分派 |
| `crates/marqdo-expression/src/{lexer,parser,ast}.rs` | 独立计算表达式与基础赋值 | 同一词法语法加 `if/else/while/end/return/params`；不重写表达式计算 | `parse_statement_v1` 后备分支 |
| `crates/marqdo-ir/src/lib.rs` | Function/Stmt/Branch/Loop 等 IR | 尽量复用；必要时提升语义元数据和 source map | 并行的 `LegacyStmt` IR |
| `crates/cli/src/{loader,commands,main}.rs` | 装配、run/call/check/fmt/inspect | 移除语义版本选择；入口、诊断、帮助与样例仅按新规则 | `--legacy`、旧模块按 v1 加载 |
| `stdlib/**/*.mq.md` | 正式标准库源码 | **全数迁移**，API 名/值语义可保持；更新 digest、bundled 路径和 fixture | 包中存在旧的斜体返回等语法 |
| `spec/` | 权威规范 | 将 `mapping.md` / `expression.ebnf` 等**原位变更**为当前唯一规范，校对其他相关 spec | 活跃的 mapping-v1 与 mapping-v2 两份规范 |
| `examples/` | 公开可运行样例 | 全部源文件逐个重写和自动跑通 | README 仍展示旧式列表控制 |
| `tests/`、`crates/*/tests/` | conformance、negative、integration、security | 旧语义测试替换为新行为；Markdown AST profile 测试可留原始 Markdown 输入 | 旧语法“仍应通过”的断言 |
| `README.md`、`docs/`、`COMPATIBILITY.md`、`CHANGELOG.md` | 面向用户的介绍/说明/历史 | 现行文本全部更新；历史 ADR 归档/标记废止 | 活跃入口链接到旧语法教程 |
| `tools/`、CI 配置 | 打包/性能/检查 | 迁移扫描、示例执行检查、stdlib digest 校验与多平台门禁 | CI 仍将旧语法成功视为正确 |

**建议目标结构：保留现有 crate 组织，不因本轮工作新增版本目录。**

```text
marqdo-cli/
├── crates/
│   ├── marqdo-markdown/src/        # 仅 Markdown AST/Profile
│   ├── marqdo-expression/src/      # 唯一词法/语句/表达式解析
│   ├── marqdo-frontend/src/
│   │   ├── units.rs               # 标题角色和模式继承
│   │   ├── payload.rs             # 统一载荷与 source span
│   │   ├── map.rs                 # 唯一 Markdown→Stmt 映射与控制栈
│   │   ├── signature.rs           # params 声明（无旧写法）
│   │   └── analyze.rs             # 单入口
│   ├── marqdo-ir/                 # 共享 IR
│   ├── marqdo-runtime/            # 既有值/内存/资源执行语义
│   ├── marqdo-host/               # 既有权限和系统边界
│   └── cli/                       # 单语法 CLI
├── stdlib/                        # 全部已更新的单版本官方标准库
├── spec/
│   ├── mapping.md                 # 当前唯一 AST→IR 规范
│   ├── expression.ebnf            # 当前唯一语句/表达式 EBNF
│   └── markdown-profile.md        # Comrak/GFM 解析 profile
├── examples/                      # 只有新语法可运行样例
├── tests/                         # 以“新语法”划分功能，而非版本
└── docs/                          # 现行指南+显式历史目录
```

开发期间可以把不同职责的测试放在 `tests/semantics/heading_modes/`、`tests/semantics/control/` 等，但**不得创建 `tests/v1/` 与 `tests/v2/` 的永久双轨布局**。

## 7.3 数据结构设计：把新的概念压到最少

推荐在 Frontend 内部（不是对外语言 Value）使用：

```rust
// 设计草图，不代表现有 API
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SemanticMode { Knowledge, Exec }

struct HeadingMode {
    unit: UnitId,
    mode: SemanticMode,
    origin: ModeOrigin,       // Explicit | Inherited { ancestor }
    marker_span: Option<Span>,
}

struct ExecutableSegment<'a> {
    owner_function: FunctionId,
    owner_unit: UnitId,
    direct_blocks: &'a [MdNode],
    mode: SemanticMode,       // 只接受 Exec
}

struct StatementPayload {
    text: String,
    file_segments: Vec<PayloadSegment>,
    outer_span: Span,
    carrier: CarrierKind,     // Strong | Emph | InlineCode
    owner_unit: UnitId,
}

enum V2StmtToken {
    If(Expr), Else, While(Expr), End, Return(Expr),
    Assignment(AssignTarget, Expr), Eval(Expr), Params(Signature),
}
```

结构必须能够保持：同一个 AST 不被重复遍历造成重复执行；每个 Payload 可追溯到精确 UTF-8 span；每个参数/控制词可追溯到原始语句；语义模式与 H2 Callable 身份是**不同字段**。

## 7.4 唯一 AST→IR 生成流水线

不能继续以旧 `Mapper::block()` 为默认路径；必须将其**替换**为新规则：

```text
1. parse_markdown_once
2. build_heading_index_and_resolve_modes
3. index H2 Exec callable functions
4. collect document resources (inert)
5. traverse direct Exec function segments in source order
6. extract eligible closed payloads exactly once
7. parse every payload under one statement grammar
8. pair if/else/while/end within permitted segment scope
9. build Function.body / source maps
10. infer/validate signatures; resolve calls; emit IR
11. existing runtime and capability boundary
```

- 普通 Markdown 容器不触发语句；Knowledge 节点的可执行投影为空。
- Exec 有效载荷的语法错必须报错；不能回退成文档。
- 控制块不能跨越被规范设为终止边界的 Heading 片段，`end` 必须显式闭合。
- 若当前源码缺少 `@exec`，不得“猜它是旧语言”；将其按 Knowledge 规则处理。对旧 `main`，`run` 应明确报告无可调用入口，而不是执行旧式 Strong/Emph。
- 不允许在 CLI loader 内部临时维持旧 mapper 来“让一部分 fixture 继续通过”。

## 7.5 Source Span、UnitId 与 Analysis/Trace 的破坏性版本管理

- Heading 显示文本保留，语义名字剥离 `@exec`/`@knowledge` 后沿用 NFC/空白归一化，唯一性按新 Unit 索引检查。
- `ModeOrigin`（Explicit/Inherited）进入 inspect/Analysis，以便编辑器解释某节点为何参与执行。
- byte Span 不准丢；BOM/CRLF/CJK/Unicode source map 必须有 golden。
- Analysis、Trace、CLI JSON、外部 Tool Protocol 若因模式索引、FunctionId 或 IR 产生**公共 schema 变更**，需按各自协议规范评估版本提升并更新**全部消费者和 fixture**；不必为了运行旧源文件而做兼容转换层。
- 现有 `spec/analysis-v1.md`、`spec/trace-v1.md` 是结构化接口的历史版本名，并不表示它们可控制 `.mq.md` 旧语法。**源语言破坏性升级**不意味着所有无关 ABI/协议都必须强制破坏；但如确有破坏，就必须版本化协议并迁移消费者。
- 必须显式记录 `semantic_revision` 作为新工具输出的**描述性字段**或 schema 契约；它不是一个源文件可选择的解析器分支。
- 语法升级时清理因旧规则产生的缓存/锁文件构建派生物，避免将旧 IR 当成新 IR 执行。

## 7.6 与运行时、权限及标准库的边界

- 本轮**不重新设计** Memory/Resource/Value；新关键字只是 Frontend 语法并降级为原有 IR，不新建 `HostOp::If/Return`。
- `@exec` 绝非权限授权；现有 Strict/Project/Pure、安全隔离、文件/网络授权仍是系统层决定。
- `stdlib/` 既保持在同一仓库，又必须在**本轮全部迁移**。其中 `format.mq.md`、`io.mq.md`、`text.mq.md`、`data/table.mq.md`、`net/http.mq.md` 和 `stdlib/tests/*.mq.md` 都不得留旧式 `*return*`、有序分支或 bullet 循环。
- 应**重算** `stdlib/stdlib.json` 中的 digest，并验证 frozen CLI 离线加载和相同摘要的构建重现；源码变更会改变包身份和 Analysis snapshot，这是预期的破坏性变更，不能硬改 expected digest 以掩盖问题。
- 先保证已有函数的 API/行为在新语法下成立，**不顺手增加 stdin 等新标准库功能**，否则无法区分语法重构缺陷与扩展功能缺陷；缺口另设 issue。
- 已锁定依赖的包如果只提供旧语法，需更新包和 lock，不能继续自动调用旧 mapper。

---

# 第 8 章：破坏性重构策略——不存在 V1 兼容路径

## 8.1 决策与发布契约

**本轮是原位、单语言、破坏性的语法升级。** 无论源文件是否有 Front matter、来自用户工作区还是 `std:`、来自锁定依赖还是 CLI 示例，全部使用唯一新的 AST→IR 映射。**不允许** `map_v1`、`parse_statement_v1`、`--legacy`、按文件语义版本切换、跨语法模块调用和双份 active spec。

升级前的代码不是“保留旧格式后仍能运行”，而是**需要升级的源文件**。CLI 的发行说明应显著列为 breaking change，必要时提升 Marqdo 的语言契约版本和 CLI 发行版本。一次性的迁移脚本**可以**作为开发期离线工具辅助改写仓库，但不得引入生产 CLI 的运行时语法兼容路径，不得自动吞掉不可翻译的旧语法。

## 8.2 原有语义到新语义的迁移规则（强制应用至全部代码资产）

| 旧写法/行为 | 必须转换的新写法/行为 | 语义保真检查 |
|---|---|---|
| H2 无模式却默认为函数 | `## @exec 名称`，或从有效 Exec H1 继承 | `run/call` 入口不变、函数名剥离标记 |
| `**x=1**` 执行 | 在 Exec 范围仍可用，或统一 CodeSpan | 显式 Exec 下仅执行一次 |
| `*x*` 意味返回 | `*return x*`、`**return x**` 或独立 `` `return x` `` | return 路径和出口一致 |
| 有序列表互斥分支 | 显式 `if` / `else` / `end`；多臂条件需**保序、短路、只执行第一真分支** | 不能将互斥语义误写成并列多个 if |
| 无序列表各项独立循环 | 各自 `while cond` / `end` | 不得意外合并多个 while |
| `> print(x)` 自动调用 | Exec 下明确 `> **print(x)**` 或 `` `print(x)` `` | 原来调用次数/顺序一致 |
| H2 首段纯 CodeSpan `(n: Int)` 签名 | H2 Exec 第一条直接载荷 `params(n: Int)` | 参数顺序、默认值、名称相同 |
| 强制列表条件 / quote 调用导致报错 | 普通 Markdown 列表/引用成为容器或知识内容 | 对纯文档不再生成旧式错误 |
| CodeSpan 原先惰性（段落内） | Exec 下完整合法 CodeSpan 可成为载荷 | 原有文档必须明确标到 Knowledge，防止新增副作用 |
| 原文件缺语义标记 | 加标题级显式声明或按祖先继承 | 不允许依据正文内容猜执行意图 |

迁移采用 AST 角色比对，而不是只做正则替换。遇到嵌套强调、特殊链接调用、段落内多载荷、旧多臂短路等情况，无法静态保真就**停止并人工改写该文件**。完成后废弃迁移专用临时脚本或放入明显的 `tools/dev-only/`，不得长期维护旧语言编译器。

## 8.3 仓库文档及测试的全量迁移边界

此次迁移**必须覆盖所有活跃资产**，而不只是 `spec/mapping.md`：

1. **用户入口**：`README.md`、`COMPATIBILITY.md`、`CHANGELOG.md`、`docs/STATUS.md`、`docs/contributor-guide.md`、`tests/README.md`、`stdlib/README.md`、`examples/**/README.md`、CLI `--help` 与错误提示。
2. **权威语言规范**：`spec/mapping.md`、`spec/expression.ebnf`、`spec/markdown-profile.md`（若 parser settings 未改只需说明语义变化）、`spec/evaluation.md`、`spec/references.md`，以及引用执行规则的数学/安全/包/ABI spec。
3. **官方源码**：`stdlib/**/*.mq.md`、`examples/**/*.mq.md`、仓库全部 `.mq.md`（包括测试 fixture、项目模板 `INIT_TEMPLATE`、Rust/Python 测试中内嵌字符串）。
4. **测试与黄金文件**：`tests/conformance/ast/**`、`tests/conformance/semantics/**`、`tests/negative/**`、`tests/security/**`、`crates/cli/tests/**`、各 crate 内联 `#[test]`/`#[cfg(test)]` 测试、协议消费者用的示例数据和 snapshot。
5. **构建与发布**：`stdlib/stdlib.json` 摘要、`tools/pack-stdlib.py` 的打包验收、CI 工作流、跨平台 build/test、Tool/Analysis/Trace 的消费者脚本与 schema（只有公共协议确实变化才改 schema）。
6. **历史资料**：`docs/adr/*` 与历史 RFC/审核报告中描述旧语法的内容不必伪造成新决策，应保留为**显式归档**，在顶部标明 `Historical / Superseded` 并给出当前唯一规范链接；从 README/架构索引删除到旧语法的“现行指引”链接。历史文件中的旧例子不得被当作活跃可运行教程或自动校验的代码示例。

**每一文件必须有迁移账目**：`path | type | old semantics? | required action | owner | test gate | status`，不能用“grep 没找到 return”代表已经审阅全部仓库。

## 8.4 仓库真实资产基线与差异化处置

基于 `7fbe069...` 递归 Git tree，审查时约 **942 个文件**。以下数量是**审查 SHA 的目录统计，不是当前工作分支的不变真理**；实施时必须重新导出完整文件清单。

| 范围 | 基线规模 | 迁移要求 |
|---|---:|---|
| `docs/` | 约 92 文件，包含 39 个 ADR | 活跃规范与教程重写；历史决策标记废止/历史 |
| `spec/` | 21 文件 | 对执行/语义映射的规范原位升级，引用关系审查 |
| `examples/` | 44 文件 | 所有可运行 `.mq.md`、项目配置和 README 一起更新 |
| `stdlib/` | 11 文件；含 9 个 `.mq.md` | **全部**标准库和 self-test 重写，更新摘要 |
| `tests/` | 约 632 文件；AST conformance 320 文件、语义 conformance 146 文件 | 按测试性质分别保留 Markdown profile 或重写语义黄金 |
| `crates/cli/tests/` | 17 个集成测试文件 | 审计 Rust 内嵌 `.mq.md` 字符串、快照、预期诊断 |
| `crates/` 其他目录 | 多个单元测试/夹具 | 搜索并重写所有携带旧语义的源码样例 |

**最容易做错的地方**是 `tests/conformance/ast/`：它验证原生 Markdown AST 的解析，某些只测试强调/有序列表结构的源文件**可以保留**，因为 Markdown AST 没有变化；但任何从该输入进一步断言 `Return/Branch/Loop` 的测试都必须更新。测试是否修改，应由其**验证目标**决定，而非机械批量把每份 Markdown 都加 `@exec`。

`tests/conformance/semantics/` 的约 73 组语义样例，以及 `tests/negative/` 中的旧“列表必须有条件”“引用块只能调用”等断言，需要彻底审查：许多原负例在新语法下应成为**合法惰性 Markdown**，不能继续期待报错；另一些旧负例需要替换为 `if` 缺少条件、悬空 `else`、缺少 `end`、非法 `params` 位置等负面样例。

## 8.5 必须重新审查的现行文档（逐一可执行的改写要求）

| 文件 | 修改动作 | 完成标准 |
|---|---|---|
| `README.md` | 以 `# @knowledge` / `## @exec main` 开篇；重写 Hello、Factorial、Loop、Return；明示旧语法不支持 | 第一个 Quickstart 可直接运行，源码不需额外版本字段 |
| `docs/DEVELOPMENT_BLUEPRINT.md` | 将“Strong→执行/Emph→Return/List→Branch/Quote→Call”的设计替换；若已过时标示历史并链接新规范 | 无旧语法被当作现行设计 |
| `docs/STATUS.md` | 更新真实完成度、最新 SHA、语义重构状态和测试结果 | 不声称未跑的平台通过 |
| `docs/architecture/README.md` | 指向一个生效的语言规范入口；历史 RFC 明确不是活跃编译契约 | 从入口找不到相互冲突的现行映射 |
| `docs/contributor-guide.md` | 写出语法变更流程、fixtures、fmt/golden 更新和审核清单 | 新贡献者无需聊天背景可提交语法 PR |
| `docs/CORE_STABILIZATION_AND_ECOSYSTEM_GATE.md`、`docs/NEXT_PHASE_IMPLEMENTATION.md` | 清理旧分期、旧兼容目标，与本计划合并或归档 | 无两份相冲突的开发路线图 |
| `docs/MARQDO_COMPUTE_MACHINE_RFC_0001.md`、`docs/MARQDO_COMPUTE_CORE_STDLIB_MIGRATION_RFC_0002.md` | 保留计算模型有效内容，更新示例语法与前端假设；历史提案标注覆盖范围 | 不把旧例子当新 CLI 可运行程序 |
| `COMPATIBILITY.md` | 将旧源语法列为不兼容；同时独立呈现 OS/ABI/Plugin 兼容矩阵 | “兼容”不暗示旧语法可以执行 |
| `CHANGELOG.md`、`docs/releasing.md` | 明确 breaking change、所需代码改写、依赖更新与正式门禁 | 发布者知道这是破坏性升级 |
| `tests/README.md` / `stdlib/README.md` | 描述新的测试分类与新样例 | 文档中的命令与实际路径匹配 |
| `spec/mapping.md` | **原位完整替换**为新 AST→IR 映射 | 不再存在 V1 语义作为可选路径 |
| `spec/expression.ebnf` | 以关键字语句和现有 expression 组成唯一文法 | `if/else/while/end/return/params` 均有规范与反例 |
| `spec/evaluation.md` | 更新 Return/Branch/Loop 来源及跨标题边界，保留数值/Memory/Resource 运行语义 | 只变语法、不悄悄更改结果/权限 |
| `spec/references.md` | 复核 `@exec/@knowledge` 剥离后的标题名与 FunctionId/ref 解析 | 文档链接、模块锚点不会错位 |

**历史 ADR 操作规范**：历史文本中旧语法示例保留且注明“当时语法，仅供决策考古”。若旧 ADR 的决策已被本轮推翻，必须新增一份替代 ADR 并在旧 ADR 顶部加 `Superseded by ADR-...`；不要悄悄修改 ADR 日期或把历史叙述改写成仿佛当年已接受新语法。

## 8.6 测试集的全面改造规范（不是简单 regenerate golden）

| 测试类别/路径 | 操作 | 新验收逻辑 |
|---|---|---|
| `tests/conformance/ast/` | 保留无关的 Markdown AST golden；增加模式前缀 H1–H6、closed carrier、BOM/CRLF/CodeSpan AST fixtures | 原生 Markdown AST 不因语义升级被篡改 |
| `tests/conformance/semantics/` | **逐组改写**旧 Strong/Emph/List/Quote 语义；按照真实程序插入 Exec 标题、return/if/while/end/params | 值、可观测副作用与错误类型符合新规则 |
| `tests/negative/*.mq.md + *.expected.json` | 分类：旧负例变正例、负例保留但原因改变、旧负例删除并由新负例替代 | 不保留错误的“普通列表必须报错”预期 |
| `tests/negative/runtime/` | 语法移植，保留真实运行时负例（越界、类型、权限等） | 错误仍来自 Runtime 而不是旧格式解析失败 |
| `tests/negative/warnings/` | 重新定义“Knowledge 惰性、无可调用 main、Exec 无所属函数”等 warning/error | 不保留 V1 `markdown.inert_executable` 的错误预期 |
| `crates/cli/tests/conformance_*.rs` | 更新 golden harness 的源语义假设 | 检查当前唯一扫描器，不按文件分流 |
| `crates/cli/tests/negative.rs` | 更新反例分类和新错误码 | 负面测试不靠老规则凑数量 |
| `crates/cli/tests/examples_r6.rs` / `integration_cli.rs` | 迁移 CLI 用例中硬编码源码、命令与 expected JSON | run/call/check 都能操作新语法 |
| `crates/cli/tests/print_routing.rs` | 迁移旧语法的载荷，保留 stdout 路由与可观测结果 | 不会因改写误测 `HostOp::Print` 回退 |
| `crates/cli/tests/stdlib_*`、`p2_closure.rs` | 使用新标准库源码和新 digest 跑离线冻结二进制闭包 | 冻结 CLI 可以完全使用新语法的 stdlib |
| `crates/cli/tests/resource_streams.rs` / `http_over_tcp.rs` | 对 fixture 的 Markdown 语句做迁移，保留真实文件/TCP 行为断言 | 资源权限、短写、EOF 的回归通过 |
| `crates/cli/tests/analysis_trace.rs` / `tool_protocol.rs` | 检查 UnitId/FunctionId/Span/Trace 与 schema 是否变化；更新消费者 | CLI 工具输出不包含旧 mapper 身份 |
| `crates/cli/tests/package_env.rs` / `plugin_worker.rs` | 迁移依赖包和插件演示中的 `.mq.md`，更新 lock/摘要 | 不通过旧语法旁路来让插件测试通过 |
| `crates/cli/tests/properties.rs` | 增加 Strong↔Emph↔Code、List↔Bullet↔Quote 不变性和知识惰性性质 | 比较归一化 IR + 运行结果/trace |
| 各 crate 的源码内 `#[test]` | 精确审计嵌入的 Markdown 文本、错误码和解析假设 | Rust 单元测试也使用唯一语法 |
| `tests/security/` | 更新 helper 及正向执行 fixture，不降低安全拒绝要求 | 相同 sandbox 策略仍然 fail-closed |
| `tools/pack-stdlib.py` 与发布 CI | 校验打包摘要和语义语料闭包 | 未迁移样例/测试不得通过打包门禁 |

**强制 golden 更新规范**：每次变更先人工或受控生成“新规范正确的期望”，再运行测试；只有当新输出有明确语义解释时才能更新 golden。禁止 `UPDATE_GOLDENS=1` 一键覆盖所有预期并把语义变化视作已经审查。

## 8.7 全库扫描与可追溯迁移台账

在同一基线生成仓库文件台账（建议提交 `docs/migration/semantic-rewrite-inventory.tsv`，完成后也可保留审计证据），至少包含：

```text
path<TAB>category<TAB>active<TAB>old_semantics<TAB>action<TAB>owner<TAB>verification<TAB>status
```

扫描顺序：

1. 使用 `git ls-files` 遍历所有跟踪文件（不只 `.mq.md`）；另用 `rg` 发现 README、Rust 测试常量、文档围栏和模板中的旧语法模式。
2. 通过解析 Markdown AST 区分普通 Markdown 强调与执行节点，不允许纯 regex 自动批量改写代码。
3. 标记 `active/spec`、`active/tutorial`、`active/example`、`active/stdlib`、`active/test`、`historical/adr`、`generated/golden` 和 `non-semantic`。
4. 对每个语义受影响文件，记录新语法写法、关联测试编号和审查者。跨文件引用以最新 UnitId/FunctionId 重新验证。
5. **迁移覆盖率 = 100% 已分类文件**；活跃的 `.mq.md` 必须全部可按唯一新语法分析、运行或按预期负例拒绝。历史文档可保留旧示例，但必须有明显历史声明并从活跃规范入口隔离。

可作为 CI **初步扫描**（仅找候选，不作为正确性证明）：

```bash
git ls-files '*.mq.md' '*.md' '*.rs' '*.py' '*.json' '*.yml' '*.yaml' > /tmp/marqdo-semantic-files.txt
rg -n 'Strong.*执行|Emph.*返回|OrderedList.*分支|BulletList.*循环|BlockQuote.*调用|inert_executable|map_v1|map_v2|marqdo_semantics|--legacy'   README.md COMPATIBILITY.md CHANGELOG.md docs spec examples stdlib tests crates tools || true
```

**CI 最终证明不能依赖 grep 零匹配**：必须结合 manifest 台账、真实 AST/IR 编译、全量示例执行、负例诊断、stdlib digest、文档代码样例检查和冻结二进制 E2E。

## 8.8 不应为了破坏性重构添加的“多余东西”

- 不新增兼容 crate、第二解析器、第二语法版本 dispatch、`--legacy` 或源文件语义版本标记。
- 不为旧语法永久保留多一套教程、golden、stdlib 子目录或测试执行器。
- 不以删除大量负例/测试来“提高通过率”；旧测试若不再适用，必须以新的等价责任测试替换并记录。
- 不改变 Memory、Resource 的抽象语义；不借语法重构添加 Host API 或无关功能。
- 不让历史 ADR 在正文中未经标注地继续宣称旧语法现行有效。

---

# 第 9 章：阶段实施计划（全仓库同步交付）

**开发分支可以分多个提交实现，但不能把只有 Frontend 换了、标准库和文档未迁移的状态作为可发布中间产物。** 每阶段应明确“代码变化 + 文档变化 + 测试/黄金变化 + 全库状态”，最终合并必须完整闭环。

| 阶段 | 具体交付 | 阻断性验收 |
|---|---|---|
| **S0：规范冻结与资产台账** | 决定唯一语法、真实 Comrak AST probes、所有文件的分类和 owner；批准破坏性 ADR | 100% 跟踪文件分类，禁止源语义版本分流 |
| **S1：模式/标题索引替换** | 在 `units.rs` 原位实现 H1–H6 标记继承、知识与可执行 H2 分离 | 标题 mode 与 Unit/FunctionId golden 全部通过 |
| **S2：统一载荷替换** | 直接重构 `payload.rs`、`map.rs`；删旧 Strong/Emph/Quote/List 专用分支 | 每种闭合标记都只执行一次；普通列表/quote 无固定执行义 |
| **S3：关键词控制与签名** | 单套 Statement Parser；`if/else/while/end/return/params`、控制栈和 source span | 完整文法正反例、嵌套和跨标题负例通过 |
| **S4：官方标准库迁移** | `stdlib/**/*.mq.md` 全数改写，更新 `stdlib.json` 和 `std:` loader self-test | frozen CLI 离线加载、print、HTTP、Table、UTF-8 测试通过 |
| **S5：官方示例和文档迁移** | README、现行 docs、spec、`examples/`、CLI Help、init 模板全部迁移 | Quickstart 全部可复制运行；无旧语法现行指引 |
| **S6：全量测试语料替换** | semantics 146 文件逐项、negative/runtime/warnings、Rust 内嵌 fixture、协议 schema/golden 差异审计 | 所有新负例/正例充分覆盖，不允许保留旧期待行为 |
| **S7：跨模块与安全回归** | lock v3、Plugin/Worker/Foreign、Tool/Analysis/Trace、strict sandbox、Memory/Resource | 语法升级不绕过资源权限，错误/Trace 有可解释变更 |
| **S8：完整仓库 RC 审计** | 全量 CI、真实平台状态、Breaking release notes、迁移覆盖报表 | 100% 迁移清单闭合；单语法全仓通过；无双 mapper |

**推荐提交切分**：可以是 `docs(spec)` → `frontend/expr` → `stdlib` → `examples` → `tests` → `ci/release`，但不可让 Cargo/stdlib 的暂时失败成为长期的主分支状态。若使用 PR，最稳妥的是一个特性分支持续集成，最后一次合并而非多次把未迁移状态合入 main。

## 9.1 开发规范与职责划分

- 语言团队只改 Markdown AST→语义投影、Expression Parser、Function/Unit 索引；运行时团队确认无新增语义边界需要改动。
- 标准库团队负责迁移 `.mq.md`，**不得**借旧语法继续在 `std:` 模块中工作。
- 测试团队按业务行为和语法形态双维度建设 fixtures，逐条记录旧测试被新测试接替的位置。
- 文档团队负责所有现行入口和 README 实例可运行性，维护 `docs/migration/` 台账。
- 审查者对每个提交检查：是否引入第二语法入口？是否有未迁移 `.mq.md`？是否更新对应 spec/test？

## 9.2 每阶段最小交付格式

```text
阶段编号：Sx
变更文件：...
已改现行文档：...
替代/新增测试：...
运行命令与结果：...
受影响的 stdlib digest / protocol fixture：...
剩余未迁移文件及 owner：...
未完成的真实平台验证（不得冒充 PASS）：...
```

## 9.3 阶段与门禁的依赖关系

- `S1/S2/S3` 可在同一分支重构，但在准备 S4 前必须能编译并运行本规范第 13 章的纯计算示例。
- `S4` 完成之后，必须验证一份使用 `std:io` 的 Hello 程序可完全在新的单语法 CLI 上运行。
- `S5/S6` 不得互相等待导致文档与测试长期背离，应针对每份现行示例同时提交相应 fixture。
- `S7/S8` 需验证没有旧 mapper、旧模块调用路径或旧规范入口，尤其是跟踪目录中没有独立的 legacy test runner。

---

# 第 10 章：完整语法与行为测试矩阵（用于直接创建 fixture）

本章使用**固定输入 → 精确行为**表达测试要求。每个正例必须同时检查 Parser AST、模式、载荷序列、规范化 IR、执行结果；每个反例至少检查稳定错误码、源跨度和“不得静默执行”。

## 10.1 A 组：标题模式、函数归属和单语法（A01–A18）

| ID | 最小测试场景 | 必须验证 |
|---|---|---|
| A01 | 文件没有任何语义版本 Front matter | 直接按唯一新语法处理 |
| A02 | Front matter 提供 `import` 但无语义版本 | 导入可解析，只有新语法 |
| A03 | `# @knowledge 项目` | H1 Knowledge，语义名为 `项目` |
| A04 | `# @exec 项目` + `## main` | H2 继承 Exec，成为函数 |
| A05 | `## @exec main`（无 H1） | 根 H2 可为 Exec 函数 |
| A06 | `## @knowledge 注释` | 知识单元，不是 Callable |
| A07 | `### @exec 过程`（Exec H2 下） | 属于 H2，执行语句追加 |
| A08 | H4 Knowledge 与同级 H4 Exec | 同级分别继承父级，无 sibling 泄漏 |
| A09 | H1 Exec → H2 Knowledge → H2 无标记 | 后者继承 H1 Exec |
| A10 | H1 Knowledge → H2 Exec → H3 Knowledge → H3 Exec | 仅 Exec 片段产出 IR |
| A11 | H2 Knowledge 下 H3 Exec 带载荷 | `sem.exec_without_function` |
| A12 | H1 Exec 下无所属函数的载荷 | `sem.exec_without_function` |
| A13 | `## @exec`（无真实名称） | `sem.empty_unit_name` |
| A14 | `## @knowledg main` | 明确错误并提示 `@knowledge` |
| A15 | `## @Exec main` | 非法模式标记，不能猜测大小写 |
| A16 | `## @exec @knowledge main` | 冲突声明 |
| A17 | `## 关于 @exec 的说明` | 标题普通正文，不改变模式 |
| A18 | 含 `marqdo_semantics` Front matter | `sem.unsupported_version_directive`，不得分派旧编译器 |

## 10.2 B 组：载荷等价与惰性边界（B01–B18）

| ID | 输入变体 | 必须验证 |
|---|---|---|
| B01 | `**x = 1**` | 一个 Assign |
| B02 | `*x = 1*` | 同 B01 的 Assign |
| B03 | `` `x = 1` `` | 同 B01 的 Assign |
| B04 | `**`x = 1`**`（实际按真实 AST 验证） | 恰好一次 Assign |
| B05 | `*`x = 1`*`（实际按真实 AST 验证） | 恰好一次 Assign |
| B06 | `**return x**` | Return(x) |
| B07 | `*return x*` | 同 B06 |
| B08 | `` `return x` `` | 同 B06 |
| B09 | `*x*`（在 Exec 中） | Eval(x)，不是 Return |
| B10 | `**重点**`（在 Knowledge 中） | 零指令，无未知变量错误 |
| B11 | `**重点**`（在 Exec 中） | 参与表达式解析，无对应变量则 symbol.unknown |
| B12 | `> 普通引用正文` | 零指令，无调用错误 |
| B13 | `> **print(1)**` | 一次调用 |
| B14 | `1. 普通事项` | 零指令，无条件错误 |
| B15 | `- 普通事项` | 零指令，无循环错误 |
| B16 | Table Cell 中含 `**print(1)**` | 零指令 |
| B17 | 链接 Label 或图片 Alt 含闭合标记 | 零指令 |
| B18 | 复杂非法嵌套 Strong/Emph/Link | `sem.payload_nested`；不能部分执行 |

## 10.3 C 组：控制关键字、作用域与副作用（C01–C22）

| ID | 场景 | 预期 |
|---|---|---|
| C01 | `if true ... end` | then 运行一次 |
| C02 | `if false ... end` | then 不运行 |
| C03 | `if false ... else ... end` | else 运行一次 |
| C04 | `if true ... else ... end` | else 不运行 |
| C05 | if 嵌套 if | 最近未结束 if 优先配对 |
| C06 | if 嵌套 while | while 完成后再配对外层 if |
| C07 | while 3 次 | 条件逐轮求值，确切循环次数 |
| C08 | while 假条件 | 循环体零次 |
| C09 | `return` 在 if 内 | 退出整个 H2 函数 |
| C10 | `return` 在 while 内 | 退出整个 H2 函数 |
| C11 | `end` 无 open | `sem.unmatched_end` |
| C12 | `else` 无 if | `sem.unmatched_else` |
| C13 | `else` 对应栈顶 while | 必须拒绝 |
| C14 | if 两次 else | `sem.duplicate_else` |
| C15 | if 缺 end | `sem.unclosed_control` |
| C16 | while 缺 end | `sem.unclosed_control` |
| C17 | 控制块跨 H3 标题 | 不允许跨越，明确关联 Span |
| C18 | `if x > 0:` | `sem.syntax` |
| C19 | `end if` | `sem.syntax` |
| C20 | `return` 无表达式 | `sem.syntax` |
| C21 | `ifdef()`、`returning()` | 视为普通标识符调用，而非关键字前缀 |
| C22 | if 条件非 Bool | 既有 static/runtime type 错误 |

## 10.4 D 组：参数、名称、标准库与跨模块（D01–D16）

| ID | 场景 | 必须验证 |
|---|---|---|
| D01 | `params(x: Int)` 是 Exec H2 第一条直接载荷 | 显式签名正确 |
| D02 | 同参数声明换 Strong/Emph/Code | 相同签名 |
| D03 | `params` 出现在 List/Quote 内 | `sem.signature_position` |
| D04 | `params` 出现在其他语句之后 | `sem.signature_position` |
| D05 | 同函数第二次 `params` | 拒绝重复声明 |
| D06 | 没有 `params` | 对实际 Exec 载荷进行自由变量推断，不读取 Knowledge 文本 |
| D07 | Knowledge 中的未定义名 | 不进入程序符号分析 |
| D08 | H3 Exec 读取父 H2 的已赋值变量 | 同一调用帧 |
| D09 | H2 Knowledge 被 `call` | 拒绝，给出明确错误 |
| D10 | **新语法**模块导入**新语法**同仓库 stdlib | 成功解析执行，stdout 等效 |
| D11 | 依赖包仅有旧式源语法 | 编译拒绝，不能启用兼容 mapper |
| D12 | 资源权限拒绝 | `@exec` 不绕过 Capability |
| D13 | Memory 写入载荷在多种 Markdown 载体中等价 | Byte 写入与读回结果相同 |
| D14 | 函数无 Return | 返回 Unit，不等于 None |
| D15 | 语义名剥离模式前缀 | UnitId/FunctionId 一致且可查找 |
| D16 | 已升级的项目依赖锁文件 | digest/instance identity 可重现，无旧源码暗编译 |

## 10.5 E 组：全仓迁移、格式化和工具行为（E01–E20）

| ID | 场景 | 验收 |
|---|---|---|
| E01 | `marqdo run` 不带语义版本参数 | 唯一新语法 |
| E02 | 旧 H2/Emph 返回用例 | 不按旧语义运行，迁移后的新例成功 |
| E03 | 老式列表强制条件负例 | 转为新语法下的合法普通列表测试 |
| E04 | 仓库所有 `.mq.md` 分类审计 | 活跃正例可编译/负例按新规则拒绝 |
| E05 | 全仓没有语义 mapper 分派 | 代码检查 + 单元测试确保只有单入口 |
| E06 | `inspect --ast` | 原生 Markdown AST 未被改造 |
| E07 | `inspect --semantics`/相应输出 | 显示 Explicit/Inherited、FunctionOwner |
| E08 | 载荷 SourceSpan | 精确到字节 |
| E09 | UTF-8 中文 | 位置准确 |
| E10 | CRLF/BOM | 结果可重现 |
| E11 | Setext H1/H2 | 真正 Markdown AST 规则一致 |
| E12 | `fmt` 往返 | 语义 IR 和文档结构稳定 |
| E13 | Strong↔Emph↔Code 元变换 | 规范化 IR 等价 |
| E14 | Ordered↔Bullet↔Quote 容器等价 | 保持相同 payload 顺序时 IR 等价 |
| E15 | README Quickstart | 原样拷贝可执行 |
| E16 | `examples/**/*.mq.md` | 全数有 `check` / `run` / `call` 或明确负例 |
| E17 | `stdlib/**/*.mq.md` | 完全使用新语法、离线 frozen CLI 通过 |
| E18 | `stdlib/stdlib.json` | digest 与实际新源码树一致 |
| E19 | `docs/` 中全部 active 代码围栏 | 标为可运行的例子能按唯一语法执行；历史例子显式注明 |
| E20 | CI 工作流/发布门禁 | 无旧语言通过条件，无跳过迁移语义集的永久例外 |

## 10.6 目标 fixture 分类（按语义功能，不按语言版本）

```text
tests/
├── conformance/
│   ├── ast/                     # Markdown Profile 结构基线；无需强改纯 AST 样例
│   └── semantics/               # 完全迁移新 AST→IR 语义与 golden
├── negative/                    # 新语法错误+原有 Runtime 错误
├── semantics/
│   ├── heading_modes/           # H1–H6 继承、错误前缀
│   ├── payload_carriers/        # Strong/Emph/Code 等价
│   ├── control_flow/            # if/else/while/end/return
│   ├── functions_and_imports/   # params/H2 callable/模块引用
│   └── transformations/        # 元变换和结构等价
└── security/                    # 既有权限拒绝+更新的正例
```

不要创建 `tests/v1/` 与 `tests/v2/` 两套永久语义族。测试元数据可记录 `language_revision: "current"` 供诊断和追溯，但不得充当编译器模式选择器。

每条 golden 必须保留：原始 `.mq.md`、预期 IR/结果或诊断码、精确 SourceSpan（如该测试关心）、依赖/权限/执行环境，以及“是否在新语法重构中由旧 fixture 改写”的台账引用。

---

# 第 11 章：开发规范、代码审查规则和禁止事项

## 11.1 语言规范先行，而不是直接“修改到能跑”

每次改变语法，提交必须包含以下内容并按顺序进行：

1. **原位更新**适用的规范章节（优先 `spec/mapping.md` 和 `spec/expression.ebnf`），说明哪些源码合法、哪些非法以及原因；不创建并行活跃规范。
2. 增加**真实 Markdown AST golden**，确认 CommonMark/GFM 实际解析出了什么，尤其是星号嵌套和 CodeSpan。
3. 增加正常使用的**源码 → 归一化 IR → 结果** golden，以及相同语义的至少一个 Markdown 载体替换测试。
4. 增加至少两个**负面样例**：确保非法控制流和错拼指令不会被吞掉。
5. 再实现 Frontend / Expression 变化；如必须更改 Runtime，应提交单独 ADR 证明现有 IR 无法表达该语义。
6. 更新 README 的语法速查与迁移表；用户文档不能落后于可执行程序。

**合并时检查的关键问题**：新特性是真的语言语法，还是只是一个标准库函数？若只是功能，放进同仓库 `stdlib/`，不要增加关键字或 HostOp。

## 11.2 确定性原则（不得违反）

**明确禁止的写法或实现策略**：

- `try parse executable; if fails => ordinary prose`。
- 因为一个列表的第一项出现 `if`，就推断整个列表是可执行分支。
- 通过自然语言内容（如“若”“否则”“执行”“返回”）猜测控制语义。
- 因 `Strong` 是加粗，就赋值；因 `Emph` 是斜体，就返回（这是被废止的旧行为）。
- 因 `BlockQuote` 是引用块就自动调用；因 `OrderedList` 是列表就自动分支。
- 源码原始字符串中寻找 `@exec`、`return`、`if` 任意子串决定 AST 结构。
- 遇到未知 `@knowledg` 就退回上一祖先模式。
- 在 Knowledge 中反复扫描载荷、为“看起来是代码”的部分制造假执行事件。
- 根据 `--allow-read`、`--allow-net` 权限开关改变 AST→IR 编译结果。
- 在 Parser 中引入与 V2 正式文法没有对应测试的自动修复、冒号容错或语法猜测。

**严格隔离测试**：同一份 Knowledge 文档在不同权限开关下，应产生相同的零执行投影；同一份 Exec 文档在静态编译阶段也不应因 Host 权限不同而改变 AST 或 IR，权限只在运行时检查。

## 11.3 Markdown 是语言界面的一部分，需要像 ABI 一样审查

- **保留 Markdown 可渲染性**：不能因为方便而用私有的 Markdown 解析分支吞掉普通段落、链接或表格。
- **保留源码位置**：不能只输出拼接后的 `"return x"` 而丢失它原来位于 `*...*` 还是 `` `...` ``、哪个标题或哪个列表项。
- **保留结构关系**：即使所有执行载荷线性化，原始 AST/Unit 仍可供 inspect、未来知识索引和分析工具读取。
- **保留语法正交性**：Strong 与 CodeSpan 的执行行为应尽可能等价；列表类型、引用容器类型不能意外影响指令种类。
- **保留函数可读性**：示例应避免过度嵌套 Markdown 标记；在一个函数里不得为了满足测试而把复杂程序压成一行巨大的载荷。
- **不牺牲文档自然性**：默认 Knowledge，说明段落/表格/引用与正规 Markdown 一样工作；Exec 模式下的强调冲突必须在文档中诚实提醒。

## 11.4 Rust 工程边界和模块耦合约束

| 责任 | 唯一所属模块 | 禁止的替代方案 |
|---|---|---|
| Comrak/GFM AST | `marqdo-markdown` | 第二 Markdown parser 或 regex 语法解释 |
| 标题声明、模式继承 | `marqdo-frontend/units.rs`（或同 crate 小模块） | Host/CLI 猜标题语义 |
| Strong/Emph/CodeSpan 提取 | `marqdo-frontend/payload.rs` | 运行时自行识别 Markdown 强调 |
| 唯一 AST→IR 编排与控制栈 | `marqdo-frontend/map.rs` | `map_v1` + `map_v2` 同时存在 |
| 语句词法与关键字 | `marqdo-expression` | CLI 用正则解析 if/return |
| 函数参数 `params` | `marqdo-frontend/signature.rs` | 旧式孤立 CodeSpan `(x: Int)` 特例 |
| 数值运算、内存和资源执行 | 现有 Runtime/Host | `HostOp::If` 或 `HostOp::Return` |
| 高级文本和 I/O 组合 | 同仓库 `stdlib/` | 在 Rust 中反复增加同类算法 Builtin |
| 仓库迁移审计 | `docs/migration/` 台账 + 现有 CI | 主程序新语法、标准库旧语法 |

依赖方向保持 `markdown → frontend → ir → runtime`。不要因为破坏性语法升级创建另一个 Runtime、解释器或兼容 crate。

**必审项目**：每个影响 `.mq.md` 表面的 PR 必须列出受影响 README/Spec/Example/Stdlib/Test；没有同步迁移证据的 PR 不得视为完成。

## 11.5 性能、可维护性与错误恢复

- 解析源码只执行一次 Comrak；模式与语义索引尽量单次顺序扫描；载荷 AST 用引用视图而非复制全树。
- `N` 个 Markdown 节点的模式计算和提取应接近 `O(N)`，不应因每层标题重复扫描父标题或递归遍历整个文档形成 `O(N^2)`。
- 对恶意嵌套 Markdown 必须限制 AST 深度、表达式深度和控制栈深度，限制来自现有资源预算与解析器安全策略，不应无限递归崩溃。
- 解析器可在 IDE 检查时恢复多个错误以展示给用户，但生成执行器之前**只要存在硬错误就不得运行**；错误恢复只是诊断能力，不是容错执行。
- 诊断必须含机器可用的 `code` 和原始 `Span`；必要时提供 `related_span` 指向未关闭的 if 或对应的重复模式声明。
- 原有缓存、分析摘要和构建产物必须在此次破坏性升级时失效/重建；不得让旧语义生成的缓存 IR 在新运行时直接复用。

## 11.6 Review Checklist（每个语法 PR 都必须回答）

- [ ] 该 PR 是否改变了已定义语义？如改变，是否有批准的 ADR 和规范修订？
- [ ] 所有用户示例是否能被当前提案文法精确解释，而不是依赖人类脑补？
- [ ] 是否新增了不必要的关键字、Rust HostOp 或重复 Runtime 分支？
- [ ] 是否存在某种 Markdown 排版转换会改变程序行为？若有，是否是本来就该改变的载荷文本？
- [ ] 有没有将 Exec 语法错误静默改成普通文档？
- [ ] 普通 `1.` / `-` / `>` 是否不再强制成为分支、循环或调用？
- [ ] 是否对照真实 Comrak AST 验证星号、CodeSpan、换行与混合文本？
- [ ] Heading 继承是否正确处理兄弟、子标题、跳级和源文件结尾？
- [ ] 函数的名称、参数、作用域与 Call 语义是否保真？
- [ ] 是否已经删除所有 V1 编译分支，旧语法不能再作为合法活跃程序执行？
- [ ] 权限、Sandbox、Resource、Trace 是否保持原有安全要求？
- [ ] 迁移台账是否覆盖 README、全部 active docs、stdlib、examples、所有 fixture、Rust 内嵌样例和 CI？

---

# 第 12 章：验收标准、CI 门禁与发布条件

## 12.1 必须完成的架构与全仓验收

| 门禁 | 必须证明 | 阻断场景 |
|---|---|---|
| G01 | **只有新语法**：无 per-file 版本、无旧 mapper、无 legacy flag | 新程序仍可通过 V1 fallback |
| G02 | 所有活跃 `.mq.md` 经单一新 mapper 编译/校验 | 某 stdlib 模块仍用斜体返回 |
| G03 | H1–H6 模式继承确定 | 同层兄弟模式泄漏 |
| G04 | Knowledge 绝无执行副作用 | Knowledge Table 的 `print` 被调用 |
| G05 | Strong/Emph/CodeSpan 单句等价 | 换强调方式后 Assign 变 Return |
| G06 | 闭合节点仅提取一次 | 嵌套 CodeSpan 触发两次执行 |
| G07 | if/else/while/end 严格配对 | `else` 越过栈顶 while 绑定外层 if |
| G08 | 列表/引用块无指令特权 | 把 `1.` 换成 `-` 导致 Branch→Loop |
| G09 | 错误有确定码和精确 SourceSpan | `if x >` 被当普通说明吞掉 |
| G10 | H2 Exec 才可调用，H3 归属正确 | H3 被导出独立函数 |
| G11 | 不允许跨语义片段非法配对 | `end` 越过标题边界关闭旧 if |
| G12 | `params` 无特殊 Markdown 容器特权 | 只有独立 CodeSpan 可声明参数 |
| G13 | 复用现有 IR/Runtime/Memory/Resource | 新增一套 V2 VM |
| G14 | 安全权限保持原约束 | `@exec` 绕过文件网络 capability |
| G15 | `stdlib/` **完整迁移 + digest 更新** | 旧式标准库在新 CLI 下无法载入 |
| G16 | `examples/` 与 README 样例原样可运行 | 文档需要读者自行补模式声明 |
| G17 | `spec/` 只有一套现行语义规范 | `mapping-v1.md` 与 `mapping-v2.md` 双活跃 |
| G18 | 全仓测试 golden 与 Rust 嵌入 fixture 重构 | 旧负例仍要求普通列表报错 |
| G19 | 历史 ADR 明确历史，不误导开发者 | README 将旧 RFC 视为现行语法 |
| G20 | 文件级迁移台账 100% 闭合 | “未审阅”文件不计入失败 |
| G21 | README 初学者无需聊天背景能使用 | 找不到可复制的 main/if/while 示例 |
| G22 | 平台与安全 CI 如实标记实际验证状态 | 把本地 PASS 说成 Windows/macOS CI 全绿 |

**阻断性合并结论**：任一 G01–G22 未通过，不得合并为声称已经完成的破坏性语法重构，也不得发布“新语法稳定版”。

## 12.2 推荐 CI 检查与全仓语法门禁

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked

# 下一步由开发者实现并固定的门禁（本 RFC 不声称现有脚本已存在）
# python3 tools/check-language-migration.py --inventory docs/migration/semantic-rewrite-inventory.tsv
# python3 tools/check-document-examples.py --readme README.md --active-docs docs/
# python3 tools/check-stdlib-syntax.py --stdlib stdlib/
# python3 tools/check-single-mapper.py --crates crates/
```

新增检查脚本必须逐个列出失败文件和对应语义理由。脚本不应只用搜索文本来判断语法：需要调用真实 Frontend 对活跃 `.mq.md` 语料分类/解析；对于 Markdown 教程中标注的“可运行示例”，应提取到临时目录执行 `marqdo check/run/call` 并验证预期结果。

现有安全矩阵、Package Lock、Tool/Analysis/Trace、ABI/Foreign/Worker 回归必须并行验证；任何未能运行的目标平台明确标 `UNVERIFIED`，不能伪造 PASS。

## 12.3 性质测试优先于“碰巧跑通几个例子”

建议 10 类变形测试：

1. 同一载荷的 Strong ↔ Emph ↔ CodeSpan 替换（若 Markdown 解析出的 AST 结构满足合法要求）应使归一化 IR 等价。
2. OrderedList ↔ BulletList 替换，在**提取载荷流相同**时不改变 Branch/Loop 类别。
3. 在普通引用块外加一层 BlockQuote 包装，不应改变合法载荷操作类别；来源 Span 会变化。
4. 载荷节点前后插入普通 Knowledge 段落，不改变已有语句的控制结果。
5. 在 Exec H2 下插入 Knowledge 子节后，原执行片段已完整结束时，执行结果不改变。
6. `@exec` → `@knowledge` 模式切换必须将所覆盖的所有载荷从执行投影中移除，不能保留副作用。
7. 交换同级两个 Knowledge H3 不改变同一 H2 Exec 的**可执行** IR（在它们均无 exec 语句且不触及名称冲突时）。
8. 代码跨不同容器的视觉调整保持相同 payload 顺序时，执行结果不变。
9. 对不带任何语义版本字段的合法新语法文件，必须直接按唯一 mapper 编译；额外加入旧式版本 Front matter 字段必须产生稳定诊断。
10. 将合法 `if...end` 刻意拆成不同标题片段，必须产生明确的 unclosed/unmatched 诊断，不可默默修复。

**归一化 IR 规则**：剔除源代码 span、carrier 样式、Markdown 容器元数据，而保留运算语义、控制依赖、调用顺序、布尔分支结果与返回路径。对于副作用程序还要比较 Trace 序列或 mock Host 行为；仅比较返回值不够。

## 12.4 进入 Release Candidate 的完整定义

- README Quickstart 与 `examples/` 全部原样运行；用户不必设置版本字段或 --legacy。
- `spec/mapping.md`、`spec/expression.ebnf`、`spec/evaluation.md` 和相关规范只有一套当前有效的语法与行为定义。
- `stdlib/**/*.mq.md` 全部迁移，`stdlib/stdlib.json` digest 及冻结二进制离线闭包通过；第三方依赖旧语法明确拒绝。
- `tests/conformance/ast` 保持 Markdown 原生正确性；语义/负例/CLI 集成/安全/跨模块 fixture 全部按新语法更新，golden 更新经过语义审查。
- 已删除旧 Strong/Emph/List/Quote 自动指令映射和按语义版本路由；相关遗留代码不再编译、不得隐藏在 `cfg(feature="legacy")` 中。
- `docs/migration/semantic-rewrite-inventory.tsv` 全部跟踪文件分类完成，所有 active 文件的迁移工作结束，并有可复现测试证据。
- `docs/adr/` 历史决策标明 superseded，现行文档入口不会指向旧语法作为教程。
- Package/Capability/Memory/Resource/Tool/Analysis/Trace 正常回归；不同平台按真实测试结果汇报。
- Changelog/Release Notes 公开说明旧源代码**不兼容**、标准库包身份变化及如何改写。

此处的“没有兼容性回归”只指**已承诺保持不变的计算结果、标准库公开函数功能、资源权限和安全边界**，不包括旧 `.mq.md` 源码仍可直接执行——旧语法不兼容是本次明确接受的破坏性变化。

---

# 第 13 章：可复制的完整新语法例子与预期结果

本章所有示例均用于**重构完成后的唯一语法 CLI**；基线仓库尚无法原样执行 `@exec`、`return`、`end` 等形式。除了特别标记外，示例只使用已存在的算术、函数及 `print` 基础能力。

## 13.1 示例 A：纯文档与执行函数共存

文件：`examples/language/01-hello.mq.md`

````markdown
# @knowledge 学习笔记

普通 Markdown 可以使用**加粗**、*斜体*和 `return` 这样的文本。

1. 第一条说明
2. 第二条说明

## @exec main

`print("Hello from Marqdo v2")`
`return 1`

### @knowledge 使用方法

这一小节并不执行。

| 语言标记 | 作用 |
| --- | --- |
| @exec | 标记可执行区域 |
| @knowledge | 标记普通文档区域 |
````

预期：`main` 输出一行 `Hello from Marqdo v2`，返回 `1`；加粗、斜体、知识表格不参与计算。

## 13.2 示例 B：一份完整的条件分类程序

文件：`examples/language/02-classify.mq.md`

````markdown
# @exec 数值分类

## 分类

`params(n: Int)`

`if n < 0`
`return -1`
`else`
  `if n == 0`
  `return 0`
  `else`
  `return 1`
  `end`
`end`
````

**可实际执行的 AST 安全写法**：上例为了突出逻辑层级，在 fenced `markdown` 示例中使用了两空格缩进；实现时应将所有 Exec 载荷顶格独立成段，以避免 CommonMark 将某些连续缩进视作不同容器。官方仓库 golden **必须使用下面的完整形式**，且取代上面的人类阅读示意：

````markdown
# @exec 数值分类

## 分类

`params(n: Int)`

`if n < 0`

`return -1`

`else`

`if n == 0`

`return 0`

`else`

`return 1`

`end`

`end`
````

预期：`分类(n=-2) => -1`，`分类(n=0) => 0`，`分类(n=7) => 1`。所有 `return` 直接退出当前函数，不需要最后再写一个额外的返回表达式。

## 13.3 示例 C：多级标题覆盖、局部变量共享

文件：`examples/language/03-mode-inheritance.mq.md`

````markdown
# @knowledge 计算说明

## @exec main

`x = 5`

### @knowledge 推导

这是**重点说明**，不会被执行。

| 表达式 | 值 |
| --- | --- |
| x + 1 | 6 |

### @exec 实现

`x = x + 1`
`return x`
````

预期：返回 `6`，两个 Exec 片段共享当前 H2 函数的 `x`，Knowledge 片段零指令；不能把 H3 `实现` 导出为独立函数。

## 13.4 示例 D：写内存与语法载体等价

文件：`examples/language/04-memory.mq.md`

````markdown
## @exec main

**m = memory.new(1, 1)**

*`m[0] = 42`*

`return m[0]`
````

预期：返回 `42`。`*` 在这里承载写入语句，不是 Return。这个例子必须与等价的 `**`m[0] = 42`**` 和独立 CodeSpan `m[0] = 42` 归一化 IR 一致。

## 13.5 示例 E：普通引用、列表和受控计算并存

文件：`examples/language/05-prose-and-control.mq.md`

````markdown
## @exec main

下面两项是算法步骤说明，不是分支，也不是循环：

1. 初始化计数
2. 运行迭代

`n = 3`
`counter = 0`

> 注意：这是普通 Markdown 引用，不是调用。

- `while counter < n`

  **counter = counter + 1**

**end**
`return counter`
````

预期：返回 `3`；两个普通有序列表项和引用说明不生成语句；无序列表只提供展示结构，真正的循环由 `while/end` 决定。

## 13.6 示例 F：尚不存在的系统输入接口，明确不能当成运行验收

````markdown
---
import:
  io: "std:io"
---

## @exec main

`name = io.读取一行("你的名字：")`
`io.写值(name)`
`return name`
````

这只是 **将来标准库补齐 stdin 之后的接口构想**。它不能作为 V2 语义重构成功的必要测试，也不能被标记为现有功能已经实现。与新语法有关的验收在本章节其他示例中完成；stdin/file API 属于单独的计算内核和标准库迭代计划。

---
