# RFC-CI-0001：Marqdo CLI 最新提交缺口审查、Linux Strict 验证基础设施与跨平台/性能发布门禁

> **状态：Proposed / 工程实施指引（未执行代码修改）**  
> 仓库：`https://github.com/cflmy/marqdo-cli`  
> 审查冻结提交：`32d8fe1f7afcdd7d7316c6692c442f5b26a24db2`（`master`，2026-11-21）  
> 审查依据：该 SHA 的 GitHub Actions 运行 `44883104060`、各 Job 日志、源码、`docs/STATUS.md`、安全 VM 文档、CI 配置及性能脚本。  
> **发布结论：DO NOT RELEASE。**  
> 本文区分 **源码确认**、**该 SHA 远程实测**、**旧 SHA 记录** 和 **建议待实施**。本文件不宣称已在 Linux VM 上重新测试，不声称 Windows/macOS 已修复，不改动 GitHub 仓库。

## 0. 第一次阅读者应先知道的事情

Marqdo CLI 是 Rust 工作区实现的 Markdown 原生编程语言。当前 `.mq.md` 已经破坏性迁移到统一语义：多级标题可声明 `@exec` / `@knowledge`；仅 Exec H2 可调用；Strong/Emph/CodeSpan 承载相同的一条程序语句；载荷中的 `if/else/while/end/return/params` 决定程序语义；普通列表、引用块不再强制代表控制流。**这次语言语义在 Linux 本地已有 `9c1723d` 的 507 个通过测试记录，不等于最新 SHA 的跨平台与隔离安全验收。**

仓库还具有三个相关但独立的机制：

1. `pure/project/strict`：执行权限模式。`strict` L1 限定纯 Marqdo、原生数学与经授权的 stdout；不允许外部文件、网络、Foreign、Plugin、Trace 等扩权。
2. `marqdo-sandbox`：Linux 上用 Bubblewrap 构造用户、挂载、PID、网络等命名空间，带内存/进程/CPU/输出限制；在环境不可用时 **fail closed**。Mac/Windows 不提供该 strict 隔离。
3. `.github/workflows/ci.yml`：GitHub-hosted Linux + Windows + macOS 测试与打包；`.github/workflows/security-linux.yml`：需要具备命名空间能力的专用自托管 Linux VM 安全测试；`.woodpecker.yml`：Linux Docker 常规开发测试。**Linux 容器通过 Cargo 测试，不等于 Linux strict 隔离通过。**

本文目标：修复当前实际阻断，可靠地在 Linux 运行/编排严格隔离测试，取得同 SHA 的各平台证据，建立不被不稳定主机噪声污染的性能回归与发布体系。**本方案不修改语言语法，不迁出仓库标准库，不重新发明 Sandbox。**

## 1. 最新 SHA 的证据：不是简单的“UNVERIFIED”

### 1.1 该提交对应的公共 CI

- GitHub Actions：`https://github.com/cflmy/marqdo-cli/actions/runs/44883104060`
- 执行时间：2026-11-21 08:03–08:05 UTC，`head_sha=32d8fe1...`。
- `schema instance validation + consumers`：**PASS**。
- `fmt + clippy + test (Linux x86_64)`：**FAIL**，fmt 与 clippy 通过，`cargo test --workspace` 中 `strict_mode_enforces_isolation_or_refuses_closed` 失败。报错为 `stdout is not a single JSON document: EOF while parsing a value at line 1 column 0`，即在 strict `--json` 失败路径读到空 stdout。
- `runtime contract (windows-latest)`：**FAIL / compile E0425**：`crates/cli/src/commands/run.rs:67,70` 调用了只在 Linux 编译的 `marqdo_sandbox::marker_present`、`verify_active`。
- `runtime contract (macos-latest)`：**FAIL / compile E0425**：同上。
- `release candidate artifact`：**SKIPPED**，因此无本 SHA 的候选包验证。
- `Linux strict-security (G-C)` disposable VM：**本 SHA 无远程通过记录**。历史 VM 在旧源码 `70d916d` / `8af7262` 有 7/7 对抗案例证据，不可沿用为本次提交验收。

**重要区别：** Windows、macOS 在本 SHA 上“已经运行但失败”，应标为 `FAIL`；strict VM 未运行，才标为 `UNVERIFIED`。对 Linux GitHub-hosted 普通 runner 的失败，不能通过直接重跑旧 VM 结果消除；必须修复 CLI JSON 协议的失败路径。

### 1.2 精确定位与原因

| ID | 代码/证据位置 | 已确认问题 | 后果 | 优先级 |
|---|---|---|---|---|
| B01 | `crates/cli/src/commands/run.rs:67/70`；`crates/marqdo-sandbox/src/lib.rs:15–16` | CLI 直接引用 `#[cfg(target_os="linux")]` 导出的函数 | Mac/Windows 无法编译 | **P0** |
| B02 | `crates/cli/src/commands.rs::launch_strict_process`；`crates/cli/tests/integration_cli.rs` | strict 监督器/启动失败与 JSON 输出路径没有统一规范；非零退出或 `Err` 可能只写 stderr | `--json` 消费端收到空文档，本 SHA Linux CI 失败 | **P0** |
| B03 | `strict_is_available` 与实际 `run` 的独立探测 | probe 与 launch 有时间窗口；在资源波动主机上条件可能变化 | 测试易依赖时序，容易把可用性与拒绝行为混为一谈 | **P0** |
| B04 | `.github/workflows/security-linux.yml` | 只有手动和每周计划触发，依赖 `self-hosted,linux,marqdo-security` 标签 | 未形成每个候选 SHA 自动且可信的 G-C 证据 | **P0** |
| B05 | `.github/workflows/ci.yml` 和 `docs/STATUS.md` | Windows/macOS 已存在 GitHub-hosted 矩阵，但部分文档仍描述未配置 runner | 文档状态与实际仓库不一致，误导发布判断 | **P1** |
| B06 | `docs/architecture/CURRENT.md` 第 3 节接口表 | 新的 fs request/DNS/TLS 等“已实现”行与旧的“未实现”行同时存在 | 当前架构单一事实源发生自相矛盾 | **P1** |
| B07 | `COMPATIBILITY.md`、`docs/ci.md`、`docs/NEXT_RELEASE_CI_CONVERGENCE.md` | 含旧 SHA 基线、过时的 Windows/macOS runner 状态 | 使用者无法判断当前 SHA 是 PASS/FAIL/UNVERIFIED | **P1** |
| B08 | `tools/perf/bench.py` | 仅 7 个样例、每项 3 次、无隔离或稳定性量测、结果校验仅成功标记 | 只能作旧 P6 基线，不能作性能回归门禁 | **P1** |
| B09 | `docs/security-vm.md` | VM 文档存在，但镜像使用 `current` 可变地址、运行者可选手动拷贝目录 | 可复现性和源码证据链不够强 | **P1** |
| B10 | `tests/security/run-lane.sh` | 具备 7/7 对抗矩阵和正向 CLI 合约，但没被同 SHA 发布总门禁严格消费 | 老 VM 通过记录不能保证新发行版有效 | **P1** |

### 1.3 不能误报为缺失的能力

- `@exec/@knowledge`、统一载荷、v2 语句解析、当前 IR、Linux 本地语法测试：已有实现与旧同日记录；应在本 SHA 重测，而不是重新实现。
- `stdio/stdin/stdout/stderr`、文件/目录能力、JSON/CSV、HTTP 字节协议、DNS/TCP/UDP/TLS、环境/时间/随机/进程等：`docs/architecture/CURRENT.md` 记录有实现和产品测试。不要因旧文档片段再次宣布“全部缺失”。
- Linux strict L1 的 Bubblewrap Supervisor 与 7 项对抗测试已经存在：需求是 **当前 SHA、可信 VM、真实执行、不跳过、可追溯结果**。
- Windows/macOS strict 不是本次目标：不应为了过测试而谎称提供操作系统隔离；在这些系统上仍必须明确拒绝 `--mode strict`。

## 2. 推荐目标架构：四种 lane、一个 SHA、一份发布裁决

```text
Git commit SHA 32d8fe1...
  ├── G0: Linux GitHub-hosted / Woodpecker: fmt, clippy, compile, unit, integration, docs, stdlib
  ├── G1: Windows 原生 runner: 编译、通用语义、资源与 CLI；strict 必须 fail-closed
  ├── G2: macOS 原生 runner: 编译、通用语义、资源与 CLI；strict 必须 fail-closed
  ├── G3: disposable Linux VM: Bubblewrap 真隔离、无权限逃逸、正反路径、L1 对抗矩阵
  └── G4: 固定 Linux 性能节点: 正确性校验 + 性能统计 + 固定硬件可比性
             │
             └── Gate Aggregator：校验每项证据 tested_sha == 目标 SHA
                   ├── any failure / missing / skipped / SHA mismatch -> DO NOT RELEASE
                   └── all required PASS + clean-install + approval -> RELEASE CANDIDATE
```

**职责分离：** G0/G1/G2 验证行为与可移植性，G3 验证隔离**是否成立**，G4 验证实现变化是否引入显著性能回退。不能把 G0 的 namespace 失败称为 G3 已失败；也不能把 G3 的 QEMU/TCG 时间当成性能指标。

## 3. P0 修复一：跨平台条件编译，绝不伪造隔离

当前 `crates/marqdo-sandbox/src/lib.rs` 只在 Linux 导出 `marker_present()` 和 `verify_active()`，但 `commands/run.rs` 无条件调用。**Rust 中 `if cfg!(target_os="linux")` 不会阻止另一分支被类型检查**，不能这样修。

**推荐将平台判断集中在 sandbox crate，提供单一公共 API：**

```rust
pub enum StrictChildState {
    NotChild,
    VerifiedChild,
}

pub fn inspect_strict_child() -> Result<StrictChildState, String> {
    #[cfg(target_os = "linux")]
    {
        if !marker_present() { return Ok(StrictChildState::NotChild); }
        verify_active().map(|_| StrictChildState::VerifiedChild)
    }
    #[cfg(not(target_os = "linux"))]
    {
        // On non-Linux, even a forged marker must never authorize strict.
        if std::env::var_os("MARQDO_STRICT_ACTIVE").is_some() {
            return Err("strict active marker is unsupported on this platform".into());
        }
        Ok(StrictChildState::NotChild)
    }
}
```

这是一段 **API 设计草案**，实施时应检查模块导入与类型可见性，不可未经编译直接粘贴交付。`run.rs` 只处理 `NotChild/VerifiedChild/Err`，所有 Linux 名称只出现在 sandbox 的 `#[cfg]` 区域。`VerifiedChild` 仍必须由现有 PID 1 / uid map / NoNewPrivs / CapEff / net namespace 验证取得，不能仅凭环境变量。

**验证清单**：

1. Linux `cargo check --workspace --all-targets --locked`；Mac 和 Windows 使用各自原生环境运行同样命令。
2. `cargo test --workspace --locked --no-fail-fast` 在三平台编译并执行；禁止通过 `#[cfg]` 跳过所有跨平台行为测试。
3. 两个非 Linux 平台：`marqdo run examples/hello.mq.md --mode strict` 非零、清晰错误、不出现任何用户程序输出；伪造 `MARQDO_STRICT_ACTIVE=1` 同样拒绝。
4. Linux：原有 strict 子进程验证与伪造 marker 拒绝保持不变。
5. `--json` 模式无论三平台何种拒绝路径都产生 **一个**合法 JSON 错误文档。

## 4. P0 修复二：strict 的 `--json` 合约和 probe→launch 时序

### 4.1 目前错误具体在哪里

`commands/run.rs` 调用 `launch_strict_process(&root, file, settings)`；其在 `commands.rs` 使用 `marqdo_sandbox::launch_strict()`，正常时透传子进程 stdout/stderr，错误时只 `eprintln!("error: ...")`。同时 `launch_strict()` 的封装可能将 Bubblewrap 的非零退出原样向上传递。这样如果 runtime probe 曾可用，但实际 bwrap 无法建命名空间，`--json` 就可能收到空 stdout，并触发当前 CI 中的 `json_stdout` panic。

**这里必须区分两种失败：**

- **控制平面失败**：probe、supervisor 启动、namespace 构造、超时、捕获输出失败或没有可验证的子进程输出。必须 fail closed，不能运行项目；在 `--json` 输出规范化错误。
- **程序平面失败**：真正进入 verified strict child 并返回用户程序错误。应保留子进程输出的合法 JSON，不得再包裹一份 JSON；真实程序失败码应保留。

### 4.2 实施方案

使用已有 `launch_strict_capture` / `StrictOutcome` 做父进程输出路由，或将等价的捕获接口用于 CLI；不要把结构化输出完全留给失败中的 bwrap 进程。

- `launch_strict_process` 接收 `json: bool` 与 `command_name: &str`，所有控制平面错误统一走 `emit_strict_failure_json`。输出至少：`{ "command":"run", "success":false, "error": "...", "code":"strict.launch_failed" }`。最终字段与 `--json` 既有对外 schema 保持兼容；不要在输出中泄露宿主路径、用户数据或不受控的长 stderr。
- `StrictOutcome` 是成功运行子进程后的字节捕获，不表示已经完成隔离验证。遇到非零退出而没有合法 JSON 时，父进程应按 supervisor failure 或受控非 JSON 程序错误规范化，不得在 strict `--json` 模式输出空内容或多个 JSON。
- 一旦子进程产生合法程序响应，父进程转发 **恰好一份** JSON；拒绝 stdout 附带无关文本。`--json` 下任何 CLI 错误路径都必须保持同一协议约定。
- 记录内部诊断区分 `strict.probe_unavailable`、`strict.spawn_failed`、`strict.namespace_failure`、`strict.child_invalid`、`strict.timeout`、`strict.resource_limit`，但对外要避免泄漏宿主环境敏感信息。错误码命名在 ADR 中冻结。
- 对 `probe OK → launch 失败`、`bwrap 退出 1 且 stdout 为空`、`spawn 返回 Err`、`strict child 报运行时错误`、`超时/信号退出`分别注入故障测试，校验退出码、结构化错误、**没有任何用户代码副作用**。

### 4.3 重写易受时序影响的测试

当前 `strict_is_available()` 会单独 `inspect --platform`，再根据这个独立时间点判断后续 `run`；这是合理探测但不能当永久保证。

拆成：

- **每个平台都必须跑的 fail-closed contract**：使用可注入 supervisor 的单元/集成测试，不依赖宿主是否允许 namespace；故障必须拒绝且输出合法 JSON。**不能以 `if unavailable { skip }` 作为通过。**
- **普通 Linux hosted runner 的 smoke**：如果环境不支持 Bubblewrap，验证 `strict unavailable` 的标准拒绝；如果支持则执行正例；如果 probe 后能力丢失，验证拒绝路径，不允许 fallback。
- **专用 G3 VM 的强制正例**：显式要求 `strict available`、`run --mode strict` 必须成功、7 项对抗案例必须全部执行；任意 skip 就失败。现有 `MARQDO_REQUIRE_STRICT_SECURITY=1` 已是正确机制，应继续沿用。

**绝不能**为使 GitHub-hosted runner 变绿而移除 strict 测试、把 JSON 断言降级为“只要退出非零”、关闭用户命名空间保护或将 strict 自动降级为 project 模式。

## 5. Linux disposable-VM strict lane 的现实实现

### 5.1 为什么优先 VM 而不是 Docker

`bwrap` 需要新建用户、挂载、PID、网络命名空间。普通 Docker CI 可能已经受 seccomp/AppArmor/容器权限限制。即使 `cargo test` 在容器通过，也不能说明用户进程能在同样条件下启动新的命名空间。仓库原有 `docs/security-vm.md` 与 `tests/security/run-lane.sh` 已用 **普通用户** 在 Ubuntu disposable VM 中完成历史 7/7 验证；应在此基础上工程化。GitHub-hosted 普通 Ubuntu runner 对本项目可变的 namespace 资源也不稳定，不应承担强制 G3 正向证据。

### 5.2 建立两类 Linux 机器

**Security VM（G3）**：每个待发布 SHA 启动一台临时 Ubuntu 虚拟机；普通用户、不暴露主机秘密、Bubblewrap/AppArmor 按发行版官方配置；尽可能 KVM 加速，缺少 `/dev/kvm` 时可用 QEMU/TCG 完成隔离**正确性**验证，但 TCG 时间不能用作性能依据。VM 使用独立可销毁的 qcow2 overlay；每次任务销毁整个 overlay，不复用可能被污染的 writable home/cache。

**Performance runner（G4）**：独立固定的 Linux 硬件或 KVM VM；保持 CPU 型号、微码、内核、Rust toolchain、构建参数、负载条件与发行版一致；不参与 untrusted PR 与安全对抗代码；不与 G3 共用性能基线。G4 可以在较稳定的硬件上运行，不必跑 Bubblewrap 对抗测试。

### 5.3 可复现实例：Host、Guest、检查与测试

推荐 Ubuntu 主机：

```bash
sudo apt-get update
sudo apt-get install -y --no-install-recommends \
  qemu-system-x86 qemu-utils genisoimage openssh-client
# 若具备 KVM：
ls -l /dev/kvm || true
```

**重要：** 建议根据仓库 `docs/security-vm.md` 创建 cloud-init NoCloud seed 和独立登录密钥；下列为**构建新镜像的示意**，不是在已有机器直接清盘的命令。网络下载的 Ubuntu cloud image 应事先下载并对照官方发布校验值，记录实际 `sha256sum` 与 `uname -a`，而不是永久依赖 `.../current/...` 可变目标。

```bash
# 以下路径应在专用工作目录内执行，不可对生产盘运行。
qemu-img create -f qcow2 -F qcow2 -b ubuntu-base-pinned.qcow2 \
  strict-${SOURCE_SHA}.qcow2 24G
# 通过事先准备的 cloud-init seed（普通 dev 账号）启动隔离 VM；
# KVM 可用则 -accel kvm；否则 -accel tcg,thread=multi。
```

在 **guest** 内，管理员只为初次准备镜像安装系统依赖；测试使用普通 `dev`：

```bash
sudo apt-get update
sudo apt-get install -y --no-install-recommends \
  bubblewrap build-essential python3 git curl ca-certificates
id -u                         # 测试时必须不是 0
uname -a
bwrap --version
systemd-detect-virt --container || true
cat /proc/self/uid_map
findmnt -t cgroup2 || true
bwrap --unshare-user --unshare-net --ro-bind / / -- /bin/true
```

**不能**全局禁用 AppArmor、调低安全约束或用 `sudo cargo test` 来“让测试通过”。Ubuntu 版本对 unprivileged user namespace 的策略不同，遵循 `docs/security-vm.md` 的 packaged Bubblewrap/AppArmor 配置；如果机制不可用，G3 标为 **FAIL/UNAVAILABLE**，修环境而不修掉测试。

为保证准确的 SHA 对应关系，宿主只把目标 `git archive` 或固定提交的 checkout 传入 guest。推荐由受信任的管理端构建归档：

```bash
export SOURCE_SHA=32d8fe1f7afcdd7d7316c6692c442f5b26a24db2
# 在 Git repo 且已 fetch 到 SOURCE_SHA 的前提下：
git rev-parse HEAD
git archive --format=tar "$SOURCE_SHA" > source.tar
sha256sum source.tar > source.tar.sha256
# 把 tar 和 sha256 通过受控 SSH/SCP 传入临时 VM；guest 验证后解包
```

在 **guest** 使用普通账户运行：

```bash
set -euo pipefail
export MARQDO_REQUIRE_STRICT_SECURITY=1
export CARGO_TERM_COLOR=never
# 必须提前验证固定源码的 SHA 和传输 SHA256
cargo test -p marqdo-sandbox --test security_linux --locked -- --test-threads=1
./tests/security/run-lane.sh
# 正式 RC 还应独立构建 --release --locked，并对 release binary
# 做 strict 正例、非法权限、伪造 marker、预算拒绝的 smoke。
```

`run-lane.sh` 已经会执行 `security_linux`，上例中单独再调用一次用于先快速定位对抗测试；正式 CI 可以去掉重复以节省时间，**但不能删去任何安全场景**。

### 5.4 安全测试矩阵：不能只测“启动成功”

必须保持现有 7 项：

1. 严格子进程 UID/PID1/NoNewPrivs/CapEff/uid_map/net namespace 真正生效。
2. 无法读到宿主 `/etc/shadow`、`/root`、其他用户目录，却能读挂载的 `/project` 许可内容。
3. 无法写宿主目录或只读项目；私有 `/tmp` 不影响宿主。
4. `/proc`、capability 提升、`setuid`、`mount`、再次 `unshare` 等路径被拒绝。
5. 非 loopback 出站网络及原始 socket 被拒绝。
6. 文件描述符、子进程数量受到监督。
7. 内存、CPU、墙钟、stdout 输出预算触发安全拒绝。

叠加当前语言/资源语义的新回归：`@knowledge` 中的任意可疑载荷绝不产生 Host effect，`@exec` 中未经授权 `resource.open(file/tcp)` 拒绝；`stdin/stderr` 的 strict 权限符合当前 L1 明确契约；`--allow-env`、`--allow-process`、`--allow-random`、`--allow-net`、Plugin、Foreign、Worker、Trace 均被严格拒绝；`MARQDO_STRICT_ACTIVE` 伪造标记失败；缺少 bwrap 与 probe→launch race 均 fail closed；`--json` 的成功、失败都恰好输出一个文档。

### 5.5 接入 CI 时最关键的安全边界

`security-linux.yml` 当前使用 `runs-on: [self-hosted, linux, marqdo-security]`，但 GitHub 官方安全指导特别提醒：**公开仓库的持久自托管 runner 会受到来自 PR 代码的严重威胁**。不要直接把常驻 runner 放在拥有 GitHub Token、SSH 私钥、内网资源或重要文件的 Linux 主机上。规范见 `https://docs.github.com/en/actions/reference/security/secure-use`。

建议：

- GitHub-hosted Windows/macOS/G0 **保持现有 hosted runner**，不必迁移到 Linux Docker。
- G3 仅接收经过保护分支审查后的 SHA 或由管理员指定 SHA 的手动验证；**不让任意 fork PR 在受信任自托管 VM 上执行**。
- 每个 G3 job 启动独立临时 VM（或使用生命周期真正一次性的自托管 runner）；无持久磁盘、无主机挂载、无宿主 Docker socket、无 SSH/Git/云凭证、无发布 token；最小 `permissions: contents: read`，`actions/checkout` 使用 `persist-credentials: false`。
- Runner 注册凭证应由受信任的受控 orchestrator 短期签发，不应进入项目源码或客户机磁盘快照；执行后销毁整个 VM、归档日志和检查结果。若无法安全提供 runner，请先运行**独立 Linux VM 实验**并报告“本地 VM PASS、远程 G3 UNVERIFIED”，切勿伪造绿色 CI。
- 单独记录 `source_sha`、`runner_image_sha256`、`guest_kernel`、`bwrap_version`、`rustc -vV`、`git archive_sha256`、`runner_kind`、`outcome`、`artifact_sha256`、`run_url`。所有这些都应写入一个 schema 化的 `strict-evidence.json`。

**触发与绑定：** 当前 security workflow 只有 `workflow_dispatch/schedule`，无法自动证明发布候选 SHA。建议在可信 `master` push 后触发专用 VM job，或者由 release orchestrator 请求显式的 `source_sha`。**尤其注意：手动 workflow_dispatch 所在工作流的 `GITHUB_SHA` 不一定等于输入的被测试源码 SHA；必须分别记录 `workflow_sha` 与 `tested_sha`，并校验 checkout/归档等于 `tested_sha`。**

### 5.6 推荐 `security-linux.yml` 修改要点（示意，不是现成完整文件）

```yaml
name: Linux strict security lane (G-C)
on:
  push:
    branches: [master]
  workflow_dispatch:
    inputs:
      source_sha:
        description: "Exact reviewed commit SHA"
        required: true
        type: string
permissions:
  contents: read
jobs:
  strict-security:
    if: github.event_name != 'pull_request'
    runs-on: [self-hosted, linux, marqdo-security]
    timeout-minutes: 45
    steps:
      - uses: actions/checkout@v4
        with:
          ref: ${{ github.event_name == 'workflow_dispatch' && inputs.source_sha || github.sha }}
          persist-credentials: false
      - name: Bind tested SHA and run G-C
        shell: bash
        run: |
          set -euo pipefail
          tested_sha="$(git rev-parse HEAD)"
          echo "TESTED_SHA=$tested_sha"
          test "$(id -u)" -ne 0
          ./tests/security/run-lane.sh
          # 后续调用安全证据脚本生成 strict-evidence.json，
          # 包含 tested_sha、环境、结果、工件摘要与流水线链接
```

前提：**在 runner 连接 workflow 前已经完成 ephemeral VM provisioning 和信任隔离**。上述 YAML 本身不会使持久 self-hosted runner 安全；如果没有完成此基础设施，先保持手动隔离 VM 的证据并拒绝发布。

## 6. Windows/macOS：Linux 能做什么、不能做什么

**直接回答：不能仅在一台 Linux 主机上完成真实的 macOS 与 Windows 原生 CI 认证。** Linux 可负责调度、打包源码、跨目标检查部分编译问题、核验日志和合并结果，但不能用 `cargo check --target`、Wine、模拟执行替代真实原生 runner 对文件系统、动态库、路径分隔符、权限、线程、进程与协议的测试。

**最低成本方案：继续使用已经配置的 GitHub-hosted `windows-latest`、`macos-latest`。** 当前这两个 Job 已运行，问题是 `E0425`，不是“无 runner”；先修 B01，再运行同 SHA 相关后续提交的原生 matrix。

Windows 原生应覆盖：正常 `--mode project/pure`、CLI run/call/check、UTF-8 和 CRLF、锁与路径、标准库、Tool/Trace、非 Linux strict fail-closed；明确未支持的 Windows Native/Worker ABI 要验证 **拒绝而非莫名崩溃**。

macOS 原生应覆盖：上述 portable 语义、动态插件 host target、路径大小写与 `dylib`，非 Linux strict fail-closed。若有承诺 Linux+macOS Native 插件，应建立真实 dylib 调用与 ABI 错误测试；不要以“Cargo 编译成功”替代运行验证。

如果组织希望**所有设备统一由 Linux 内部调度**：Windows 可以在 Linux/KVM 上管理真实 Windows guest（需有效 OS 授权、镜像安全与许可），注册 guest runner；macOS 则应使用真实受支持的 Apple/macOS runner 或 GitHub-hosted macOS。不能在 Linux 上把 macOS 交叉编译当作“原生 lane”。

跨平台 CI 修复并绿灯后，才在最新提交上记录确切 Job URL、OS、架构、编译器、成功测试数量和 unsupported 能力列表。不要将这项证明等同于 G3 strict 安全证明。

## 7. 性能稳定性：单独建立可比的 Linux 性能基线

### 7.1 已有脚本的真实边界

现有 `tools/perf/bench.py` 用 `/usr/bin/time` 和 `time.perf_counter` 对七个 workload 各跑 **3 次**，输出中位运行时间与峰值 RSS。它检查进程退出码和 JSON `success`，但没有验证每个工作负载的正确输出、没有预热、没有环境噪声报告、没有隔离/固定 CPU、没有历史可比基线、没有性能回归置信区间。**这个脚本是可重复实验起点，不是长期性能稳定性保障。**

### 7.2 分离三种性能指标

| 类别 | 目标 | 宿主 | 测量方法 |
|---|---|---|---|
| 语言核心吞吐 | 解释器、表达式、内存/容器/标准库算法 | 固定 G4 Linux | workload 完整输出校验 + wall、CPU、RSS |
| CLI 用户体验 | `check/run/inspect` 冷启动、解析、载入 stdlib | 固定 G4 Linux | fresh process p50/p95、启动时间与加载时间；冷热分组 |
| strict 安全开销 | Bubblewrap probe、re-exec、映射/检查、严格模式运行时间 | 固定隔离性能节点（与常规 G4 区分） | 同输入 pure 与 strict 比值；只作性能评估，不替代 G3 安全证据 |

**不要在 QEMU/TCG 上做最终性能门禁**；其不稳定和极高模拟开销与解释器性能无关。若在 KVM guest 中对性能做长期追踪，固定相同 CPU 型号、vCPU 拓扑、资源配额和宿主负载；不要把不同物理机器或任意公共 GitHub-hosted runner 的耗时直接比较。

### 7.3 新性能基准工具要求

建议扩展 `tools/perf/bench.py` 或替换为 `tools/perf/runner.py`，但不要在本轮重构中引入第二套业务标准库。

1. **固定二进制**：`cargo build --release --locked -p marqdo-cli`，记录 SHA、`rustc -vV`、`Cargo.lock` digest、release profile 与二进制 SHA256。基准期间禁止重新编译。
2. **确认正确性**：每个 workload 事先记录预期 wire JSON 的 digest / 值、stdout；任一结果不一致直接报 `CORRECTNESS_FAIL`，不得拿错误运行的“更快时间”计入性能。
3. **预热**：至少 5 次 warmup，正式每项至少 20 次采样，顺序采用固定随机种子或交错设计，以避免热度/频率漂移造成系统性偏差；冷启动任务单独统计，不混入温态样本。
4. **报告统计**：p50/median、p95、MAD/离散度、最大 RSS、CPU user/sys 时间；记录样本总数、超时/错误比例、环境状态与输出摘要。对极短小样本增加批量重复，避免计时器粒度主导结果。
5. **环境控制**：宿主上使用非 root 性能账户；固定 CPU affinity（如 `taskset -c <隔离核心>`，前提是该核心确实可用）、固定性能模式/频率策略仅在专用宿主且经管理员允许时设置。检查温度/节流、并发负载、CPU governor 和内核版本；若严重干扰则结果记 `NOISY` 重新采样，不偷偷删掉慢样本。
6. **性能变更统计**：同一机器同一配方上与预先批准的基线比较。新项目初期可以把 p50 回退 ≥15% 或峰值 RSS 回退 ≥20% 作为**需要调查的建议阈值**，并非绝对通用的自动回滚线。基线稳定并经历史数据校准之后再启用 hard gate；使用 MAD 或 bootstrap 区间避免偶发假阳性。
7. **跨平台构建不等于性能验证**：Windows/macOS matrix 重点测功能；若要宣称它们的性能稳定，另建各自稳定的性能 runner/baseline。
8. **双向检验**：对于有意大改解析器/Stdlib 的提交，记录吞吐增益、内存占用变化和 cold-start 代价，不因单项变快而忽略整体退化。

### 7.4 推荐工作负载（在已有 7 项之外补齐）

- **Parse/check**：真实大 Markdown 文件、1000+ heading/mode 切换、中文 Span、GFM 表格、复杂引用。
- **Runtime**：递归与迭代阶乘、hash table、大整数、`Memory` 增长/边界、嵌套 JSON、CSV。
- **Stdlib**：UTF-8 大字符串、JSON 反序列化、Table join/group/sort、HTTP 字节 chunked。
- **I/O**：stdout 多次小块/单次大块、文件读写、TCP loopback（单独统计 I/O 与 OS 影响）。
- **strict**：同一纯 workload 在 normal/pure 与 strict 上对照、supervisor 启动开销、受限输出；不在生产安全 runner 上跑压力极限长测试。

示例基准结果 schema（字段名称正式实施时冻结）：

```json
{
  "schema": "marqdo.perf.v2",
  "source_sha": "<full 40-hex commit>",
  "binary_sha256": "sha256:<digest>",
  "platform_id": "linux-x86_64-fixed-node-01",
  "os_kernel": "<uname -r>",
  "rustc": "<rustc -vV>",
  "profile": "release",
  "workload": "json-parser",
  "valid_output_sha256": "sha256:<expected-output>",
  "warmups": 5,
  "samples": 20,
  "median_ms": 0,
  "p95_ms": 0,
  "mad_ms": 0,
  "max_rss_kb_median": 0,
  "status": "MEASURED"
}
```

数值 `0` 仅是占位示意，实际报告**不得使用假数据**；字段必须由真实测量填充。JSON 中不能把 guest / KVM / TCG 的结果混为同一基线。

## 8. 统一发布证据与状态治理

### 8.1 Evidence manifest：可验证的五元组

对每一个待发布提交，建立 `docs/evidence/ci/<SHA>/manifest.json`（或 CI artifact 同路径），包含：

```json
{
  "schema": "marqdo.release-gates.v1",
  "target_sha": "<full commit SHA>",
  "gates": {
    "linux": {"state": "PASS|FAIL|UNVERIFIED", "tested_sha": "<SHA>", "run_url": "<URL>"},
    "windows": {"state": "PASS|FAIL|UNVERIFIED", "tested_sha": "<SHA>", "run_url": "<URL>"},
    "macos": {"state": "PASS|FAIL|UNVERIFIED", "tested_sha": "<SHA>", "run_url": "<URL>"},
    "strict_vm": {"state": "PASS|FAIL|UNVERIFIED", "tested_sha": "<SHA>", "run_url": "<URL>"},
    "performance": {"state": "PASS|WARN|FAIL|UNVERIFIED", "tested_sha": "<SHA>", "run_url": "<URL>"}
  },
  "release_decision": "DO_NOT_RELEASE"
}
```

正式工具中应将 `state` 设计为枚举（而不是把以上 `PASS|FAIL...` 字符串当数据）。校验所有 `tested_sha` 与 `target_sha` 的相等性、run conclusion/job 状态、测试完整数量、artifact digest、构建二进制 digest，以及 `strict_vm` 的实际 VM/非 root/7/7 证据。**跳过、超时、取消、排队未启动、缺失、老 SHA 均不算 PASS**。

### 8.2 强制决策规则

- `G0` Linux CI **必须 PASS**；Mac/Windows 原生支持矩阵所承诺的测试必须 **PASS**。
- `G3` strict VM **必须 PASS**，才能发布包含 `strict L1 supported` 的候选。若选择移除 strict 产品宣称，应有独立的 API/版本策略与负责人批准，本 RFC 不把这当作自动逃生路径。
- 性能基线 G4 至少必须 `MEASURED` 且没有未解决的确定性大回退；未达到统计可比性的报告记 `UNVERIFIED` 或 `NOISY`，不能写 PASS。
- RC 构建与 clean-install smoke 绑定同一 commit SHA，包含 stdlib digest 验证、二进制 checksum 与完整 LICENSE/README/specs；平台未覆盖的功能必须明确标注 `UNSUPPORTED`。
- 发布最后由负责人审批；绝不让文档的 `PASS local` 自动变成 `PASS remote`。

### 8.3 清理文档状态漂移

只保留三个职责明确的事实源：

1. `docs/architecture/CURRENT.md`：唯一的**当前架构和 API 支持矩阵**。删除已经被后续实现推翻的重复旧行，不能在同一表同时写“已实现”和“未实现 DNS/TLS”。
2. `docs/STATUS.md`：**提交/SHA + 运行平台 + 结果 + CI URL/本地日志**，新数据写在顶部；历史章节明确标记旧 SHA。
3. `COMPATIBILITY.md` 和 `docs/releasing.md`：从前两者与 CI evidence 自动/受控整理支持声明，不允许复制过去版本的绿色状态。

`docs/ci.md` 要改掉“Windows/macOS 原生 runner 未配置”的绝对表述：GitHub-hosted matrix **确实已运行，本 SHA 编译失败**；Woodpecker 本身没有 Windows/macOS agent 是另一件事。`docs/NEXT_RELEASE_CI_CONVERGENCE.md` 标记旧基线为 Historical，不再作为当前 green 证明。`docs/security-vm.md` 给出当前 SHA 的 VM 重跑流程，旧数据只作复现参考。

## 9. 分阶段开发方案：具体文件、PR 与合并门禁

### PR-1：修复平台编译与 strict 结构化拒绝（最先做）

**主要文件**：`crates/marqdo-sandbox/src/lib.rs`、`crates/cli/src/commands/run.rs`、`crates/cli/src/commands.rs`、`crates/cli/tests/integration_cli.rs`、`crates/marqdo-sandbox/src/linux.rs`（仅必要的 outcome 分类与可测试注入）、相关负向测试。

**做法**：统一跨平台 `StrictChildState`、保证所有 `strict --json` 失败返回单 JSON、保持非 Linux fail closed、为 probe/launch race 注入确定性测试。禁止动语言解析器或 Host 的权限边界。

**合并门禁**：Linux fmt/clippy/test PASS、Windows/macOS 原生 matrix 通过编译和 portable tests；GitHub-hosted Linux namespace 受限情况下仍规范拒绝；证明 project 代码从未执行。

### PR-2：G3 Linux VM 可重建与本 SHA 安全证据

**主要文件**：`docs/security-vm.md`、`tests/security/run-lane.sh`、`tests/security` 下的回归场景、`tools/ci/strict-evidence.py`（新）、`.github/workflows/security-linux.yml`，以及 runner provisioning **受信任基础设施**（必要时独立私有仓库，不放生产 SSH key）。

**做法**：固定 VM 基镜像及 digest、无 root 测试、7/7 + 新语义负向案例、未运行零跳过、生成证据 JSON、按 SHA 触发并上传日志。Runner 一次一销毁。

**合并门禁**：`tests/security/run-lane.sh` 对 **当前最终候选 SHA** 真正 PASS；包含足够的测试日志、VM 环境摘要和运行 URL。Security VM 跑在容器内或以 root 通过不合格。

### PR-3：Linux 性能样本与可比基准

**主要文件**：`tools/perf/bench.py`（扩展或原位替换）、`tools/perf/workloads.json`、`tools/perf/expected/`、`tools/perf/README.md`、`docs/performance.md`（新）、独立性能 CI 工作流。

**做法**：固定 runner、Release 二进制 SHA、5 warmups/20 samples、先正确性后计时、p50/p95/MAD/RSS、环境 manifest；按可信历史样本建立 baseline。不得误把 TCG 模拟负载当发布性能。

**合并门禁**：全部 workload 正确、报告 schema 可机读、能重复运行并比较相同硬件同一构建配置；达到统计阈值的真实回退需调查解决。

### PR-4：按同一 SHA 聚合发布门禁并清理文档

**主要文件**：`.github/workflows/ci.yml`、`docs/STATUS.md`、`docs/architecture/CURRENT.md`、`COMPATIBILITY.md`、`docs/ci.md`、`docs/releasing.md`、`docs/security-gate.md`、`docs/evidence/`、`crates/cli/tests/docs_consistency.rs`。

**做法**：聚合每个平台最新通过状态，禁止 skipped/老 SHA 证据复用；删除文档重复冲突行；RC clean-install 与 `SOURCE_SHA` / stdlib digest 绑定。增强 docs consistency，以防出现“已实现 TLS”和“无 TLS”的同页冲突。

**合并门禁**：所有 release gate 都能明确输出 `PASS/FAIL/UNVERIFIED`，明确记录所测 SHA；不能手工把旧证据改成当前 PASS。

## 10. 必须增加的自动化测试清单

### 10.1 跨平台与 JSON

- [ ] Windows/macOS `cargo check --workspace --all-targets --locked` 和 `cargo test --workspace --locked` 均执行成功。
- [ ] 非 Linux 不编译 Linux 专用标记函数的直接访问；伪造 marker 不绕过保护。
- [ ] strict `--json`：无 bwrap、probe 拒绝、spawn 报错、namespace 建立失败、超时、被 kill、成功执行、执行期 RuntimeError、Foreign/Plugin flags 拒绝都只产生一份 JSON。
- [ ] 不能从 `--json` 错误路径输出未经编码的 Bubblewrap 文本作为完整标准输出。
- [ ] 逻辑相同的普通输出路径没有 stdout/stderr 渠道混乱；项目输出不得在拒绝后出现。

### 10.2 strict VM

- [ ] 本 SHA guest `id -u != 0` 且 `systemd-detect-virt --container` 未把它识别成被禁止的受限容器。
- [ ] strict probe 可用；7/7 对抗测试强制运行、全部通过、0 ignored/skipped。
- [ ] 正向 `run/call/json` 成功；无权限文件/网络/扩展申请 fail closed；伪造 marker 失败。
- [ ] `@knowledge` 中载荷无副作用，不突破权限；`@exec` 中不授权资源会拒绝。
- [ ] 同 SHA debug 与 release 二进制 smoke 均 PASS；退出码和 JSON 对外协议固定。
- [ ] 新镜像、运行日志、证据 manifest、`tested_sha`、run URL 全部可追溯。

### 10.3 性能与资源稳定性

- [ ] 现有七 workload 预期输出已验证，不仅是 `success=true`。
- [ ] 大型 Markdown cold check、复杂 JSON/CSV、Table、I/O、Memory、循环等有覆盖。
- [ ] 基准二进制和环境哈希固定；独立性能 runner 上建立至少三次不同日期/同硬件的基线后再冻结阈值。
- [ ] 报告至少包含 warmup/sample 次数、p50/p95/MAD、RSS、失败率、环境噪声信息。
- [ ] strict VM 对抗时延与普通性能指标分离；不会把 QEMU TCG 时间与 KVM/native 时间混比。
- [ ] release 命中明确阈值时需要 review，不能靠一条 noisy 采样直接判回归或通过。

## 11. 工程开发规范与禁止事项

- 修改安全边界必须先加不允许逃逸的负向测试；修改 CLI 错误协议先改测试契约再实现。
- 非 Linux 上不可编译的 symbol 只能存在于 `#[cfg]` 对应模块内，**不使用运行时 `cfg!` 规避**。
- 新安全机制默认 `DENY`，可用性由实际探测决定；probe 成功不代表后续 launch 永远成功。
- 严格模式不得自动降级为 `pure/project`，不得保留原宿主目录的真实写权限；不得全局关闭 AppArmor、seccomp 或 namespace 限制以取得绿色测试。
- 不在生产机/常驻 privileged Docker 直接运行对抗安全测试。普通业务测试与 G3 隔离测试分开维护。
- 不把 Secret、runner registration token、SSH key、环境变量凭证、完整宿主绝对路径打到公共日志或 evidence。
- 现有标准库坚持 `.mq.md`，本轮工作不新增专用 Rust JSON/CSV/打印内置算法，也不将 stdlib 强行拆仓库。
- CI 结果与性能证据要有固定 schema、SHA、文件摘要；无法核验的历史记录标记为历史，不声称本 SHA 通过。
- 文档必须能被新开发者按顺序运行；关键命令说明 host/guest、预期退出码与什么行为意味着 FAIL。

## 12. 最终验收与结论

**P0 阻断清单（全部完成才能讨论 RC）**：

1. 修复 `E0425`，Windows/macOS 原生 job 在当前候选 SHA 通过全部已承诺的 portable 语义与 CLI 测试。
2. 修复 GitHub-hosted Linux 上 strict 失败时 `--json` 输出为空的缺陷，证明 launch race 仍 fail closed。
3. 用真正的非 root disposable Linux VM 重跑全部 G-C 7/7 + 当前语义的 strict 正反例，产生 **本 SHA** 证据。
4. 引入同 SHA 证据聚合和 clean-install 检查，缺失或失败立即 `DO NOT RELEASE`。
5. 性能基准具备正确性校验与固定宿主比较能力；任何真实、可重复、未经批准的严重性能回退必须解决或公开记录为阻断项。

**当前评定**：语法重构不是当下主阻断；新的远程 CI 已经给出了精确故障，Windows/macOS **FAIL**、Linux 主测试 **FAIL**、G3 VM **UNVERIFIED**、RC **SKIPPED**。修复优先级应是 **P0 跨平台条件编译 → P0 strict JSON/失败分类 → P1 disposable VM 同 SHA 正向隔离证据 → P1 性能稳定性与 release evidence**。

### 参考源码与证据

- 最新审查 SHA：`https://github.com/cflmy/marqdo-cli/commit/32d8fe1f7afcdd7d7316c6692c442f5b26a24db2`
- 远程测试：`https://github.com/cflmy/marqdo-cli/actions/runs/44883104060`
- 跨平台直接引用问题：`crates/cli/src/commands/run.rs`，`crates/marqdo-sandbox/src/lib.rs`
- JSON 路由/测试：`crates/cli/src/commands.rs`、`crates/cli/tests/integration_cli.rs`
- 安全 VM：`docs/security-vm.md`、`docs/security-gate.md`、`tests/security/run-lane.sh`
- CI：`.github/workflows/ci.yml`、`.github/workflows/security-linux.yml`、`.woodpecker.yml`
- 性能：`tools/perf/bench.py`
- 实现状态与文档：`docs/STATUS.md`、`docs/architecture/CURRENT.md`、`COMPATIBILITY.md`
- GitHub 官方自托管 runner 风险说明：`https://docs.github.com/en/actions/reference/security/secure-use`
- Bubblewrap 项目：`https://github.com/containers/bubblewrap`

---

*备注：文档中列出的代码与 YAML 是设计和实施模板，未在当前环境对最新 SHA 执行编译、安全 VM、性能测试；通过状态只引用实际可见的 GitHub Actions 日志或仓库已有的历史记录。*
