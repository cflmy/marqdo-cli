# Marqdo CLI 稳定版本开发与发布实施总规范

> **Historical roadmap note / superseded language syntax.** 本文件是稳定版本
> 规划快照（编写基线 `e966b766`，2026-11-20），不是执行状态记录。其语言前提
> 早于
> [`docs/architecture/RFC-LANG-0002-unified-semantics.md`](architecture/RFC-LANG-0002-unified-semantics.md)
> （[ADR-0052](adr/0052-unified-markdown-semantics.md)）：旧 `Strong`=执行、
> `Emph`=返回、列表=分支/循环、引用块=调用的映射已被废止；语言语义只以该 RFC
> 与 `spec/mapping.md` / `spec/expression.ebnf` 为准。发布计划/门槛内容仍可作为
> 下一阶段规划的参考输入，但必须按当前 HEAD、`docs/STATUS.md` 与迁移台账刷新后
> 才能当作现行门禁；不得据此声称新语法或任一平台已完成。

> **定位：** 可直接提交到 `cflmy/marqdo-cli` 的 `docs/STABLE_RELEASE_IMPLEMENTATION.md`，并完整交由 DeepSeek Harness 执行。  
> **状态：** Proposed / 下一阶段唯一总体实施计划（必须以实际代码、规范和测试验证结果修正状态）。  
> **编写基线：** `cflmy/marqdo-cli`，默认分支 `master`，远程 HEAD `e966b7661f03ee7462df959917f5630353e66b10`（2026-11-20）。  
> **最新远程 CI：** https://github.com/cflmy/marqdo-cli/actions/runs/44680378977 — Linux workspace 与 Schema 消费者通过；macOS/Windows workspace 未通过。  
> **适用环境：** Ubuntu Linux 开发主机 + DeepSeek Harness；可使用经过用户授权的 `sudo`；跨平台结果以对应原生 CI 为准。  
> **唯一目标：** 在不扩大语言核心、不增加不必要 CLI 顶层命令、不塞入 Knowledge/Proof/View 的前提下，修复所有承诺支持功能的正确性、可复现性、跨平台和安全缺口，最终发布**有明确定义的稳定版本**。  
> **不要误读：** 本文不是从旧 `cflmy/marqdo` 仓库迁移代码的方案；当前项目是独立 Rust 实现。此文亦不承诺在首次稳定版中实现 JIT、通用 CAS、通用包注册表、IDE 或 Lean 证明。

---

## 0. 给第一次接触项目的开发者：十分钟了解任务

Marqdo 是 Markdown AST 原生承载计算语义的语言。日常文档既可以被 Markdown 阅读器阅读，也可以通过**独立的 Rust 解释器**执行。其理念为“代码即文档，文档即知识库，知识即可执行”，但**知识编译、形式证明、可视化是独立仓库**，不是本仓库内建的三个产品。

用户体验是一个 `marqdo` 命令：

```bash
marqdo run index.mq.md
marqdo call math.mq.md 计算面积 --arg r=3
marqdo check index.mq.md
marqdo add math --path deps/math
marqdo sync --locked
marqdo tool install ./some-tool/tool.json
marqdo <任意已注册工具名> ...
```

当前仓库另有 `init`、`lock`、`remove`、`inspect`、`fmt`、`plugin inspect` 等既有命令。**本阶段维持其已有合同，但不为了新功能继续增加顶层命令。** 任意新命令如 `marqdo view`、`marqdo prove`、`marqdo knowledge` 都由独立工具向通用 `Tool Protocol` 注册，而非修改内核源码。

从当前提交看：Markdown 解析/语义、原生解释器、Callable Registry、包图 lock v3、Analysis v2、Trace v2、Tool Protocol、ABI Worker、Foreign Provider 及相应测试已经存在。**核心任务是结束“实现存在，但发布证据、平台边界、安全闭环还不完整”的状态。**

### 0.1 给 Harness 的执行约束（必须遵守）

1. **先读本文件、`AGENTS.md`、`docs/STATUS.md`、相关 `spec/` 与 ADR，再改代码。** 它们可能互相过时，以当前源码与可复现测试为最终事实；遇到冲突先记录和修正文档。
2. **任何开发阶段先写能够复现缺口的测试，然后实现最小修复。** 不允许简单跳过失败测试、重写所有 golden、删除安全检查以通过 CI。
3. **普通构建永远使用非 root 用户。** `sudo` 只用于 OS/namespace/cgroup/AppArmor 诊断、测试 VM 系统配置和特定需要提权的安全测试，不使用 root 运行受试解释器或原生不可信插件。
4. **允许破坏性重构尚未稳定的内部 API**，但不擅自改 Markdown profile、已冻结语言语义和公开 ABI；若必须修改公开 Analysis/Trace/Tool schema，先给 ADR、schema 版本、迁移测试与消费者测试。
5. **不修改 CLI 来预先认识官方工具。** 第三方任意名称工具应与 View/Knowledge/Proof 完全同等接入。
6. **每阶段都给出：变更文件、为什么改、测试命令及结果、兼容性影响、剩余阻断项、commit SHA 和 CI URL。** 禁止把未运行的测试写成 PASS。
7. **发现新阻断项时可继续修复，但必须界定该项与当前发布合同的关系**；不因为“将来可能有用”增加新通用框架。正常情况下不要停下来等待设计确认；如果新问题改变语言语义/安全模型/项目发布范围，提交明确 ADR 后再推进。
8. **不要以 root、容器或某台机器上通过的结果替代普通用户和独立 CI 的验收。**

### 0.2 本文件里的用语

- **Implemented：** 当前源码存在实现，不代表全平台测试通过。
- **Verified：** 特定 commit、平台、测试命令确实通过且有日志。
- **Core Stable：** 受支持的语言、包管理、协议、运行时和 CLI 功能有完整回归测试和兼容承诺。
- **Strict Available：** 对特定 Linux 环境真实启用了隔离并通过逃逸/资源限制测试；未通过的环境继续 fail-closed。
- **Release blocker：** 在该版本明确承诺的支持矩阵内失败，必须修复才能发布。

---

## 1. 真实仓库基线，不要重新设计已经做完的东西

### 1.1 需要首先读取的资料

```text
README.md
AGENTS.md
Cargo.toml
crates/cli/src/main.rs
crates/cli/src/{commands,loader,analysis,trace,tool}.rs
crates/marqdo-package/src/{lib,fileset}.rs
crates/marqdo-host/src/{host,capability,plugin_worker,fs,handles,budget}.rs
crates/marqdo-foreign/src/lib.rs
crates/marqdo-sandbox/src/lib.rs
crates/marqdo-plugin-abi/src/{manifest,worker,wire,loader}.rs
spec/{markdown-profile,mapping,expression.ebnf,evaluation,references}.md
spec/{package-format,analysis-v2,trace-v2,tool-v1,plugin-abi,foreign,security}.md
docs/adr/0008-0019*.md
docs/STATUS.md
.github/workflows/ci.yml
crates/cli/tests/
tests/conformance/; tests/consumers/
```

现有代码库大约十五个 Rust crate，已有多种金样与负例，不需创建一套新的 monorepo 或把现有 crate 重新拆成更多微型 crate。

### 1.2 仓库已完成的基础（不要重复开发）

| 子系统 | 实际能力 | 源码/证据 | 需要保持的边界 |
|---|---|---|---|
| Markdown profile / AST | 固定 Markdown profile、金样解析 | `marqdo-markdown`、`spec/markdown-profile.md` | 不让工具插件改写 Markdown 语义 |
| 原生运行时 | 函数/表达式/分支/循环、原生 Rust 求值 | `marqdo-frontend`、`marqdo-runtime` | 不能经 Python 转译执行 |
| Callable Registry | 内置、模块函数、声明式 ABI export 的统一可调用描述 | `marqdo-ir/src/callable.rs`、ADR-0009 | 扩展可调用函数不修改多个 `match` |
| Package / lock | lock v3 全传递闭包、包实例身份、Git 固定版本、环境物化 | `marqdo-package`、ADR-0016 | 锁失败拒绝；`check` 不抓取依赖 |
| Analysis v2 | 逻辑身份、函数/公式/代码资源投影、独立消费者 | `cli/src/analysis.rs`、`spec/analysis-v2.md` | 外部不用私有 Rust IR |
| Trace v2 | 模块归属、容量预算、截断标识、来源摘要 | `cli/src/trace.rs`、`spec/trace-v2.md` | 没有执行就不产生 trace；不漏密钥 |
| Tool Protocol | 任意命令注册、SemVer 兼容、整包摘要校验 | `cli/src/tool.rs` | 不能内置 View/Proof/Knowledge 特判 |
| ABI Native/Worker | ABI v1、多导出、Worker IPC 与能力读取桥 | `marqdo-plugin-abi`、`marqdo-host`、ADR-0018 | Worker 不是 sandbox |
| Foreign Provider | 可外部注册、固定 provider/环境目录摘要 | `marqdo-foreign`、`spec/foreign.md` | 不把 uv/npm 硬编码进解释器 |
| 安全门禁 | `strict` 不可用时拒绝，不降级 | `marqdo-sandbox`、ADR-0019 | 仅真实隔离通过安全测试才翻开门禁 |

### 1.3 最新公开 CI 的确切结果（与旧 STATUS 描述区分）

在 `e966b766` 的 [CI #44680378977](https://github.com/cflmy/marqdo-cli/actions/runs/44680378977)：

- Linux `fmt + clippy + cargo test --workspace + smoke`：**PASS**；
- Schema 实例验证 + Analysis/Trace 独立 Python 消费者：**PASS**；
- macOS `cargo test --workspace`：**FAIL**，`integration_cli::plugin_inspect_and_explicit_trusted_invocation` 假设 Linux `x86_64-unknown-linux-gnu` 插件 artifact 在 `aarch64-apple-darwin` 也可用，得到 `no_matching_artifact`；
- Windows `cargo test --workspace`：**FAIL**，`conformance_ast::ast_goldens_match_the_frozen_profile` 对第一个 golden 字符串不等，但按 `.lines()` 显示“0 line(s) differ”，需用字节级比较确定 CRLF/LF 或其他差异；
- 后两者因为 workspace 失败，后续 portable smoke 和 strict refusal 没执行。

**重要：** 所以截至此基线，不能写“全平台 CI 绿”、“G-B 已验收”。`docs/STATUS.md` 中“CI URL 缺失”现已过时，应修正到此 CI 链接，并记录红色结果。也不要将 Linux 的成功说成 Ubuntu 上的严格 OS 隔离成功。

### 1.4 本阶段禁止扩张的项目

不得为所谓稳定版强制加入：JIT/字节码、Web UI、Lean、知识图谱、向量数据库、通用插件市场、GitHub 注册表、SemVer SAT 求解器、长期 Foreign session、完整数学 CAS、通用调试服务器。除非已有正式语言规范要求其中某项才能保证正确性，否则全部列入后续独立项目或未来版本。

---

## 2. 目标架构、依赖方向与跨仓职责

```text
用户输入 `marqdo ...`
   │
   ├── Core command surface: run / call / check / add / sync / tool
   │      （现有 init/lock/remove/inspect/fmt/plugin inspect 暂时兼容）
   │
   ├── Generic tool dispatcher ── tool.json / tool registry
   │      └── external executable: 任意工具名
   │              ├── marqdo-knowledge (独立仓)
   │              ├── marqdo-proof     (独立仓)
   │              ├── marqdo-view      (独立仓)
   │              └── 第三方插件工具（和官方平等）
   │
   └── Core compile & execute
          ├── Markdown parser + language mapping (固定语义)
          ├── Semantic analysis + CallableRegistry
          ├── PackageResolver / lock v3 / physical source map
          ├── Analysis v2 export (公开投影，不是私有 IR)
          ├── Rust interpreter
          │     ├── Value + error + module-aware span
          │     └── Host authorization and effects
          │            ├── Math
          │            ├── Foreign Kernel Provider
          │            └── ABI Native / Worker
          ├── Trace v2 export
          └── strict OS isolation adapter (Linux security gate)
```

### 2.1 三个外部扩展域禁止混用

1. **CLI Tool：** `marqdo tool install ./tool.json` 注册一段独立本地程序，`marqdo benchmark ...` 通过通用分发器执行。它以使用者权限运行，不承诺自动受 Marqdo 语言沙箱保护。
2. **ABI Plugin：** 扩展 `.mq.md` 调用目标，以 C ABI v1、声明导出和 Host 权限判断为界。可信 Native 可进程内；Worker 只提供崩溃隔离，未进 strict 时不具备敌意代码沙箱。
3. **Foreign Kernel Provider：** 显式执行外部代码块内容，需要独立的 provider/executable/env/授权/协议。不能通过 CLI Tool 安装自动变成可执行的 Marqdo 内核，也不能靠一份 JSON 声明获得系统权限。

### 2.2 严格单向依赖

- `marqdo-markdown` 不依赖 runtime、host、tool、Web。
- `marqdo-runtime` 不依赖 Tool、Knowledge、Proof、View。
- `marqdo-package` 负责包图和来源；`cli` 只负责命令装配，不自行解释包依赖。
- `marqdo-host` 负责执行 effect 授权/代理，`marqdo-sandbox` 负责操作系统机制与能力检测；**Host 授权 ≠ OS 隔离**。
- 外部工具只依赖发布的 `marqdo` 命令、Analysis/Trace v2、Tool v1 协议，不链接私有 `marqdo-ir` Rust 类型。
- core 不认识 `knowledge`、`prove`、`view` 的具体名字，不维护官方专属白名单和下载地址。

### 2.3 语言与协议冻结

Markdown profile、代码节点判定、表达式语法、参数推断、显式导入和引用不执行属于语言层；不能为了 View 或 Proof 变通。Analysis v2、Trace v2、Tool v1 是面向外部消费者的交换层。新增不兼容字段须出独立版本及迁移测试；内部 IR 可以破坏性重构，不得泄露私有 Host 句柄或内存地址。

---

## 3. DeepSeek Harness 与 Ubuntu 的开发环境和提权规范

### 3.1 推荐开发环境

**推荐配置：** 独立 Ubuntu 24.04 LTS x86_64 主机或 KVM VM、普通开发用户、Git/Rust、Python3、C 编译器、Node.js（仅用于 Foreign 集成测试）、Bubblewrap、AppArmor 工具，以及可操作的 cgroup v2。内核可支持的机制必须用当前主机实际 probe 判定，不能凭操作系统版本猜测。

如果用户宿主机没有敏感数据，允许 Harness 用 `sudo` 安装软件/配置专用测试环境。但请始终遵守：**root 是 Harness 的管理权限，不是 Marqdo 被隔离程序的运行身份。** 一些安全测试必须由非 root 发起，以证明真实普通用户体验。

**建议流程：** 先由用户用 `sudo -v` 建立提权会话，确认 `sudo -n true` 是否成功；Harness 只对必要命令显式写 `sudo`。对于完全可销毁的 VM，可给特定开发账户 `sudo` 组或配置无密码提权，但必须让 Harness 将每次系统级改动记入日志，完成测试后恢复与验证。不要在日常主机上直接禁用系统 AppArmor 或永久修改全局 namespace 策略来制造虚假的 PASS。

### 3.1.1 如果确实需要让 Harness 无交互提权（仅限可销毁专用 VM）

普通 Ubuntu 主机先用管理员账户将开发用户加入 `sudo` 组（若尚未有权限），然后**重新登录**该用户：

```bash
# 在管理员终端执行；将 devuser 替换为实际开发用户名
sudo usermod -aG sudo devuser
# 重新登录开发用户后检查
id devuser
sudo -v
```

`sudo` 组默认可能仍要求输入密码。若 DeepSeek Harness 的终端不能交互输入密码，而你完全掌控、可以随时重建这台专用 VM，可以**有意识地**为此开发用户建立单独的临时 sudoers 规则：

```text
# 用 sudo visudo -f /etc/sudoers.d/marqdo-harness 编辑并校验语法
# 注意替换 devuser；下面允许该 VM 的此用户免密执行任意 sudo 命令
devuser ALL=(ALL) NOPASSWD: ALL
```

保存后用 `sudo visudo -c` 校验，切回 Harness 的开发账户执行 `sudo -n true`。如果无法通过，不要让 Harness用 `sudo` 的失败来推断内核缺少 namespace；先检查 Harness 自身终端 sandbox/权限审批、宿主 sudoers、父级容器限制。**完成开发后移除临时 sudoers 规则**（在管理终端执行 `sudo rm /etc/sudoers.d/marqdo-harness` 并再次 `sudo visudo -c`）。

这只是针对可销毁 VM 的开发便利设置，不应提交仓库、提供给普通 Marqdo 用户或用于发布的普通用户权限测试。不要把 root 作为 `marqdo` 普通运行的系统要求。

### 3.2 开始执行前输出环境基线

```bash
set -u
whoami; id; id -u
sudo -n true && echo 'SUDO_NONINTERACTIVE_OK' || echo 'SUDO_REQUIRES_APPROVAL'
cat /etc/os-release
uname -a
rustc -Vv; cargo -V; git --version
command -v cc || true; command -v python3 || true; command -v node || true
command -v bwrap || true; command -v aa-status || true
systemd-detect-virt --container || true
findmnt -t cgroup2 || true
cat /sys/fs/cgroup/cgroup.controllers 2>/dev/null || true
sysctl kernel.unprivileged_userns_clone 2>/dev/null || true
sysctl kernel.apparmor_restrict_unprivileged_userns 2>/dev/null || true
sysctl user.max_user_namespaces 2>/dev/null || true
unshare --user --map-root-user true; echo "userns=$?"
unshare --user --map-root-user --net true; echo "userns_net=$?"
```

**记录诊断，而不是盲目修改系统：** `EPERM` 可能由 Docker/LXC 的父级 seccomp、AppArmor、namespace 上限或权能限制导致。`sudo` 可能解决某一子项，也可能完全不能绕过上级容器配置。若不能在普通用户环境下可靠测试 strict，请建立专用可控的 Ubuntu VM 或 self-hosted security runner，不要在普通 CI 上假装执行了安全逃逸测试。

Ubuntu 24.04 AppArmor user namespace 参考：https://documentation.ubuntu.com/release-notes/24.04/ ；Bubblewrap 机制说明：https://github.com/containers/bubblewrap 。

### 3.3 Harness 的权限管理边界

1. `cargo build/test/clippy/fmt`、Git 修改/提交和普通 `.mq.md` 的 run：**非 root**。
2. 安装 apt 依赖、`apparmor_parser`、配置测试专用 namespace/cgroup 管理权：**按需 `sudo`**。
3. 用 root 建立隔离机制可行，但子程序必须降权并验证有效 UID/GID 与实际受限权限。
4. 每个涉及 `sudo` 的系统变更都记录操作命令、变更前后值、回滚方式和验证结果。
5. 不运行项目依赖中未知的安装脚本，不允许“为了测试通过”关闭 `strict` 的拒绝逻辑。
6. 如在 root 环境可运行而普通用户不能运行，则判定**普通用户支持门槛仍失败**，而不是宣称 strict 可用。
7. 不以当前主机没有敏感信息作为放弃安全负例测试的理由：发布给他人的 CLI 将运行在拥有真实数据的主机上。

### 3.4 建议工作区组织

```text
marqdo-cli/                   # 正式受版本控制的代码
  docs/STABLE_RELEASE_IMPLEMENTATION.md
  docs/STATUS.md
  docs/adr/...
  crates/...
  spec/...
  tests/...
  .github/workflows/...
../marqdo-security-lab/       # 测试脚本、恶意 fixture、临时沙箱数据（如需要）
../marqdo-evidence/           # 日志、真实 CI ID、检查报告（不要误提交敏感环境值）
```

系统测试 VM 快照/恢复流程应单独保存，**发布不能依赖当前开发主机的隐含配置**。

---

## 4. 发布目标、版本语义和范围

### 4.1 什么叫“稳定版”

稳定不是“所有想象功能都实现了”，而是**所有明确承诺的接口、执行能力、平台支持和安全语义都经测试成立，并对未来版本有可持续兼容约束**。

建议实行：

- **Linux Ubuntu x86_64：** 首个优先稳定目标，支持语言、包、Tool、Analysis/Trace、ABI、Foreign 的已声明子集。若版本对外宣称 `strict` 可用，则该 Ubuntu 平台必须完成 G-C 安全门禁。否则发布中只能明确写“strict 不可用”，不得虚称完整安全版。
- **Windows / macOS：** 可作为 Core Stable 支持平台，但只有对应原生 runtime / Tool / 外部协议 / 包环境测试全部通过才能列为支持平台；未实现 strict 的平台持续 fail-closed。不必为了跨平台严格沙箱而让 Linux Core 永远无法发布。
- **Foreign Python/Node：** 支持已声明、已固定/校验的 Provider 与环境；自动下载、安装包和运行任意 npm/uv 脚本不是语言核心的稳定性要求。如果发版文案承诺“自动物化 uv/npm 环境”，就必须实现独立可复现的流程、相应测试和 provenance；否则不作该承诺。
- **Knowledge/Proof/View：** 不存在于 CLI release bundle，只有公共 Tool/Analysis/Trace 兼容合同；不发布它们的功能性声明。

### 4.2 版本命名建议

当前工作区版本在基线附近为 `0.1.0`。不应因“用户希望稳定”就机械增加版本号：

1. 先做工程内部 Milestone `release-candidate`，通过第 5–12 章全部适用门槛；
2. 以 `v0.2.0-rc.1` 等候选版本演练（实际具体号应先检查现有 tags/releases 和 semver 计划）；
3. 若语言、公开 ABI、Tool、Analysis/Trace 和包文件格式的兼容承诺均已冻结、受支持平台测试为绿，可考虑发布 `v1.0.0`；否则也可以发布明确定义稳定支持范围的 `v0.2.0`，不冒充全部能力稳定；
4. 任何标签都不能代替 CI、源码、release asset 与校验和证据。

### 4.3 分离的三道门禁

- **G-A / Ecosystem Development Ready：** 通用 Tool + Analysis/Trace 可以被陌生外部消费者读取，适合开始 View/Knowledge/Proof 独立开发。当前源码接近具备，不要求 strict。
- **G-B / Core Stable：** 当前承诺的语言/包图/协议/Native/Foreign 已严格测试；Linux CI 全绿，Windows/macOS 对支持平台全绿，文档/兼容矩阵真实。为稳定核心版本的必要条件。
- **G-C / Linux Strict Verified：** 在有权限和机制的 Ubuntu 环境执行真实 OS 隔离与逃逸测试通过；未通过平台拒绝，不隐式降级。只有声称支持严格隔离才需要对相应平台取得 G-C。

**本次用户期望：** 建议 Harness 尽力把 Ubuntu Linux 的 G-B + G-C 全部完成，同时修正 Windows/macOS 的已确认 CI 失败并提升跨平台真实测试；不能因个别环境缺少 namespace 自动认定工作已经完成。

## 5. P0：先把所有平台的现存 CI 测试修到真实全绿

> 优先级：Release blocker。必须在开发严肃隔离功能之前解决。完成后才有可信的回归基线。

### 5.1 Windows AST golden：先证明是字节问题还是语义问题

**证据：** 远程 `e966b766` 的 Windows job 在 `crates/cli/tests/conformance_ast.rs:134` 失败；`001-atx-h1.mq.md` 报告 expected != actual，但 `.lines()` 比较显示“0 line(s) differ”。`conformance_ast.rs::golden_text` 对 AST 序列化加一个 LF，`report_mismatch` 用 `.lines()` 打印，因此最有可能是 expected 中 CRLF 与 actual 中 LF 不一致；此为**高可信假设，尚须字节测试确认**。

**实施步骤：**

1. 在 Windows runner 或 Windows VM 取 `tests/conformance/ast/001-atx-h1.ast.json` 原始字节，记录 `CRLF`/`LF` 数量、文件长度及第一个不同字节。不要靠肉眼看渲染文本。
2. 在 `crates/cli/tests/conformance_ast.rs` 添加更有用的字节级差异诊断：首个不同字节位置、附近转义十六进制、是否仅行尾不同。**不要**修改 Markdown 语法来迁就测试文件换行。
3. 采用统一且可重复的金样存储策略。优先通过 `.gitattributes` 为 `.mq.md` / `.ast.json` / `.json` / `.yaml` 等需严格字节身份的规范 fixture 指定 `text eol=lf`；但若 `.mq.md` 语义测试需要保留 CRLF 特例，必须把这些特例单独设计为字节 fixture，而不是盲目全量归一化。
4. 在 `git add --renormalize` 后审查变更，避免意外修改语言原始文本或 Span 测试的期望。对依赖原始字节 Span 的 case，明确分别测试 LF/CRLF 输入的行为。
5. 再运行 `cargo test -p marqdo-cli --test conformance_ast` 于 Windows、Ubuntu、macOS，确认 AST 金样一致或将正当的换行行为作为显式语义规则。

**强制负例：** 人为更改 AST golden 的一个节点名称/Span 必须失败；仅当 profile 有经 ADR 批准的语义变更才可使用 `UPDATE_GOLDENS=1`，并提交完整 diff 审查。**绝不以批量重生成 golden 代替定位根因。**

**完成证据：** Windows 失败 fixture 再现后已消失；LF/CRLF 的语言语义与 Span 测试有独立可检查预期；同一 golden 由至少 Linux/Windows 读取通过。

### 5.2 macOS 插件测试：构建当前平台 artifact，不能依赖 Linux 目标

**证据：** `crates/cli/tests/integration_cli.rs::plugin_inspect_and_explicit_trusted_invocation` 在 macOS `aarch64-apple-darwin` 报 `no_matching_artifact`；仓库示例 `examples/plugin-demo/plugin.yaml` 只声明 `x86_64-unknown-linux-gnu`，`build.sh` 强制输出 `.so` 且使用 Linux `sha256sum`。

**必须改成两层测试：**

1. **跨平台的 Manifest/ABI 静态测试**：验证非本平台 artifact 时报告精确的 `no_matching_artifact`；不会隐式使用任意 `.so`/`.dll`/`.dylib`；不加载代码。可以跨平台无条件运行。
2. **真实动态库执行测试**：仅在本平台已构建兼容目标的 artifact、C 工具链具备时运行；Linux 使用 `.so`，macOS 使用 `.dylib`，Windows 使用 `.dll`；构建脚本/测试 harness 生成临时清单与正确 SHA-256。不能把不存在的平台产物当作语言解释器错误。

**建议实现：** 将示例插件构建逻辑抽成测试 helper（使用 `rustc -vV` 的实际 `host`、平台动态库后缀、可用 C 编译器），构建产物和 `plugin.yaml` 写入 tempdir。原 `examples/plugin-demo/build.sh` 可保留为 Linux 开发示例，但不能成为所有平台 CI 的隐含前提。**不要在测试中原地修改仓库内受版本控制的 `plugin.yaml`。**

**测试要求：** 当前平台成功 `plugin inspect` + Native（显式 trust）调用 + Worker 调用；故意给出其他平台 target 必须失败；错误 sha256、错误 ABI major、缺导出、多导出冲突必须失败且不发生隐式加载。

**完成证据：** macOS workspace 与相应 Smoke PASS；Windows 的 C ABI 动态库/Worker 测试若尚无条件，明确单列未支持并不得在发布矩阵中宣称已验证。

### 5.3 CI 自身的验证质量

需要把 `.github/workflows/ci.yml` 调整成**失败归因清楚、证据独立且不得静默跳过**的门禁：

- Linux 核心 job：`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`、示例 smoke。
- Schema job：从真正 CLI 产出 Analysis v2、Trace v2，按相应 JSON Schema 验证，同时用独立 Python 消费者读取、验证 digest/stale 与非法输入。
- macOS / Windows：所有宣称可移植的核心测试真正运行，而不只 `cargo check`；C ABI/Worker/Foreign 运行测试必须单独明确 supported/skipped/failed，而非空测试视为成功。
- Linux strict security lane：**独立**于普通 CI，见第 10 章；不把缺权限普通 runner 的 `EPERM` 当作隔离机制已通过。
- Release job：只针对同一 SHA 的所有必需 job 成功才允许发 tag 或上传发行资产，禁止在红 CI 上发布。

CI 中任何 `#[cfg(unix)]` 或 `if tool_available` 跳过测试，都要有一张支持矩阵。**跳过不计为 PASS。** 外部依赖如 Python、Node、C toolchain、git，在目标平台应通过 CI 安装以保证该平台的承诺测试真正运行。

### 5.4 H0 验收命令

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test -p marqdo-cli --test conformance_ast
cargo test -p marqdo-cli --test integration_cli
cargo test -p marqdo-cli --test tool_protocol
cargo test -p marqdo-cli --test analysis_trace
```

每条命令必须以非 root 用户执行并记录运行时间、平台、退出码。CI 完成后将实际 SHA/URL 写入 `docs/STATUS.md`。如果有测试失败，下一轮仅专注修复，不开始以 `strict` 启用为目标的 PR。

---

## 6. P1：包图 lock v3、环境同步与正确性收尾

> 目标：继续使用现有 ADR-0016 与 lock v3，不重建包管理器，不增加 `tree/update/vendor` 命令；验证**真实可复现**而非“路径解析得到”。

### 6.1 现状与核心保证

`crates/marqdo-package/src/fileset.rs` 已将 `.git`、`.marqdo` 排除，限制普通文件、symlink、数量/大小/深度；`spec/package-format.md` 描述 package instance graph、完整依赖边、根 manifest digest 和严格 sync，`run/call/check` 不得自动下载或重写锁。

**不允许倒退到旧 lock v2 的“只锁第一层依赖”模式。** 正式版本的 lock v3 必须保证：同一来源/内容得到同一 package instance，不因缓存物理路径而变；路径/Git/传递依赖闭包完整；内容变化导致明确 stale，不能默默回退最新版本；同一个 alias 在不同 parent 下可以指向不同包实例。

### 6.2 必须补齐的测试与可疑边界

| 要求 | 正例 | 负例 / 破坏性输入 | 验收 |
|---|---|---|---|
| 跨目录确定性 | 项目复制到另一绝对路径后 lock/Analysis 与包身份一致 | 硬编码项目根路径 | 逻辑身份和摘要一致 |
| 完整依赖图 | A→B→C，B 与另一个包拥有同名 alias | 删除 C 的 lock 节点/依赖边 | 不能 `run`、`sync` 绕过缺失校验 |
| 多版本包 | B@v1、B@v2 并存 | 隐式选择先加载版本 | 唯一身份与显式引用 |
| Git pinned | 固定完整 SHA、已缓存离线重放 | URL/SHA/缓存内容篡改、修改 marker | 在加载前拒绝，不执行 hook |
| 文件集合 | 正常文件、排除 `.git`/`.marqdo` | symlink、特殊文件、超深、过大、遍历错误 | 锁和复制使用同一列表与字节 |
| 环境发布 | 已存在环境，原子更新成功 | 途中复制失败、无空间、并发 sync | 旧可用环境可恢复；不留半环境 |
| 锁严格 | `sync --locked` 与 `sync` 一致校验 lock | 旧 lock v1/v2、root manifest drift | 拒绝，并给出 relock 指引 |
| 检查副作用 | `check` 静态加载缓存资源 | 缺失 Git cache 时偷偷 clone | `check` 不发生网络写入/执行 |

### 6.3 建议代码落点

- `crates/marqdo-package/src/fileset.rs`：保持文件集 profile 的单一实现，增加错误路径和特殊文件测试；若优化 hash，必须与物化环境摘要一致。
- `crates/marqdo-package/src/lib.rs`：锁图解析、完整闭包、环检测、不可变已锁解析、cache marker 与原子 sync。
- `crates/cli/src/commands.rs`：`lock` 与 `sync` 命令只做交互装配，不自行重写 PackageResolver 的业务算法。
- `crates/cli/tests/package_env.rs`：通过真正 binary 验证 Git/path、传递边、stale drift 与跨目录行为；必要时在 `tests/fixtures` 增加稳定的小仓库 fixture。

**尽量不添加新抽象：** `SourceResolver` 已能处理 path/Git 时不要为了未来 registry 多建立“通用 source framework”。未支持来源明确报错即可。异常处理和缓存完整性优先于可扩展来源种类。

### 6.4 本阶段发布阻断项

如发现 `lock` 具有路径依赖不稳定、包内容变更未发现、物化时漏项、`sync` 会破坏现有可用环境或 `run/check` 在未锁定时自动获取依赖，则**阻断 G-B**。但不需要在本阶段开发公共包市场或网络下载代理。

---

## 7. P2：Analysis / Trace / Tool 公共协议最终稳定化

> 目标：证明三个外部仓库只依赖 CLI 已发布协议就能获得真实结构与执行证据，核心无须为它们写任何专门代码。

### 7.1 Analysis v2 应维持的合同

`marqdo check <file> --emit analysis.json` 必须：

- 仅静态解析/语义分析，不执行任何 `.mq.md` 指令、不加载动态库、不启动 Foreign；
- 输出明确的 `schema: marqdo.analysis`、`schema_version: 2`、语言 profile、生产者版本、逻辑项目身份、包实例/模块身份、源码摘要；
- 输出可用的函数签名、受支持的函数体语义节点、call/reference/import 边、作用域、资源信息（Formula / Code，包括来源 Span 与 digest）；
- 跨机器/跨目录移动相同源与锁时不出现物理路径污染；
- 写入以原子方式完成；静态错误禁止遗留旧成功 artifact；
- 不声称“已经证明”“已经执行”“Unknown effect 是纯函数”。

**独立消费者最小验证：** Python / JavaScript 任意一门非 Rust 语言实现约百行样例，读取 `analysis.json`，查找指定函数、查找资源来源、解析跨模块调用、验证 Schema 与字节摘要。故意以 v1/v99 和错误来源 digest 输入时正确拒绝。

不要把 `marqdo-ir` 用 serde 直接输出，也不要继续叠加 `inspect --ir` 的私有格式作为公开 API。Proof 将来需要类型和语义模型的额外属性时，由独立消费用例驱动 `Analysis v2.x` 附加字段或 `Analysis v3`，而非现在就设计完整 Proof IR。

### 7.2 Trace v2 应维持的合同

`marqdo run/call --trace-file trace.json` 必须：

- 每个含 Span 的事件有真实 `module_id`；入口文件与被导入模块错误不会混淆；
- Trace 的 `analysis_digest` 等于按规范计算的同一项目 Analysis 产物字节摘要，并可识别 stale；
- 调用、分支、循环、返回、Host effect 的全局 sequence 单调；
- 大循环触发事件数/字节预算后记录 `trace.truncated`，不把执行结果改成成功或失败的错误状态；
- 默认不记录密钥、任意文件内容、插件 opaque handle；
- 没有实际执行的 `check`、View 阅读态等操作不能伪造 Trace；
- 截断、错误路径、取消和写盘失败时有清楚状态而不是半合法成功 JSON。

**额外稳定性测试：** 长时间运行的循环、含跨模块调用的链、插件/Foreign effect、超大输入、写盘失败、trace cap=0 的无限制语义（如允许此选项则须对内存保留边界），事件按模块追溯源码。考虑为实际写入模式增加流式或受限内存实现，但**只有已经观察到风险/压力测试失败时**才进行大规模重写。

### 7.3 Tool Protocol v1 应维持的合同

`marqdo tool install ./tool.json`、`list`、`remove` 和未知子命令分发：

- 工具名来自 manifest，官方与第三方无任何优先接入差别；保留核心命令优先、命令名冲突拒绝；
- 整包（不止入口文件）摘要与文件列表，在安装与运行时校验；目录逃逸、symlink、缺失/多余文件明确拒绝；
- `compatibility.marqdo` 用标准 SemVer；使用 Analysis/Trace 的工具在安装与 dispatch 时检测其支持的协议主版本；
- 保留调用者 `cwd`、stdin/stdout/stderr、原样 argv Unicode 参数和真实退出码；不用 shell 拼接执行；
- 不扫描 PATH 中的未知可执行文件并直接赋予 `marqdo` 子命令权限；
- Tool 本身不获得任何语言内部 Host API 权限；允许 Tool 自行通过公开 `marqdo check/run` 命令获取产物；
- 软件包安装失败必须回滚，删除不能越出 Tool home；可预防同目录并发安装与删除导致的污染。

**发布兼容测试：** 用源码中根本没提到的随机合法 command 名、独立 Python/Go/Rust 二进制，调用安装、执行、升级（首版可用 remove+install）、参数与退出码、篡改与拒绝。更换工具名或实现语言不能需要重新编译 `marqdo`。

### 7.4 协议演化与兼容矩阵

| 范围 | 已有版本 | 兼容性政策 |
|---|---|---|
| Markdown profile | `marqdo.md.v1` | 语义变更单独 ADR + 金样 + profile 版本 |
| 语言/CLI | workspace 版本当前 `0.1.0` | 稳定发布之后按 SemVer；CLI flags 破坏要明确声明 |
| `marqdo.lock` | v3 | 旧 v1/v2 明确拒绝，命令给出 relock 指引 |
| Analysis | v2 | 不兼容结构变更升级 major；消费者拒绝未知 major |
| Trace | v2 | 同上；source digest/staleness 语义固定 |
| CLI Tool | v1 | Manifest schema、version negotiation、bundle hash 稳定 |
| ABI | v1 | 不允许不经版本协商破坏二进制 layout |
| Foreign wire | 当前受支持版本 | 每次协议变更需独立 provider 的兼容测试 |

每项协议必须有**有效实例测试 + 非法实例测试 + 独立消费者测试**，而不是仅检验 schema 文件能被 `json.loads` 解析。任何协议示例都要与当前代码真实输出一致，不可以靠手写 JSON 代表测试。

---

## 8. P3：Foreign Provider / Python / Node 的环境闭环（B-12）

> 目标：修复“完整性校验存在，但环境关联与解释器选择可能不一致”的真实边界。不要把 uv/npm 发展为内核的另一个包管理器。

### 8.1 当前设计事实

`crates/marqdo-foreign/src/lib.rs` 已具有 `KernelRegistry`、外部 `KernelDescriptor`，后者可声明 `digest`、`environment_id`、`environment_digest`、`environment_dir`、`interpreter_version`；外部 provider 与其环境目录在注册及实际 spawn 前校验 SHA-256。环境目录的文件集禁止 symlink 和特殊文件，并有资源上限。已有 provider 正反测试。

**仍需专项核验：** 当前 `execute()` 会先 `resolve_program(descriptor)` 得到可执行路径，之后设置子进程的 `MARQDO_FOREIGN_ENV` 和扩展 `PATH`。对于显式外部 provider，其入口是 descriptor 的绝对路径；对于内置 Python/Node，`resolve_program` 用宿主 PATH 找出可执行文件，再给子进程增加环境 `bin`。因此校验了某个环境目录，并不自动意味着运行的解释器就是该目录中的解释器，也不自动意味着解释器依赖只来自固定环境。这是**代码可见的语义缺口/需要契约明确的行为**。

### 8.2 必须先决定“谁负责环境创建”

推荐设计：

1. **Marqdo Core 负责**：解析外部 Kernel manifest、明确 provider executable、校验 provider/env 内容摘要、在发起运行前校验版本协议、提供纯粹的生命周期/授权/超时/结果封装。
2. **可选、明确授权的外部 Provisioner 负责**：创建/下载 Python uv 或 Node npm 依赖，产生固定可执行路径、环境摘要和可审查的 recipe/lock；此工具可以通过通用 Tool Protocol 提供，但不能要求 CLI 特判它的名字。
3. **项目作者负责**：把 Provider 描述和所需环境工件/recipe 显式提交或登记。同步或检查 `.mq.md` 不隐式执行第三方安装脚本。
4. **不能宣称未实现的能力**：若没有自动物化功能，稳定版文档明确“支持固定环境验证，不自动创建”；这并不妨碍 Foreign v1 成为稳定的、范围明确的协议。

如果本轮确实要补齐 B-12 的**自动创建**，建议只交付一个**独立可执行的参考 provisioner**，在 `examples/` / `tools/` 作为单独 Tool fixture；让其对固定 uv lock / npm lock 做显式、可审计安装并输出 environment descriptor。不要在 `marqdo sync` 中引入运行 npm 生命周期脚本的隐式步骤。Tool 可以先支持最简单两种 recipe，不做通用软件包管理平台。

### 8.3 运行时必须绑定真实解释器与环境

- 如果 provider 声明 `environment_dir`，定义它是“只提供资源路径”还是“必须使用其解释器”；接口不能含糊。对于完整可复现模式，优先**固定 provider 可执行文件绝对路径与摘要**，并明确环境下的解释器和模块导入目录。不能只靠 prepend PATH 获得正确执行语义。
- 对环境内真实 interpreter 的定位规则必须稳定且可验证。若固定环境没有声明的解释器，则 fail-closed，不能回退宿主 `python3`。
- Python `sys.executable`、`sys.path`；Node `process.execPath` 和模块解析路径必须纳入测试证据。平台不同的 `bin/`/`Scripts/` 差异不能隐式假定相同。
- `interpreter_version` 如声明则必须做匹配检查或明确仅为展示字段；不能写了字段但实际不检查却宣称环境完全固定。
- `environment_digest` 必须和运行环境绑定；资源改动应在 spawn **之前**拒绝，已启动子进程不被实时文件改变强保证覆盖（不保证 TOCTOU 不可能，若需要强保证应从受保护的 immutable snapshot 启动）。
- Foreign 的授权仍由 Host 完成；`env_clear`、临时 cwd 和超时只是安全卫生，不是对恶意 Python/Node 的 OS 沙箱。

### 8.4 测试矩阵

| 测试 | 输入 | 期待 |
|---|---|---|
| 固定 provider 正例 | 明确绝对路径 + 正确 SHA + 完整环境 | 成功执行、产出 wire 值 |
| Provider 篡改 | 校验后修改可执行文件 | spawn 前拒绝 |
| Env 篡改 | 在环境目录中增删/修改普通文件 | spawn 前拒绝 |
| 双 Python 测试 | 宿主 Python 与环境 Python 明显不同 | 实际 `sys.executable` 与声明约定一致 |
| 双 Node 测试 | `process.execPath` 与声明不一致 | 明确失败或按定义选择固定解释器 |
| 版本漂移 | 更换解释器、版本标签错误 | 不得当作成功固定环境 |
| symlink/特殊文件 | 添加 symlink/FIFO | 检查拒绝 |
| 无授权 | 未提供 `--allow-foreign` | 绝不启动子进程 |
| 超时与进程树 | provider 创建子进程并卡住 | 所有可管理后代清理，无残留 |
| 不受支持系统 | 外部 Kernel 无平台适配 | 清楚提示不可用，不能冒用宿主默认 |

**门禁：** 上述固定环境行为、协议、可靠退出和基本 Foreign 测试必须在宣称提供 Foreign 稳定支持的平台通过。uv/npm 自动创建功能可以作为独立参考工具附带，也可以明确不属于首个核心稳定版；不要留下“似乎已经自动创建”的文档错觉。

---

## 9. P4：C ABI、Worker、能力代理及故障隔离完整性（H3-A）

### 9.1 不要把三种安全保证混为一谈

- `CapabilityBroker`：授权与 Host 受控代理。它可以拒绝 `fs.read` 越权请求，但**不能阻止 native/worker 插件直接调用系统函数**。
- `Worker`：地址空间与异常隔离，避免原生插件 crash 直接杀死解释器，支持 IPC timeout/请求限制。它仍然通常以用户权限运行。
- `Strict OS isolation`：操作系统强制的文件系统、网络、进程和资源边界；须 G-C 真实测试才能对不可信代码声明安全。

### 9.2 当前已经具备的能力

ADR-0018 已定义 worker `capability_request`/`capability_reply` 与请求 ID、Host 绑定真实插件身份、单次调用 deadline；当前已经通过 `ProjectFs` 实际执行获授权 `fs.read`，Native/Worker 都有测试，单次 64 次、每次 64 KiB 请求预算。**不要重做一个全新的 RPC 框架。**

### 9.3 仍需完成的工程工作

1. **Session / invocation 身份与生命周期**：Host 绑定 `worker instance + package + run/session/generation`；消息中声称的 package/id 不能获得权限；跨 invocation 旧句柄不可冒用。已有 `PluginHandleTable` 应继续成为权威表。
2. **实际可用能力名单**：当前仅 `fs.read` 构成完整实际服务闭环。`fs.write`、`net.request`、`process`、`clock`、`random` 等不能因为“Broker 返回允许”就被宣称已经提供完整实际代理。如果不在本版承诺，返回 `capability.unavailable` 且有稳定错误码；严禁返回空成功骗过插件。新增实际 Host API 仅从明确用途和可测试 contract 出发。
3. **Worker 传输失败后不能复活**：半帧、超长帧、错误 reply id、EOF、无效 JSON、进程 crash 后应标记会话失效并 kill/reap。不能继续使用可能错位的 frame。
4. **预算**：每 invocation 的 Host capability 请求数、字节数、完成时间包含往返；CPU/内存/子进程树独立于这些 IPC budget，不能以其中一项替代所有资源限制。
5. **动态库真实 ABI 合同**：测试结构体布局、大小、对齐、版本协商、导出名称唯一、不同前缀多导出、调用参数合法性、Wire 出入的所有基础 Value 类型；不依赖 Python SDK 才可使用。
6. **严格模式 Native 禁止**：`--trust-native-plugin` 在 strict 中必须拒绝。即便将来允许 Worker 在 strict，也要确认 Worker 由严格 Supervisor 派生且继承限制，且没有未受控系统调用权限。

### 9.4 建议测试

- Native grant `fs.read` 返回允许文件真实内容；拒绝未授权路径，审计有 owner 与结果；
- Worker 相同正/反例，带两个不同插件，不允许身份串用；
- 合法句柄在同 run 内可读写其对应资源；猜测 handle、跨 run、过期 generation、被卸载后重用都拒绝；
- 错误/重复 reply id、先发 capability reply、恶意 frame 嵌套与字节预算越界；
- Worker crash/timeout 解释器幸存、进程和管道全部回收；
- 同名 export 冲突必须提前报诊断；签名不匹配在 `check --plugin` 被拒绝，不靠运行时猜测；
- C 代码由另一仓库/独立 fixture 编译，不依赖核心 Rust 内部私有类型；
- 在 Linux、macOS、Windows 各支持平台使用真实动态库而不是只编译测试 Rust crate。

不要求这一版把所有可能的 Host API 都开发出来；只要**承诺可用的 API 确实闭环，其余明确拒绝**，并保持 ABI 不变，就能成为稳定的受限服务子集。

---

## 10. P5：Ubuntu Linux strict OS 隔离实现与安全验收（H3-B / G-C）

> **这是独立安全工程，不是给 `--mode strict` 放开一个布尔值。** 当前 `crates/marqdo-sandbox/src/lib.rs::PlatformCapabilities::detect` 对所有平台返回 `StrictIsolation::Unavailable`，是 ADR-0019 要求的正确 fail-closed 状态。开发任务是实现**真正可强制执行的 Linux 适配器**并通过完整测试，然后只在通过的环境上报告 Available。

### 10.1 Ubuntu namespace 失败究竟意味着什么

此前开发环境的 `unshare`/`bwrap` 返回 `Operation not permitted`。原因未必是缺 root：可能是受限容器禁止创建新 namespace、AppArmor 限制 userns、user namespace 数量为零或 cgroup 未受委派。**不能以一条 `sudo unshare` 成功来证明普通用户 `marqdo run --mode strict` 可以成功。**

建议 Harness 为其选择一台真正可控制的 Ubuntu VM，并留下环境报告：`id`、`uname -a`、`systemd-detect-virt`、AppArmor profile/sysctl、user namespace / net namespace probe、cgroup v2 controllers 和权限。用户允许在该专用开发 VM 内 sudo 做必要配置。遇到 AppArmor 限制，优先为 sandbox launcher 配置有界的专用 AppArmor profile；不要在发布验收中依赖全局关掉保护。Ubuntu AppArmor 已发布的文档中讨论了特定程序 `userns` 权限的正确授权模型。

如果当前宿主实际上是容器而父级拒绝 namespace，Harness 必须：

1. 记录无法完成安全验收的具体 `EPERM` 和运行环境信息；
2. 将安全测试迁到有权限的专用 Ubuntu VM/self-hosted runner；
3. **不要**将 `strict` 从 Unavailable 改为 Available；
4. 继续做不依赖该机制的 G-B 开发；
5. 不能宣称已经完成 G-C，直到真实安全 lane 通过。

### 10.2 最小可行安全架构

```text
          marqdo run doc.mq.md --mode strict
                         │
                  CLI option parsing
                         │
               Strict Supervisor (trusted)
                         │
       ┌─────────────────┼─────────────────────────┐
       │                 │                         │
 runtime OS probe   capability policy         resource budget
       │                 │                         │
       ├─ mount/user/PID/NET namespaces          │
       ├─ filesystem allowlist + readonly roots  │
       ├─ seccomp / no_new_privs / no ambient caps│
       ├─ cgroup v2 or equivalent hard limits    │
       ├─ inherited fd/env whitelist              │
       └─ process-tree supervisor/reaper           │
                         │
               Restricted execution child
                  non-root effective uid
                  interpreter + authorized Host
                         │
                   result + trace
```

可优先使用外部已维护的 Bubblewrap（固定版本范围、运行时能力检测）构造隔离边界，并补齐其未覆盖的 seccomp、cgroup/rlimit、FD、子进程树和错误处理。必要时在 `marqdo-sandbox` 内实现极小的受信 Supervisor，而不是自己重造一个通用 OCI 引擎。**Bubblewrap 只是构造机制，不是自动通过验收的证书。**

`strict` 的 CLI 表面不变；`marqdo-sandbox` 负责：

- `CapabilityProbe`: 检查每项机制是否可获取（非 root 能否启动、本次执行是否可配额）；
- `PolicyBuilder`: 从当前 `--allow-*` 与沙箱模式导出**最小白名单**；
- `Launcher/Supervisor`: 为子程序搭建隔离、设置资源控制、管理文件描述符与进程树、回传结构化错误；
- `SecurityEvidence`: 运行时报告实际启用的机制，而非仅报告内核理论支持。

不要为此在 CLI 增加 `sandbox` 顶层命令，也不要让 Source Parser 自己实现 Linux namespace API。保持 `marqdo-sandbox` 现有职责，直接拓展受控实现。

### 10.3 按三个递增层级交付，不许跨级跳过测试

**L0 — 只验证机制可获得，默认 strict 仍不可用。**

- 在测试 VM 运行 `unshare`/Bubblewrap 基础探测；记录 UID/GID mapping、mount/PID/network namespace、cgroup 控制器权限及可用 AppArmor / seccomp。
- 提供对每项实际机制的错误原因；probe 只能判定环境，不能把“可用 namespace”当作整个 strict 已可用。
- 安全 fixture 和环境日志进入测试 evidence。普通用户无权限时维持不可用。

**L1 — 最小纯语言 strict。**

- 仅允许解释器、内置 Math、无不受控执行能力的 Host 操作。外部 ABI、Foreign Kernel 在本级 `strict` 中明确拒绝，直到它们真正继承隔离约束。
- 项目文件和固定包资源按白名单映射；默认只读；外部 `/etc`、`$HOME`、`/root`、其他项目目录不可读写。允许访问的 runtime/libs 只提供必要最小只读视图；不要把整个主机根目录映射进沙箱。
- 网络默认禁止；出站连接、监听、抽象 Unix socket、外部可达服务均需要检查拒绝能力。不允许利用 `/proc`、`/dev` 或继承文件描述符绕过。
- 新进程默认不允许；如果解释器必须允许某种内部运行机制，则显式限定路径与时机，不能让用户文档间接运行任意 shell。
- CPU、内存、运行时长、最大输出、文件描述符、进程数均有可验证的硬/软边界。实现超时后清理整棵进程树，不能残留子进程。
- 如果任何机制无法应用，必须在真正执行用户代码前失败，不得降级 `project`。`inspect --platform` 报真实原因。

**L2 — 扩展进程的严格继承。**

- 在已经通过 L1 后，允许明确受控的 ABI Worker 或 Foreign Provider 在严格 Supervisor 的**同一隔离域/等价更严格隔离**中运行；它们不允许脱离 sandbox 创建宿主进程。
- 真实 Plugin CapabilityBroker 仍做应用级授权，OS sandbox 额外兜底即使插件调用原生系统 API 也无法访问未授权对象。
- `--trust-native-plugin` 在 strict 下始终不可用，除非未来有彻底不同的可信模型和新 ADR；不要把其编译进同一用户进程。
- 如果某一平台只支持 L1，允许该平台将 strict 报告为**只支持纯语言**，但 CLI 调用 Foreign/Worker 时必须确定拒绝并在支持矩阵注明；不要把它展示成“strict 中可任意运行插件”。

### 10.4 root / sudo 的正确使用

- Harness 可 `sudo apt install`、设置专用 AppArmor profile、创建 VM/systemd 资源测试环境，也可用 root 创建测试隔离设施；
- **最终测试必须从普通用户执行 CLI**，并在隔离子程序中用 `id -u` 或等效行为证明没有宿主 root 权限；
- root-only 的内部 supervisor/helper 必须最小化可执行入口、严格拒绝任意路径与命令注入、降权、设置 no-new-privileges、关掉额外 fd/ambient caps；
- 不把 `sudo` 加进 `marqdo run` 的公开用法，不能要求最终用户全程 root 才能使用语言或 strict；
- 若 VM 无法委派 cgroup 或沙箱工具不支持必要机制，则在该配置上保持 strict unavailable。可新增一个受控 **security-lab** 自托管 CI runner，不以通用 CI runner 的权限缺口拒绝整个 G-B 交付。

### 10.5 安全测试矩阵（全部是运行时反例，不是静态断言）

| 测试族 | 测试方法 | 失败时必须怎样 | 通过标准 |
|---|---|---|---|
| 已授权 project read | 允许目录内已知文件 | 不该因无关路径被拒绝 | 正确读取，记录机制 |
| 未授权文件读取 | `/etc/...`、`$HOME`、其他项目、绝对路径 | 返回拒绝，无数据泄漏 | OS 层直接 syscall 也失败 |
| 写越界 | `../`、绝对路径、符号链接换指向、交换目录的竞争 | 不产生目标文件 | 不出现 TOCTOU 写入 |
| 设备与 proc | 访问 `/proc/*/fd`、宿主 `/dev`、已继承 fd | 不读到宿主敏感对象 | fd/mount 隔离正确 |
| 网络 | IPv4、IPv6、DNS、Unix socket、loopback | 连接建立失败/不越界 | 默认 deny 且可验证 |
| 进程生成 | fork/exec/后台子进程、进程组逃逸 | 拒绝或受限；无遗留子进程 | supervisor 完成清理 |
| CPU 限制 | 无限循环或受控忙循环 | 达预算后被停止 | 不占满宿主直到人工干预 |
| 内存限制 | 大量分配、内存映射、容器构造 | 预算内拒绝/杀死子进程 | 宿主保活 |
| FD / proc 数 | 打开大量 fd、快速创建进程 | 在限额拒绝 | 无资源逃逸 |
| 输出与时间 | 持续大输出/长时间卡死 | 到限后停止并报告 | 不无限增长 log |
| Native ABI | `--trust-native-plugin` + strict | **明确拒绝** | 拒绝发生在加载/执行前 |
| Worker/Foreign | L1 时显式请求运行 | **明确拒绝** | 未产生外部进程 |
| Worker/Foreign | L2 时 OS 直读越权 | OS deny | Broker/Worker 不可绕开 |
| 机制缺失 | 关闭某个测试用资源控制或人为拒绝 userns | strict unavailable | 绝不降级执行 |
| 非 root | 普通用户正常执行 strict 支持子集 | 支持时成功；不支持时拒绝 | 不能只有 root 成功 |

**注意：** 某些 Marqdo 程序本身不具备“直接发 socket / fork”的语法。为了真正验证 OS 边界，测试必须在**同一 Sandbox Launcher / 隔离策略**下运行独立的恶意原生测试 helper；只通过 Marqdo Host 权限拒绝不能证明 Linux sandbox 防 syscall 绕过。

### 10.6 安全 CI 运行方式

建议 `.github/workflows/security-linux.yml`（或现有 workflow 的独立 security job）：

- 指向受控 Ubuntu VM/self-hosted runner，明确 CPU/内存/cgroup/userns/AppArmor 等可用；
- 执行真实隔离子进程与恶意 helper；
- 输出经过脱敏的机制 probe、隔离后的 UID/GID、mount/net/cgroup 状态和每个拒绝用例结果；
- 任何逃逸测试未运行、被跳过、意外成功都使 **G-C 失败**；
- 首次实现时使用 ADR-0020（建议编号，以当前仓库实际序号为准）记录 Linux 机制/威胁模型/依赖/拒绝规则；
- G-C 仅在该平台 lane 完整通过、审查已实施隔离后才能将 `StrictIsolation::Available` 打开。

**如果安全 lane 所需硬件无法获得：** 保持 `strict` 故意不可用，可以发布明确标注“不支持执行不可信代码”的 G-B Core Stable，但**不能以此版本对外宣称 G-C 或 Linux strict 已完成**。若本次 release 明确要求 G-C，则这一事实就是发布阻断，必须找到合适测试 VM 而不是简化门槛。

---

## 11. P6：稳定性与跨平台完整质量门禁（B-14、资源、诊断）

### 11.1 测试层次与证明范围

| 层 | 具体测试 | 不能代替的其它层 |
|---|---|---|
| 单元 | Parser、Value、Path/Manifest、SemVer、Broker 等 | 不能证明 CLI 子进程行为 |
| 金样 | Markdown AST、语义执行、负例 | 不等于跨平台 Byte-for-byte 一致 |
| CLI 黑盒 | run/call/check/add/sync/tool、参数、输出、退出码 | 不等于独立协议消费者可用 |
| 协议契约 | 真正产出 JSON、Schema+负例、跨语言消费者 | 不等于可安全运行恶意程序 |
| ABI integration | C lib、Native+Worker、多 export 与权限 | 不等于进程 OS 沙箱 |
| Foreign integration | 固定 provider/env、双版本解释器、超时 | 不等于 lock v3 包一致性 |
| 项目环境 | 多版本依赖、完整传递图、Git 缓存、物化 | 不等于公网上 registry 可用 |
| 压测/性质 | 流水线取消、无限循环预算、深递归、超大消息 | 不等于操作系统逃逸测试 |
| Security G-C | 真恶意 helper、网络/FS/进程/资源越权 | 不能靠 root 运行得到可信结论 |
| Release smoke | 安装打包产物、运行样例、验证 sha256 | 不等于源树 `cargo test` |

### 11.2 Rust 品质要求

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test -p marqdo-cli
cargo build --release --locked -p marqdo-cli
```

加固建议按可靠性排序：

1. **恐慌/越界**：未经信任的 `.mq.md`、JSON、IPC 帧和 lock 数据不得引起非预期宿主 panic 或栈溢出。对深度递归、庞大嵌套 List、无限 while、异常大值和跨模块错误增加限定性测试。已有 fuel/stack/output 预算要验证覆盖真实路径。
2. **线程与进程回收**：长时间运行 Worker、Foreign，故意杀死进程/关闭 Pipe；无僵尸、fd 泄漏、永不结束的等待线程。
3. **回滚与完整性**：Tool 安装与删除、Package sync、Analysis/Trace 写文件必须原子或能清楚失败，不能留下看起来合法但实际过期的产物。
4. **覆盖率/性质测试**：适当采用 proptest/cargo-fuzz 对 Markdown/表达式解析、JSON/Wire、lock v3 与路径处理，重点是**不崩溃、不失去确定性、不产生未授权 effect**；在 CI 无法负担时可建独立定时/手动 lane，而不是删掉。
5. **依赖审查**：保留 `Cargo.lock`、依赖版本固定与许可证审查；建议加入供应链漏洞扫描，但优先保证正确构建、CI 与发行资产完整。
6. **诊断**：用户错误必须有稳定 code、真实模块、可读提示。任何异常不能错误标注成入口文件或“已成功执行”。

### 11.3 平台支持矩阵必须真实填写

| 功能 | Ubuntu x86_64 | macOS arm64 | Windows x86_64 | 备注 |
|---|---|---|---|---|
| Markdown parser / Rust runtime | 必须 PASS | 发布平台则必须 PASS | 发布平台则必须 PASS | 语义金样严禁随意 regenerate |
| check/Analysis v2/Trace v2 | 必须 PASS | 必须 PASS | 必须 PASS | 独立消费者验证 |
| Tool Protocol v1 | 必须 PASS | 必须 PASS | 必须 PASS | 不依赖 sh，已有便携 Rust fixture |
| lock v3 path/Git | 必须 PASS | 发布平台则必须 PASS | 发布平台则必须 PASS | 不以 `#[cfg(unix)]` 空跳过计为 PASS |
| ABI Native & Worker | 必须 PASS | 要么有目标动态库 PASS，要么标 unsupported | 同左 | ABI 支持标签必须如实 |
| Foreign provider/env | 必须 PASS | 发布平台有对应测试才声称支持 | 同左 | 解释器与环境路径在不同 OS 不同 |
| strict | 需要 G-C 才宣称支持 | 默认 fail-closed | 默认 fail-closed | 不依赖其它平台同日实现 |

对 macOS/Windows 如果仅 Core Stable 被支持，发布说明必须明确 ABI/Foreign/strict 的细分状态。**不能因为 Linux 上可以运行，就无条件上传 macOS/Windows 发行二进制并宣称同等能力。**

### 11.4 发行包必须独立于源码工作树测试

正式发布时使用构建好的 `target/release/marqdo`、必要的发行文档/许可/完整性文件，在**干净目录**安装后执行：

- `marqdo --version`、`marqdo --help`；
- `marqdo run examples/hello.mq.md`；
- `marqdo call examples/factorial.mq.md 阶乘 --arg n=5`；
- `marqdo check examples/... --emit analysis.json`，独立消费者读；
- `marqdo run ... --trace-file trace.json`，校验摘要关联；
- `marqdo tool install` 一个随机名称工具，执行、删除、故意篡改拒绝；
- lock v3 项目 `marqdo sync --locked`，可重复物化并重跑；
- 本平台承诺的 Foreign 与 ABI 真实调用；
- Linux 如果发布说明宣称 strict，则在安全 runner 执行完整 G-C；未宣称 strict 则验证 fail-closed。

打包与文件布局必须与其发布安装说明一致；不可要求用户检出整个 workspace 才能运行基础示例。

---

## 12. P7：三仓独立消费的发布前演练（不在 CLI 中实现三仓）

### 12.1 独立 Tool 无需核心适配

用**三个第三方模拟工具**而不是在 CLI 中加入 `knowledge/prove/view` 固定枚举：

1. `sample-view`：读取 `.mq.md` 源码与 Analysis v2 / Trace v2，打印函数数量、资源类型、调用来源行号。不得执行 `check` 时的项目代码。
2. `sample-knowledge`：读取 Analysis v2，输出简单稳定索引（function id、resource id、source digest、引用边），不引入向量数据库。
3. `sample-proof`：仅读取 Analysis v2，挑选受支持的纯整数表达式节点并给出“可解析/不支持/未知效应”的保守分类；**不宣称 Lean 已证明程序**。

上述程序可以写为小型独立 Python/Go/Rust 测试 fixture；只作为 `tests/consumers` 或 `examples/` 的黑盒验证，不必创建新的发行仓库。

### 12.2 目标验收

- 三者都通过 Tool Protocol 使用合法随机名称安装；
- 三者消费同一份 Analysis v2 / Trace v2，不链接任何私有 `marqdo-*` crate；
- 相同源码和 lock 在不同目录的知识身份稳定；源码变化使关联证据 stale；
- 第三方 Tool 可以替换编程语言、升级版本、卸载，`marqdo-cli` 无需重新编译；
- 未支持的 AST node、未解决的引用、Effect unknown 显式标注，不虚构证明或执行结果；
- CI 真实编译/运行（Python fixture 需要安装 Python，不允许缺 Python 就假装已验证）。

该演练一旦通过，说明 Core 已有合格的**生态扩展边界**。不要趁机把示例工具发展为三个产品的正式实现。

---

## 13. P8：发布工程、文档一致性、兼容承诺与版本发布

### 13.1 修复已经可见的文档漂移

当前 `docs/STATUS.md` 顶部仍称 `M0.5`，但正文已描述 lock v3、Analysis v2、Trace v2 和新 CI；`spec/tool-v1.md` 可能残存 Proposed 语句；`spec/security.md` 的部分陈述仍在描述旧 Worker 不可用/单前缀 export 等历史限制。这些都容易误导新开发者。

**必须做的文档清理：**

- `docs/STATUS.md`：更新 HEAD SHA、真实 CI #44680378977 的 Linux PASS / macOS+Windows FAIL，随后每次修复后更新新 CI URL。将历史 snapshot 移到明确的“历史”章节，避免整篇以 M0.5 状态作为现状。
- `docs/CORE_STABILIZATION_AND_ECOSYSTEM_GATE.md`：保留为阶段性历史设计；注明其 `0adebe4` 基线已被当前稳定化规范超越，不能把旧未完成清单直接用于新代码。
- `docs/NEXT_PHASE_IMPLEMENTATION.md` 与早期 ADR：保留历史决策，不复制多个互相冲突的“最新总体计划”；本文件成为新增任务唯一总纲。
- `spec/analysis-v2.md`、`spec/trace-v2.md`、`spec/tool-v1.md`：用真实当前输出对齐字段、状态、版本、兼容规则和异常。
- `spec/security.md`、`docs/adr/0019`：明确 Worker 能做/不能做什么、H3-A 现有 `fs.read` 实际桥、strict 未启用的原因和未来 G-C 机制；**不要假定 `Landlock-only` 或 `Worker-only` 足够**。
- `README.md`、`AGENTS.md`、`docs/contributor-guide.md`：添加稳定支持矩阵与本文件入口；新开发者不会读聊天记录也能重现代码和 release gates。

### 13.2 正式发布资料要求

1. `CHANGELOG.md`：清楚列出语言兼容、包 lock v3、Analysis/Trace v2、Tool v1、ABI v1、Foreign 支持与安全限制。
2. `RELEASE.md` 或 `docs/releasing.md`：列出 tag、平台、编译工具链、资产上传、校验和、撤回/回滚方法及验证流程。
3. `COMPATIBILITY.md` 或独立章节：公开协议主版本、支持平台、最低环境依赖、严格沙箱可用条件、降级与拒绝规则。
4. 示例/Quickstart：干净 Ubuntu 用户无需安装 Knowledge/Proof/View 即可 `run/call/check/add/sync/tool`；文档准确描述 Python/Node 是否自动安装。
5. `LICENSE`、公开 C header/SDK、插件示例与 API ABI 版本，确保外部独立开发者能够合法、明确地接入。
6. Release binaries：至少 Ubuntu 官方目标平台真实 `--release --locked` 构建产物；每资产 SHA-256，建议附 SBOM/构建 provenance。源码 tag 与二进制必须对应同一 SHA。
7. 安装体验：新环境能够校验二进制摘要并执行金样；不要求用户有 root 来执行普通 Marqdo 文档。

### 13.3 稳定发行步骤

```bash
# 所有操作应在完成适用 gate 后运行；以下为建议的发布序列

git fetch origin --tags
git status --porcelain=v1
git rev-parse HEAD
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release --locked -p marqdo-cli

# 由 release 流程按真实产物路径创建归档、SHA256 与 provenance
# 最终 tag/release 的具体版本号由兼容政策及全绿 CI 决定
```

**不能直接运行 `git tag v1.0.0` 并宣称完成。** 必须先在远程提交上获得必需 CI 通过、至少一次 release candidate 干净安装演练、实际发行产物验证及正式发布说明。

### 13.4 失败处理和回滚

- 某 CI 平台失败：不删测试；形成最小复现、修复、重新运行；如果是明确不支持平台/功能，把该项从发布支持矩阵移除，文档写清楚，不假称同等能力。
- G-C 不可通过：strict 保持 unavailable；可在修订版本范围并经用户确认后单独发布 G-B Core Stable，但不宣传不可信代码隔离。若当前版本目标承诺 Linux strict，则必须推迟该版本发布直至安全门禁通过。
- 发布资产校验失败：撤回资产或撤销 release candidate，而不是让用户忽略 checksum。
- 正式 API 不兼容：升级版本及提供迁移说明；不要悄悄重写 schema v2 的已有字段含义。

---

## 14. 任务顺序、PR 切片、依赖关系与回滚策略

为避免 Harness 一次铺开全栈、难以调试，**一个 PR 只处理一个清晰风险面**。按以下顺序推进；必要时可并行不相互依赖的测试工作，但**同一 PR 不要同时修改 Markdown 语义和安全模块**。

| 顺序 | PR / 工作包 | 前置 | 修改范围建议 | 核心验收 | 若失败 |
|---|---|---|---|---|---|
| 0 | `ci/windows-ast-line-endings` | 无 | `.gitattributes`、AST golden harness、fixture | Windows AST + Linux golden PASS | 不改变 profile，重现字节差异 |
| 1 | `ci/macos-native-plugin-fixtures` | 无 | `examples/plugin-*`、CLI integration、CI | macOS 真实适配 / 正确跳过未支持目标 | 不能以 Linux.so 冒充 macOS artifact |
| 2 | `docs/status-reconcile` | 0/1 的结果 | STATUS/security/spec/README | 当前 SHA/CI/平台状态真实 | 标明未验证，不预填 PASS |
| 3 | `package/repro-hardening` | 基线 CI 绿 | package/fileset/lock/sync/tests | 多版本/传递/Git/env 原子完整 | 旧语义不正确则新增 ADR |
| 4 | `protocol/consumer-hardening` | 3 或独立 | Analysis/Trace/Tool/Schema | 移目录/digest/stale/陌生 Tool PASS | 只更改公开协议时升级版本 |
| 5 | `foreign/pinned-environment` | 3/4 | Foreign/env descriptor/tests | 确定真实解释器与环境绑定 | 不内置 uv/npm 大生态 |
| 6 | `abi/session-broker-hardening` | 0/1 | Host/ABI/Worker/tests | 跨调用权限隔离、恶意 frame/超时 | 不修改既有 ABI layout |
| 7 | `security/linux-strict-probe` | 独立 VM 准备 | sandbox/ADR/security tests | 能力检测，不开放 strict | EPERM 记录并换 VM |
| 8 | `security/linux-strict-pure` | 7 | supervisor/sandbox/CLI | L1 所有 OS 逃逸负例 PASS | 失败则 strict 保持 Unavailable |
| 9 | `security/linux-strict-extensions` | 8、6 | sandbox/Worker/Foreign | L2 支持路径真实继承限制；不支持的明确拒绝 | 不得单纯相信 Worker |
| 10 | `ci/native-platform-matrix` | 1/5/6 | CI/fixtures/portability | 各宣称支持平台真实运行 PASS | 清晰标 unsupported |
| 11 | `ecosystem/consumer-smoke` | 4 | tests/consumers / Tool fixtures | 独立三类工具可读可用 | Core 不增加特殊 Tool 名称 |
| 12 | `release/rc-and-docs` | 0–11 适用项 | docs/workflow/version/assets | 全绿 CI + 安装演练 + SHA | 禁止给红 CI 打 stable tag |

**并行原则：** 修 Windows AST、macOS fixture 可并行；包图与协议可以由不同分支实现，但统一落盘/身份规则的冲突由 ADR 先解决。严格沙箱在单独安全 VM 开发，不阻塞纯协议测试。分支合并前用合并后的 SHA 重跑所有相交测试。

### 14.1 Harness 每个 PR 的最小输出模板

```text
PR title:
Baseline SHA:
Changed files:
Observed defect (link to file/function/test):
Why the prior design fails:
Chosen solution and rejected alternatives:
Compatibility impact (language/ABI/schema/CLI):
Red test before fix:
Green tests after fix (commands, platform, exit code):
Security/negative cases:
Documentation updated:
Commit SHA:
CI URL and exact conclusions:
Remaining blockers:
```

不接受只有“完成修复，测试通过”却没有具体测试证据的 PR 说明。

---

## 15. 完整测试命令与验收样例

以下命令是现有工具链中应保持的官方入口；个别测试文件名或 fixture 参数在后续 PR 改动时，开发者应先核对 `crates/cli/tests/` 和 `Cargo.toml`，**不能假装命令已存在或运行成功**。

### 15.1 Git 和工作区

```bash
git status -sb
git fetch origin --tags
git rev-parse HEAD
git rev-parse origin/master
git log --oneline --decorate -10
git log --oneline origin/master..HEAD   # 仍未推送的本地提交
git diff --stat; git diff --check
```

不要在未保存用户现有修改时直接 `git reset --hard`、`git clean -fdx`、强推主分支或覆盖测试基准。若工作区有未提交内容，使用单独分支/worktree，保留未提交文件的原始差异。

### 15.2 核心质量

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test -p marqdo-cli --test conformance_ast
cargo test -p marqdo-cli --test conformance_semantics
cargo test -p marqdo-cli --test negative
cargo test -p marqdo-cli --test properties
cargo test -p marqdo-cli --test analysis_trace
cargo test -p marqdo-cli --test tool_protocol
cargo test -p marqdo-cli --test package_env
cargo test -p marqdo-cli --test plugin_worker
cargo test -p marqdo-cli --test integration_cli
cargo build --release --locked -p marqdo-cli
```

### 15.3 公共数据协议的端到端示例

```bash
cargo build -p marqdo-cli

./target/debug/marqdo check \
  examples/multi-module/index.mq.md --refs --emit /tmp/analysis-v2.json

./target/debug/marqdo run \
  examples/multi-module/index.mq.md --trace-file /tmp/trace-v2.json

python3 tests/consumers/analysis_consumer.py /tmp/analysis-v2.json
python3 tests/consumers/trace_consumer.py \
  /tmp/analysis-v2.json /tmp/trace-v2.json

# 此脚本使用 jsonschema 包；先安装明确版本
python3 tests/consumers/validate_protocols.py \
  /tmp/analysis-v2.json /tmp/trace-v2.json \
  examples/tool-unknown-name/tool.json
```

不得为这些命令运行成功而调整 fixture 使其失去负例覆盖；如果 CLI 输出协议破坏消费者，则应检查是哪一侧错误并遵守版本政策。

### 15.4 插件、Foreign、环境与 Tool

```bash
# 参考当前代码中的真实参数/样例路径；Windows/macOS 需适配动态库
./target/debug/marqdo check examples/plugin-multi-export/plugin-call.mq.md \
  --plugin examples/plugin-multi-export

./target/debug/marqdo run examples/kernel-mock/demo.mq.md \
  --kernel examples/kernel-mock/kernel.json --allow-foreign

# 隔离 Tool 测试安装目录，避免影响主机用户注册表
export MARQDO_TOOL_HOME="$(mktemp -d)"
./target/debug/marqdo tool install examples/tool-unknown-name/tool.json
./target/debug/marqdo benchmark run .
./target/debug/marqdo tool list
./target/debug/marqdo tool remove benchmark
```

**重要：** 当前 `examples/plugin-demo/build.sh` 只适用于 Linux 开发示例且会改写示例 manifest。正式 CI 应通过临时 fixture 构建本平台动态库，不在运行测试时写回受版本控制的 manifest。

### 15.5 strict 的普通用户拒绝 / 安全 lane

```bash
# 当前尚未实现严格隔离时，退出非零是预期、正确行为
./target/debug/marqdo inspect --platform --json
./target/debug/marqdo run examples/hello.mq.md --mode strict

# 未来真正的 security lane 必须运行第 10.5 节所有真实 OS 负例
# 不能用这两条“拒绝测试”证明 strict 已支持
```

### 15.6 失败时的最短定位路线

1. `git rev-parse HEAD` 与 CI 失败 SHA 一致吗？
2. 故障仅 Windows/macOS，还是 Ubuntu 也重现？
3. 测试在比较字节、真实行为、协议 schema 还是构建目标？
4. 当前 shell 是否 root/容器/受限制的 AppArmor profile？
5. CLI 输入是被锁定版本、静态引用、显式插件 manifest 还是未知动态目标？
6. 如何写出最小、稳定、无网络的复现 fixture？
7. 改动是否会让此前拒绝路径变成默许执行？
8. 修复后被同一目录、另一目录、不同 OS、恶意输入测试覆盖了吗？
9. PR 是否记录了明确的版本/安全/兼容影响？

任何一项不清楚，就先做最小复现而不是提交大量“可能有帮助”的重构。

---

## 16. 质量门禁与可签署发布清单

下面每一项必须以 **PASS + 真实 SHA/CI URL/日志** 或 **EXPLICITLY UNSUPPORTED（不宣称支持）** 两类状态之一回答。除平台明确不承诺的可选能力外，核心必需项不得标为 UNSUPPORTED。

### 16.1 Core Stable / G-B 必要门禁

- [ ] GitHub 当前目标 SHA 的 Linux `fmt/clippy/cargo test/workspace/smoke` 全部通过。
- [ ] Windows AST golden 根因查明，受支持平台的语义 profile 一致；没有靠 `UPDATE_GOLDENS=1` 淹没问题。
- [ ] macOS 插件测试不再依赖 Linux artifact；支持的平台真正运行 Native 插件集成。
- [ ] Windows/macOS 所承诺的语言、包、协议、Tool 功能真正运行测试，结果不是 `cargo check` 或空跳过。
- [ ] 全部路径/Git/传递锁图 v3 测试通过；锁失败严格拒绝，不下载或重写。
- [ ] 完整包内容 hash、symlink/特殊文件/权限/缓存篡改测试通过。
- [ ] Analysis v2 逻辑身份与资源投影跨目录可移植，真实 Schema 和独立消费者 PASS。
- [ ] Trace v2 的跨模块事件归属、digest/stale、截断与隐私测试 PASS。
- [ ] Tool v1 的任意名称、SemVer/Analysis/Trace 兼容、整包 hash/路径逃逸、退出码、卸载原子性 PASS。
- [ ] 外部 C ABI SDK 编译成功，静态检查/多导出/ABI mismatch/Worker 失败均有回归。
- [ ] Host 能力代理已承诺的 `fs.read` 和其他**实际声明可用**服务通过真实授权/拒绝测试；不可用能力不假装成功。
- [ ] Foreign Provider 摘要、固定环境、真实解释器选择/版本、超时/输出预算/非法 wire 校验通过。
- [ ] `check`、`inspect`、Analysis 导出证明不执行任何 `.mq.md` / Foreign / Native 插件副作用。
- [ ] 代码/协议规范/STATUS/README/AGENTS/示例和 CI 支持矩阵互相一致。
- [ ] 三种独立 Tool 消费者可运行，不需要 CLI 为官方名称写特殊注册分支。
- [ ] 对外 ABI / Tool / Analysis / Trace / lock 版本与兼容、迁移拒绝规则已明确冻结。
- [ ] 在真正干净的系统或 VM 中安装 release artifact、验证 SHA-256、运行 smoke PASS。
- [ ] Tag 对应同一验证过的 HEAD；CHANGELOG、发行说明、兼容矩阵和回滚步骤齐全。

### 16.2 Linux Strict / G-C 独立必要门禁

- [ ] 测试在独立受控 Ubuntu VM 或受信 self-hosted runner 上真实运行。
- [ ] 普通用户可用性/机制 probe 结果明确；不会靠根权限假装普通用户可用。
- [ ] 严格 Supervisor 正确隔离文件系统、网络、进程/FD 和资源上限。
- [ ] 直接 syscall 恶意 helper 的逃逸负例全部执行且拒绝成功。
- [ ] 子进程及其子树被可靠回收，time/memory/CPU/FD/process/output 均有真实预算测试。
- [ ] Native ABI 在 strict 拒绝；Worker/Foreign 只能在真正继承 OS 约束时启用。
- [ ] 任一机制不可用时继续 fail-closed，无隐式 `project` 回退。
- [ ] 安全 ADR、威胁模型、平台能力和逃逸测试均有对应 commit + security CI URL。
- [ ] 对外严格描述实际支持范围，不将 Worker、Landlock-only 或有 `sudo` 误认为完整沙箱。

### 16.3 最终发布判定

```text
Release candidate:
Source SHA:
Target version/tag:
Linux CI URL / status:
Windows CI URL / status:
macOS CI URL / status:
Protocol consumer CI URL / status:
Security G-C CI URL / status:
Package/Tool/ABI/Foreign integration summary:
Release artifact SHA-256:
Clean-install smoke report:
Supported platforms / explicitly unsupported features:
Known non-blocking limitations:
Release notes and compatibility link:
Final decision: RELEASE / DO NOT RELEASE
Reason:
```

**任何缺少真实来源的 PASS 都要降为 UNVERIFIED；不能把“代码已写完”“本地 sudo 下通过”填写为发布验收通过。**

---

## 17. 可直接交给 DeepSeek Harness 的总任务指令

> 下列内容可以复制到 DeepSeek Harness 的代码任务会话中。它的职责是执行本文件，并在有权限、资源和代码访问的条件下持续推进到发布门槛，而不是仅提出下一份设计方案。若用户尚未授权向远程发布 tag/release，可先准备并验证 RC，待明确发布授权后执行最终动作。

```text
你是 cflmy/marqdo-cli 的资深 Rust / Linux 系统安全 / CI 工程负责人。

仓库内的 docs/STABLE_RELEASE_IMPLEMENTATION.md 是本次完整的实施规范。
请先完整阅读该文档、AGENTS.md、docs/STATUS.md、spec/ 与 ADR-0008 至 0019，
再对照当前真实源码与 GitHub CI。不要依赖任何此前的聊天记录，也不要假定
旧 cflmy/marqdo 仓库的代码应被迁移；这个 CLI 是独立 Rust 实现。

最终目标：在保留 Markdown-native 语言及轻量 CLI 架构的前提下，完成该文档
定义的所有适用稳定版门禁 G-B，并在可控 Ubuntu 安全 VM 上争取完成 G-C，
形成发布候选版本、真实发行资产、测试证据、兼容矩阵与发布说明。

允许在本机使用明确授权的 sudo 安装系统工具、配置专用 AppArmor / namespace /
cgroup 测试环境；但普通 build 和正式测试不可总是 root，被测不可信程序必须
在真实 OS 隔离内以低权限运行。sudo 无法解除上级容器限制时，应报告证据并
转移安全测试到合适 Ubuntu VM，不得关掉 strict 的 fail-closed 来凑通过。

执行要求：
1. 基于最新 SHA 先修现有 CI：Windows AST golden 换行/字节差异和 macOS
   平台不匹配的插件 artifact。保留失败复现和正反测试，不允许无差别重生 goldens。
2. 复核 lock v3 全图、Git cache、内容 hash/环境发布的可复现性质。
3. 收尾公开 Analysis v2、Trace v2、Tool v1，包括跨仓独立消费者的黑盒测试。
4. 完成 Foreign provider 与固定环境/真实解释器绑定的完整性语义；任何自动
   uv/npm 环境创建应在独立明确授权的工具中完成，不要写死进解释器。
5. 完成 ABI Native/Worker/Host Broker 的真实调用和会话边界测试。
6. 实现 Linux strict 的真实 OS 隔离：先机制 probe、再纯语言模式，再按测试门禁
   扩展 Worker/Foreign；文件/网络/进程/资源逃逸负例必须真的执行。
7. 加强 CI 平台矩阵与 release job；使实际支持矩阵与测试结果一致。
8. 完成文档和 STATUS 清理、发行资产、SHA256、干净安装 smoke、release candidate。

不可做：
- 不新增不必要的 CLI 顶层命令；不把 Knowledge/Proof/View 代码放入核心；
- 不引入第二个解释器、通用注册表、JIT、IDE、CAS、Lean 等本轮不必要功能；
- 不为官方 Tool 做特殊接入分支；不让插件篡改 Markdown 语言语义；
- 不以 root 下单次成功、跳过测试、忽略 CI 失败或静态权限检查代替隔离验证；
- 不强推主分支、不删除用户未提交工作、不在未得到发布授权时擅自给正式版打 tag。

工作方式：
- 分小 PR / commits；每项先做失败复现测试，再最小修复，再完整回归。
- 允许破坏性重构尚未稳定的内部 API，但公开 schema/ABI 变更须 ADR 和版本迁移。
- 若有必须补充的核心正确性缺陷，不因它没写在本文件就跳过；但先证明影响
  和复现，再进行最小必要修复。
- 每阶段汇报真实修改、通过和失败的测试、日志、SHA、CI URL、剩余阻断。
- 更新 docs/STATUS.md，不能保留与最新源码矛盾的“已完成/未完成”描述。
- 在每阶段现有门禁未绿前不要直接声称项目完成。

从仓库基线审计和现有 CI 失败复现开始，随后按文档第 14 节推进。
目标是实现并验证真正可稳定发布的结果，而不是再写一份规划。
```

### 17.1 对 Harness 的交互指引

开始时只需对它下达上述总任务，并指明本文件路径。它可以从小阶段开始编码、测试、提交。若 Harness 在一个会话里难以完整完成所有任务，下一次会话应让它读取 `docs/STATUS.md`、最新 CI、`git log` 和第 16 节验收表继续，不需要重述我们的历史对话。**同一份文档是项目唯一的任务解释权威，测试和代码是事实权威。**

当它问到 sudo：允许它按需为**专用开发/测试 VM** 提权，要求说明目的和将运行的命令；你可以批准该系统操作。若它请求让所有生产 CLI 和测试都以 root 常态运行，应拒绝这种设计，因为这不能验证最终普通用户体验。

当它问到 G-C 和 Linux namespace：先给它当前 Ubuntu 诊断输出。若无法访问安全测试 VM，可继续 G-B，不要为了完成一行状态清单冒险修改 `PlatformCapabilities::detect()` 返回值。

当它问到 uv/npm：不要把 package manager 直接嵌入解释器；按第 8 章做固定 Provider/环境验证与可选外部 provisioner。如确有必须进 core 的跨平台依赖/协议需要，先以独立消费者用例证明，避免重现过去的内核膨胀。

当它问到发版：以第 16 节完整签署验收为准。对 stable release 名称和是否包含 G-C，必须与公开支持声明一致。用户可单独授权发布 tag/GitHub release；在此之前先完成全部工程内容和 RC 演练。

---

## 18. 与历史文件的关系、引用和保留策略

本文件是新阶段总实施计划，不取代规范里已经确定的具体语言语义：

- 语言权威：`spec/markdown-profile.md`、`spec/mapping.md`、`spec/expression.ebnf`、`spec/evaluation.md`、`spec/references.md`。
- Package：`spec/package-format.md`、ADR-0016。
- Public API：`spec/analysis-v2.md`、`spec/trace-v2.md`、`spec/tool-v1.md`、`spec/plugin-abi.md`、`spec/foreign.md`。
- Security：`spec/security.md`、ADR-0004、ADR-0011、ADR-0018、ADR-0019；strict 新实现须有独立新 ADR。
- 项目历史：`docs/DEVELOPMENT_BLUEPRINT.md`、`docs/NEXT_PHASE_IMPLEMENTATION.md`、`docs/CORE_STABILIZATION_AND_ECOSYSTEM_GATE.md` 可保留，但涉及阶段优先级和旧 HEAD 的事实时以当前源码及本文件任务规范为准。

供开发者查阅的参考链接：

- 仓库： https://github.com/cflmy/marqdo-cli
- 本次基线 CI： https://github.com/cflmy/marqdo-cli/actions/runs/44680378977
- Ubuntu 24.04 AppArmor/userns： https://documentation.ubuntu.com/release-notes/24.04/
- Ubuntu AppArmor 安全配置： https://documentation.ubuntu.com/security/security-features/privilege-restriction/apparmor/
- Bubblewrap： https://github.com/containers/bubblewrap
- DeepSeek Harness： https://www.deepseek.com/harness/en/
- DeepSeek Harness 开发文档： https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/development.md

---

## 19. 作为开发负责人的最终工程判断

Marqdo CLI 的核心扩展架构**已经基本建立，不能把“没有 G-C”误解为“还需要重新设计一套 CLI”**。真正需要完结的是：

1. 当前 CI 明确可复现的两个平台问题；
2. 包与协议的稳定性和边界测试，不仅仅是功能演示；
3. Foreign 固定环境要约束真实执行目标，ABI 要有实际授权/错误与会话保证；
4. 在可控 Ubuntu VM 上实现并验证严格隔离，若无法取得足够 OS 权限，诚实保留 fail-closed；
5. 实际发行产物、兼容矩阵、独立工具消费者与全绿 CI 的真实证据。

**没有任何一项要求新增 `marqdo knowledge` / `marqdo prove` / `marqdo view` 的内置分支。** 如果未来某个扩展需求一定要修改 CLI 源码才能接入第三方命令、普通 Foreign kernel 或普通 ABI export，应首先视为扩展协议的架构缺陷，再决定最小修正。

**完成定义：** 当第 16.1 节所有适用 G-B 门禁均具备可审计的 PASS，首个核心稳定版本可以发行；当第 16.2 节 G-C 也确实通过时，才可以进一步对所测的 Linux 环境宣称支持 strict。切忌把“我们愿意给 Harness root”写成 OS 安全保证，也切忌因追求全功能而再次膨胀轻量内核。
