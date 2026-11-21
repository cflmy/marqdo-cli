# Marqdo 3.x 第三方开发者接入手册

> 状态：**随 `3.0.0-beta.1` 发布**（Linux x86_64 beta）。产品版本为 `3.0.0-beta.1`；协议版本独立：Analysis v2、Trace v2、Tool v1、Plugin ABI v1、Foreign wire v1、Provider manifest v2、Lock v4、Markdown profile `marqdo.md.v1`。官方 stdlib 包版本 `0.1.0`、插件/工具/示例包版本也都是独立版本轴，不随产品版本变化。
>
> 本文是入门与工程接洽流程；与已发布二进制冲突时，以该版本 `--help`、`--version`、`spec/` 和 release notes 为准。本文所有命令片段都已在 `3.0.0-beta.1` 二进制上验证；未验证的可能平台行为会明确标注为 UNVERIFIED/UNSUPPORTED，绝不默认为支持。

配套入口：[`developers/README.md`](developers/README.md)（按任务分拆的
quickstart/libraries/tools/extensions/analysis-trace/compatibility）和
[`developers/release-limits.md`](developers/release-limits.md)（beta 限制）。

## 0. 快速定位：你要开发的是什么？

| 目标 | 必须使用的扩展入口 | 可否完全独立于核心仓库 | 第三方主要交付物 |
|---|---|---|---|
| 普通 AI、数学、量子算法、SKILL、数据处理 | Marqdo `.mq.md` 包 | 是 | `index.mq.md`、`marqdo.yaml`、README、fixtures |
| 知识分析、Proof、View、IDE、Lint 等外部工具 | CLI Tool Protocol v1 | 是 | 独立可执行程序、`tool.json`、Analysis/Trace 消费者 |
| 高性能原生算法/量子门适配函数 | Callable Plugin ABI v1 | 是（遵循 ABI） | `plugin.yaml`、目标平台动态库、C header/SDK |
| 新的 Python/R/Julia 等外语执行后端 | Foreign Kernel | 是（遵循 wire） | `kernel.json`、可执行 provider、固定环境 |
| 硬件资源、数据库会话、外部流设备 | Resource Provider v2 | 是（遵循别名协议） | `provider.json`、C ABI library（`marqdo-provider-worker` 由 CLI 发行包提供，不随扩展分发） |
| 改变 `@exec` 语义、增添 `if` 关键字/原生数值类型 | 正式语言 RFC | **不是**第三方扩展 | core 变更 + 迁移 + 完整语言一致性测试 |

**最重要的架构规则：** 第三方仓库只依赖已发布的 CLI/规范协议；禁止依赖
`marqdo-ir` 私有 Rust 结构、内部 `HostOp`、CLI 当前源码相对路径、未冻结的
`inspect --ir` 私有结构或运行时进程内存布局。新领域函数不得靠在核心
`builtins_impl.rs` 加 `quantum.*`/`agent.*` 分支获得支持。

## 1. 前置条件、安装布局与版本协商

### 1.1 发布归档

`3.0.0-beta.1` 只发布 **Linux x86_64 (glibc)** 归档：

```text
marqdo-3.0.0-beta.1-x86_64-unknown-linux-gnu.tar.gz
marqdo-3.0.0-beta.1-x86_64-unknown-linux-gnu.tar.gz.sha256
```

```bash
sha256sum -c marqdo-3.0.0-beta.1-x86_64-unknown-linux-gnu.tar.gz.sha256
tar -xzf marqdo-3.0.0-beta.1-x86_64-unknown-linux-gnu.tar.gz
cd marqdo-3.0.0-beta.1-x86_64-unknown-linux-gnu
./marqdo --version        # marqdo 3.0.0-beta.1
```

归档内至少包含：`marqdo`、**同目录的** `marqdo-provider-worker`、
`RELEASE_INFO.json`（source SHA/target/toolchain/binary SHA256）、`SOURCE_SHA`、
`KNOWN_LIMITATIONS.md`、`spec/`、`docs/developers/`、`examples/`、
`tests/consumers/`。`sha256` 只证明内容完整性，**不是**发布者身份签名。

`marqdo-provider-worker` 只按 `marqdo` 可执行文件同目录解析，不查 `PATH`、
不退化为 Host 进程内加载；缺失时 Provider 请求以
`ext.worker_unavailable` fail-closed。Provider 扩展作者只交付
`provider.json` + C ABI library。

### 1.2 源码/工具链前置条件

- 消费扩展：不需要 Rust。需要 `git`（Git 依赖）、`python3`（归档中的
  Analysis/Trace consumer）。
- 自己编译插件/Provider：Rust 1.82+ 或仅 C 编译器（`cc`）+ 冻结 C header。
- 内置 Foreign kernel：系统 `python3`/`node`（或显式固定环境）。
- `--mode strict`：仅 Linux，需要 Bubblewrap 且内核/环境允许 namespace；
  否则 fail-closed。

### 1.3 版本轴与 SemVer 预发布规则

产品版本、协议版本、包版本是三个独立轴：

| 轴 | 当前值 | 说明 |
|---|---|---|
| 产品/CLI | `3.0.0-beta.1` | `marqdo --version`、Analysis `producer.version` |
| 协议 | Analysis v2、Trace v2、Tool v1、Plugin ABI v1、Foreign wire v1、Provider manifest v2、Lock v4 | 不因产品叫 3.0 而改成 v3 |
| 包 | 官方 stdlib `0.1.0`；示例插件/工具/项目 `0.1.0` | 各自 SemVer 轴 |

Beta 期必须显式包含预发布号。`">=3.0.0,<4.0.0"` **不会**匹配
`3.0.0-beta.1`（SemVer 规则）；请写
`">=3.0.0-beta.1,<4.0.0"`。`tool.json` 的 `compatibility.marqdo` 与
`marqdo.yaml` 的 `project.marqdo` 都按此规则**强制执行**，不匹配分别报
`requires marqdo …` / `package.marqdo_incompatible`，不会静默忽略。

### 1.4 安全模式

- `--mode pure`：纯语言执行，不授予任何外部系统权限。
- `--mode project`（默认）：只能通过显式 CLI grant/授权访问资源；**不是**安全
  OS sandbox，默认按当前用户身份运行。
- `--mode strict`：Linux-only、Bubblewrap probe-gated 的 L1 隔离（pure 语言 +
  native Math）。它拒绝 `--allow-*`、`--plugin`、`--provider`、`--kernel`、
  `--trust-native-plugin` 和 `--trace-file`；strict L2（Worker/Foreign/Provider）
  未实现。没有该发行 SHA 的隔离证据时，发布说明必须明确 `strict: unsupported`
  并保持 fail-closed。

## 2. 纯 `.mq.md` 库：优先采用

推荐独立仓库结构：

```text
marqdo-quantum/
  README.md
  marqdo.yaml
  index.mq.md
  tests/
  examples/
```

`marqdo.yaml`（`project.marqdo` 在 beta 起强制校验）：

```yaml
project:
  name: quantum
  version: 0.1.0
  marqdo: ">=3.0.0-beta.1,<4.0.0"
dependencies: {}
```

`index.mq.md`（符合 RFC-LANG-0002）：

```markdown
# @knowledge 量子数学工具

此包在不修改 Marqdo 内核的情况下向外公开领域函数。

## @exec 加倍

`params(x: Int)`

`return x * 2`
```

消费者固定真实 Git commit（**小写 40 或 64 位完整 SHA**；分支/标签不可复现）：

```yaml
# consumer/marqdo.yaml
project:
  name: quantum-consumer
  version: 0.1.0
  marqdo: ">=3.0.0-beta.1,<4.0.0"
dependencies:
  quantum:
    git: https://github.com/your-org/marqdo-quantum.git
    rev: 0123456789abcdef0123456789abcdef01234567
```

```markdown
---
import:
  quantum: "@quantum"
---

## @exec main

`return quantum.加倍(x=21)`
```

`@quantum` 绑定依赖包的入口 `index.mq.md`；调用需带别名前缀。路径依赖使用
`path: ./deps/x`（必须位于项目根内）；`marqdo add <alias> --git <url> --rev
<sha>` 可代写。

运行步骤（消费者项目根）：

```bash
marqdo lock
marqdo sync --locked
marqdo check index.mq.md --refs
marqdo call index.mq.md main      # -> 42
```

### 2.1 Lockfile 行为（必须理解）

- 声明依赖后，`check`/`run`/`call`/`inspect` **要求** `marqdo.lock`；缺失报
  `project.lock` 并要求先运行 `marqdo lock`。
- `run`/`check` 从不 fetch，也从不改写 lock。普通 `sync` 可能按 lock 取回
  缺失的 pinned Git revision；`sync --locked` 禁止 fetch，只用已验证缓存。
  两者都校验内容摘要，绝不静默更新 lock。
- lock v4 记录完整包实例闭包（`dep:name@version#content-digest`）、alias 边和
  扩展 manifest/artifact 身份；v3 会得到显式 `marqdo lock` 迁移错误。
- 提交 `marqdo.lock`，忽略 `.marqdo/`（`init` 已写 `.gitignore`）。

## 3. Knowledge / Prove / View：使用 Tool Protocol v1

这是外部工具的首选架构：核心只提供语言/公开协议，工具是独立进程。

```text
marqdo-cli       (语言核心，不依赖工具仓库)
  |  check --emit / run --trace-file / call / public spec
  v
marqdo-knowledge / marqdo-prove / marqdo-view   (各自独立仓库)
```

建议仓库：

```text
marqdo-view/
  README.md
  tool.json
  bin/marqdo-view                 # 独立编译后生成，不能是核心源码依赖
  src/
  tests/contract/
  examples/
```

`tool.json`（已可在 beta 上安装）：

```json
{
  "schema": "marqdo.cli-tool",
  "schema_version": 1,
  "name": "marqdo-view",
  "version": "0.1.0",
  "command": "view",
  "entrypoint": "bin/marqdo-view",
  "compatibility": {
    "marqdo": ">=3.0.0-beta.1,<4.0.0",
    "analysis": [2],
    "trace": [2]
  }
}
```

安装和调用：

```bash
marqdo tool install ./marqdo-view        # 目录或 tool.json
marqdo tool list
marqdo view --help
marqdo view index.mq.md
marqdo tool remove view
```

**Tool 运行契约：**

- CLI 原生命令优先；未知顶层命令只有已注册 `tool.json` 才分发，绝不扫描
  `PATH` 猜测执行。
- 安装：读取并校验 schema/SemVer/协议交集、entrypoint 不越界、命令不冲突；收集
  整包清单（相对路径/大小/SHA-256，拒绝符号链接/特殊文件，限制 4096 文件/
  256 MiB），复制到 staging 后重新计算并逐项比对，最后原子发布。安装目录为
  `$MARQDO_TOOL_HOME`（若设置）或用户配置目录下的 `marqdo/tools`。
- 分发：启动前重新计算整包清单并与记录比对，entrypoint 必须属于该清单；保留
  `cwd`、stdio、argv 和退出码，不经 shell、不拼接命令行。
- 工具环境变量：`MARQDO_BIN`（当前 CLI 绝对路径）、
  `MARQDO_TOOL_PROTOCOL=1`，以及协商成功后提供的
  `MARQDO_PROTOCOL_ANALYSIS=2` / `MARQDO_PROTOCOL_TRACE=2`。
- 工具不应猜测 `marqdo` 在 `PATH` 中的位置，优先调用 `MARQDO_BIN`，通过 argv
  数组传参。
- Tool 是用户主动执行的普通进程，**不继承** `.mq.md` 程序的
  `pure/project/strict` 隔离；不得静默执行文档或继承授权。
- 负例：未知 schema major、`marqdo` 范围不匹配、保留命令、逃逸/符号链接
  entrypoint、篡改 bundle、缺失 entrypoint 都必须 fail-closed；协议不匹配报
  `tool.protocol_incompatible`。

### 3.1 公共 Analysis v2：静态输入

```bash
marqdo check index.mq.md --emit ./analysis.json
```

只在退出码为 0 后读取 artifact。`--refs` 是**独立**的 Markdown 链接校验，可能让
`check` 失败；失败时 `check` 会删除旧的 emit 目标文件，因此不要用
`check --refs --emit` 生成消费 artifact 而不检查退出码。

成功时为 JSON `schema: marqdo.analysis`、`schema_version: 2`。顶层字段：
`producer`、`project`（`root_id`、`identity_kind`、`lock_digest`——无 lock 时为
空字符串、`stdlib`——可能为 null）、`modules`、`diagnostics`。模块字段：
`id`、`package_instance_id`、`relative_path`、`source_digest`、`units`、
`functions`、`imports`、`references`、`calls`、`resources`。字节偏移以 UTF-8
源文本为准；稳定索引键用包实例身份而非本地绝对路径。完整字段与 JSON Schema：
`spec/analysis-v2.md`、`spec/schemas/analysis-v2.schema.json`。`check` 不加载
插件库、不启动内核/Provider、不运行程序。

**View 责任**：读取原始 Markdown 渲染（不能用 Analysis JSON 代替原文）；把源
字节偏移/标题单元/可调用映射关联到渲染内容；运行展示只读 Trace，不能自行执行
`.mq.md` 或产生不受 Host 控制的副作用。

**Knowledge 责任**：消费 Analysis 的标题锚点、链接、资源、包身份与来源摘要；
生成索引/知识图；不假定 `@knowledge` 有运行效果；跨包地址使用显式 import/锁定
身份，禁止按物理路径猜目标。

**Prove 责任**：从公开 Analysis 提取可证明子集/证明义务，通过外部证明器生成
证书；要求源摘要、语义与契约身份一致；明确区分
`proved / disproved / unknown / unsupported`。无法证明不等于程序错误。不得修改
core IR 或把“模型说证明了”当可信证书；缺公共字段应先提版本化协议提案。

### 3.2 公共 Trace v2：动态输入

```bash
marqdo run index.mq.md --trace-file ./trace.json
```

读取 `schema: marqdo.trace`、`schema_version: 2`。把 `source.analysis_digest`
与实际 Analysis JSON **文件字节**的 SHA256 对照；不一致提示 stale trace。事件
包含 `call.enter`、`call.exit`、`branch.eval`、`loop.iteration`、`return`、
`effect`，以及可能的 `trace.truncated`；每个带来源事件包含 module id + UTF-8
span。Trace 不记录参数值、秘密或文件内容。默认预算 100000 事件 / 32 MiB，可用
`--trace-max-events` / `--trace-max-bytes` 调整；工具不得假定 trace 永远完整。
`run --trace-file` 不生成 Analysis，digest 只适合与同一项目/同一投影配置的
Analysis 对比。详见 `spec/trace-v2.md`、`spec/schemas/trace-v2.schema.json`。

**不要依赖** `inspect --ir` 的当前 JSON 形态（不是冻结跨仓协议）；需要结构信息
用 `check --emit`。

## 4. 计算插件：Callable Plugin ABI v1

平台矩阵：**Linux x86_64 的 native 与 worker 是 beta 支持目标**；macOS
native/worker 为 UNVERIFIED，Windows native/worker 为 UNSUPPORTED（Unix-gated）。
首发只应交付 `x86_64-unknown-linux-gnu` 的 `.so`。

公开 ABI 头文件 `include/marqdo_plugin_abi_v1.h`，Rust SDK
`crates/marqdo-plugin-sdk`（`mq_plugin_entry!`）。`plugin.yaml`：

```yaml
plugin:
  name: demo.solver
  version: 0.1.0
  abi: "^1.0"
  kind: worker              # native 需 --trust-native-plugin；worker 为崩溃隔离
  exports:
    - name: solver.add
      signature: "($a: Int, $b: Int) -> Int"
  requires: []
  artifacts:
    - target: x86_64-unknown-linux-gnu   # 必须与宿主 triple 精确相等
      file: libsolver.so
      sha256: "<实际工件的64位小写十六进制SHA256>"
```

消费（**只接受命名参数**；`--allow-plugin` 匹配 `plugin.name` 全名）：

```markdown
## @exec main

`return solver.add(a=20, b=22)`
```

```bash
marqdo plugin inspect path/to/plugin.yaml      # 仅静态，不加载
marqdo check index.mq.md --plugin path/to/plugin.yaml
marqdo run index.mq.md --plugin path/to/plugin.yaml --allow-plugin demo.solver
```

`check` 按 Manifest 做静态签名绑定，不加载插件（工件可以尚不存在）。`run` 先
验证工件 hash/目标平台/grant，再核对静态与动态 export 清单一一一致，不能在
运行时猜名字或合成空签名。`kind: native` 进程内执行，必须显式
`--trust-native-plugin`，可直接破坏宿主；`kind: worker` 在子进程中加载同一
library，是崩溃/地址空间隔离，**不是** OS sandbox。ABI v1 配置默认值被拒绝；
参数字面量位置参数会被规范化但库调用约定仍是命名。规范源：
`spec/plugin-abi.md`、`spec/callable-resolution.md`；可运行示例
`examples/plugin-demo/`（归档内含 host 构建工件）。

## 5. Resource Provider v2：设备/外部流

平台：**Linux x86_64**（worker transport）；macOS/Windows 不声称。

```json
{
  "schema": "marqdo.provider",
  "schema_version": 2,
  "name": "demo.sensor",
  "version": "0.1.0",
  "abi": "c-abi-1",
  "transport": "worker",
  "kinds": ["demo.sensor"],
  "artifacts": [{
    "target": "x86_64-unknown-linux-gnu",
    "path": "lib/libdemo_sensor.so",
    "sha256": "<实际工件的64位小写十六进制SHA256>"
  }],
  "permissions_requested": []
}
```

```bash
marqdo run index.mq.md \
  --provider /path/to/provider.json \
  --allow-provider demo.sensor
```

程序中继续使用普通语言接口 `resource.open("demo.sensor", "unit-1", ["read"])`
配合 `resource.read/write/close` 和 `Memory` 搬运 bytes。Provider 不新增语言
关键字或 callable。一个资源 kind 只能匹配一个 Provider，重复与未授权在真正执行
前拒绝。`marqdo-provider-worker` 必须与 `marqdo` 同目录、同目标平台；缺失时报
`ext.worker_unavailable`，不退化到 PATH 或进程内加载。`check`/`inspect` 不接受
`--provider`，也从不加载 Provider。当前特性不是安全沙箱。规范源：
`spec/resource-provider-v2.md`、ADR-0055；可运行示例 `examples/provider-demo/`
（先运行其 `build.sh`；仓库内模板 manifest 的 digest 是零值，只有生成的
`build/provider.json` 可运行）。

## 6. Foreign Kernel：新增未知语言无需改内核

使用 `kernel.json` 注册新语言标签；`Code`/`Formula` 仍是惰性值，必须显式调用
`foreign.run/foreign.call`。最小描述符（字段以 `spec/foreign.md` 为准）：

```json
{
  "provider_id": "mocklang",
  "languages": ["mocklang", "mock"],
  "wire_version": 1,
  "entrypoint": "./mocklang",
  "permissions_requested": [],
  "digest": "<entrypoint/环境工件的64位小写SHA256，必填>",
  "environment_id": "mocklang-env-1"
}
```

```bash
marqdo run demo.mq.md --kernel path/to/kernel.json --allow-foreign
```

`wire_version` 当前必须为 1；相对 `entrypoint` 相对描述符目录解析且必须
canonicalize 到普通文件；`digest` 必填并在每次 spawn 前校验；`builtin: true`
被拒绝；`--kernel` 可重复。Host 必须拒绝未授权执行、digest 不匹配、异常退出、
超时与不兼容 wire。不要按语言名在 core 加 `if language == "julia"`。示例：
`examples/kernel-mock/kernel.json` 与 `demo.mq.md`；规范源：`spec/foreign.md`。

## 7. 可复现扩展选择：`marqdo.yaml.extensions` / lock v4

```yaml
project:
  name: my-research
  version: 0.1.0
  marqdo: ">=3.0.0-beta.1,<4.0.0"
dependencies:
  quantum:
    git: https://github.com/your-org/marqdo-quantum.git
    rev: 0123456789abcdef0123456789abcdef01234567
extensions:
  plugins:
    - from: "@quantum/adapter/plugin.yaml"
  kernels: []
  providers:
    - from: "@quantum/adapter/provider.json"
```

- `from` 仅支持 `./` 相对路径或 `@alias/` 已声明包依赖路径；不允许 URL、绝对
  路径、通配符、`..`、空路径或 symlink 逃逸。
- `marqdo lock` 将已选扩展的 manifest/目标 artifact 摘要及逻辑包实例写入 lock
  v4；`marqdo inspect index.mq.md --extensions --json` 只显示身份与 lock 状态，
  不执行。
- `check`/`sync` 检查声明与 lock 一致（`ext.extension_lock_mismatch`）；
  `run/call` 在授权前再次检查实际工件。
- **选择扩展永远不等于授予权限**。`--allow-plugin NAME`、
  `--allow-provider NAME`、`--allow-foreign` 仍必须显式。`strict`/`pure` 不允许
  第三方执行扩展。
- Manifest 任何签名/路径/版本/目标平台/SHA256 不符都 fail-closed，不得“自动找
  可用最新版本”。

## 8. 跨仓库契约测试（任何外部工具/库都要交付）

**正向**：独立 Git repo、真实固定 SHA 导入；在已发布二进制上 `check`、`run/call`；
Tool 执行 `marqdo tool install ...` 和真实命令；Analysis/Trace 消费者验证
schema、digest、源码 span；Provider 与 Plugin 在正确授权下返回确定结果。

**负向**：未知语法标记不会被执行；错误函数名/arity；导入别名与 builtin 命名
空间冲突；重复导出；缺失/错误摘要；未授权；缺 Provider Worker；平台不匹配；
混淆 lock v3/v4；过期 Trace；未知 schema major；Worker 崩溃/超时；上述都必须
产生预期诊断，没有“失败后静默改用其他实现”。

**冻结 CLI 测试**：先构建或下载固定来源 SHA 的 CLI 二进制并记录 SHA256，然后
仅修改外部仓库重复完整正负验收；严格验证二进制哈希不变：

```bash
python3 tools/ci/verify_extension_contracts.py \
  --cli /path/to/frozen/marqdo \
  --fixtures /path/to/marqdo-extension-conformance \
  --out docs/evidence/extension/<release-sha>/manifest.json
```

参考独立仓库
[`marqdo-extension-conformance`](https://git.cflmy.cn/cflmy/marqdo-extension-conformance)、
`docs/evidence/extension/schema.json`。若每新增一个函数都必须重编 CLI，该扩展
不合格。

## 9. 开源发布与支持政策

- 每个第三方包发布：README、最小示例、实际支持的 CLI 产品版本范围、独立协议
  版本、目标平台矩阵、权限清单、可复现工件 digest、测试命令、限制与安全声明。
- 明确 `sha256` 仅验证内容完整性，不等于发布者身份认证；不能仅凭未知 Git URL
  的 hash 就把扩展当可信代码。
- `developers/README.md` 指向按任务拆分的 Tool/Library/Plugin/Kernel/Provider
  教程和 `spec/` 规范协议。
- `3.0.0-beta.N` 阶段的第三方应把破坏性变更视为真实风险；LTS 承诺尚未宣布。
- 本 beta 的支持平台只有 Linux x86_64；G4 是固定节点上的 Beta-only WARN，
  strict 策略以发布证据为准；这些都不是 LTS 承诺。

## 10. 独立工具开发清单（Knowledge/Prove/View）

- [ ] 独立仓库构建，`marqdo-cli` 没有任何依赖指向本仓库。
- [ ] `tool.json` 合法、入口文件属于安装包、Tool 版本与 CLI 协商成功。
- [ ] 从 `MARQDO_BIN` 派生调用，而非内部 Rust API。
- [ ] `check --emit` 读 Analysis v2；`run --trace-file` 读 Trace v2；强校验
      schema/version/digest 与退出码。
- [ ] 只解析用户授权源文件及显式依赖闭包，路径兼容多包实例/不同 checkout。
- [ ] 确定性输出、错误诊断、fixture、中文/英文快速教程齐备。
- [ ] 未验证 strict、不可证明内容、无设备支持等边界明确提示，不默默声称支持。
- [ ] 冻结 CLI 黑盒测试及反向兼容性测试 PASS；无 core source edit。

## 11. 本文引用的仓库规范

- `spec/mapping.md`、`spec/expression.ebnf`、
  `docs/architecture/RFC-LANG-0002-unified-semantics.md`
- `spec/package-format.md`、`spec/extension-selection.md`、
  `spec/callable-resolution.md`、`spec/references.md`
- `spec/tool-v1.md`、`spec/analysis-v2.md`、`spec/trace-v2.md`、
  `spec/plugin-abi.md`、`spec/resource-provider-v2.md`、`spec/foreign.md`
- `spec/security.md`、`spec/markdown-profile.md`、`spec/value-wire.md`
- `spec/schemas/analysis-v2.schema.json`、`spec/schemas/trace-v2.schema.json`
- `include/marqdo_plugin_abi_v1.h`、`crates/marqdo-plugin-sdk`
- `docs/releasing.md`、`COMPATIBILITY.md`、`docs/evidence/extension/README.md`
- `docs/developers/README.md`、`docs/developers/release-limits.md`
