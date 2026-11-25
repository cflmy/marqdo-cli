# RFC-REL-0001 — Marqdo 3.0 预发布路线、发布门禁与 LTS 策略（建议草案）

> 状态：**Adopted as the release policy for `3.0.0-beta.1`**（2026-11-22，源码 `2c7c4f31e7a388d3fcfa717a77067e696ed13854`，[release](https://git.cflmy.cn/cflmy/marqdo-cli/releases/tag/v3.0.0-beta.1)）。REL-01～REL-05 已按本文实施；实际门禁结果记录在 `docs/STATUS.md`、`docs/evidence/` 和 `docs/evidence/ci/<release-sha>/manifest.json`。
> 代码核对基线（提议时）：`cflmy/marqdo-cli@56189b176c612bf399ff965c3b642077f6b04617`，2026-11-22；§2 的“剩余门禁”表是迁移前快照，当前事实以 `docs/STATUS.md` 为准。
> 现有发布依据：`docs/releasing.md`、`docs/STATUS.md`、`COMPATIBILITY.md`、`spec/extension-selection.md`、`spec/callable-resolution.md`。
> 本文是策略与门禁定义；**tag 只能在该 SHA 的全部门禁通过后创建**，本文本身不构成任意 SHA 的发布批准。

## 1. 产品定位和版本结论

- 旧 `cflmy/marqdo` 已发布至 `v2.9.1`，新的 `marqdo-cli` 是同一品牌下的**语言重实现与破坏性架构更新**，不是旧 CLI 的逐项兼容升级。
- 公开产品版本可从 `3.0.0-beta.1` 起步；`3.0.0` 不带后缀应留给稳定版。Rust 工作区、CLI `--version`、Analysis `producer.version`、Tool 版本协商均应反映该版本。
- 使用一个仓库作为 3.x 官方主实现；旧仓库保留历史与迁移链接，**不声称源文件、插件 ABI、lockfile 兼容**。
- 正式提供持续维护承诺之前，不应把任何 Beta 或 RC 称为 LTS。建议待稳定的 `3.0.0`、`3.1.x` 实际使用数据验证后，再考虑把 `3.2.x` 设为第一条 LTS；是否能以此命名取决于质量门禁，而不是预定日历。

## 2. 已核实事实与剩余门禁

| 事项 | 基线证据 | 结论 |
|---|---|---|
| 语言执行语义 | RFC-LANG-0002、ADR-0052、现有测试 | 已实现、破坏性，不允许旧语法自动降级 |
| 外部插件与 Provider | ADR-0054～0058、`spec/resource-provider-v2.md`、`spec/extension-selection.md` | 核心路径已实现；Provider Worker 仅宣称 Linux x86_64 |
| 独立扩展验收 | `docs/evidence/extension/6a7bb983.../manifest.json`：7 项 PASS | 证据只绑定旧 SHA `6a7bb98`，需在发布提交重跑 |
| CI | `actions/runs/44928442709`，SHA `56189b1` | Linux、Windows、macOS 各运行契约 PASS；tag 发行包任务因未打 tag 而 SKIPPED |
| strict/G3 | `actions/runs/44928442715` 查询时 queued；本地隔离 VM 历史 PASS | 发布 SHA 缺远程 G3 证据；不可声称整个 strict 已认证 |
| G4 | `docs/evidence/2026-11-21-perf-woodpecker-agent-1.md` | 9/9 结果正确，固定 agent 两次 5 warmup / 20 样本；共享 powersave 环境，WARN、旧 SHA |
| 公开版本 | `Cargo.toml` 仍为 `0.1.0`，若干工具兼容样例 `<1` | 3.0.0-beta 发布前必须迁移验证 |
| 第三方入门 | README 有扩展示例、`spec/` 有协议；Tool v1 标头仍称 Proposed，plugin spec 某段称 Native only | 需要整合并去除失效状态文本 |

## 3. 两套必须明确分开的发布策略

### 3.1 Beta（允许明确声明的非安全性缺口）

目标平台为 **Linux x86_64**；其他平台的核心 CLI 测试结果可以公开说明，但不默认提供支持声明。所有清单要公开写明平台、功能和限制。

**不可豁免的 Beta 门禁：**

1. 在清洁、固定的同一提交（发布 SHA）上：`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --locked`、`cargo build --release --locked -p marqdo-cli -p marqdo-provider-worker` 全部通过。
2. GitHub 最新同 SHA 的 Linux 主 CI PASS；发布产物中 `marqdo` 与同平台 `marqdo-provider-worker` 同目录，不允许只打包 CLI。
3. 冻结 CLI 的外部仓库黑盒验收在**发布 SHA**重新执行，至少 pure pinned-Git 包、Callable Worker、陌生 Foreign Kernel、Provider v2、Tool/Analysis/Trace、权限负例及工件改动负例全部 PASS；生成并校验 `docs/evidence/extension/<SHA>/manifest.json`。
4. 冷安装、离线 `--version`/`--help`、`check/run/call`、lock/sync、Tool 安装与分发、Provider Worker 启动、错 SHA/错目标/未授权拒绝全部由发布归档中的**实际二进制**完成；不得依赖开发源码目录。
5. **不得虚假宣传安全隔离。** 发布包若保留并开放 Linux `strict`，必须有该 SHA 的真实 G3 隔离负例证据；若暂无证据，需采用明确经过测试的 fail-closed 禁用方式并在公告中写 `strict: unsupported`。不允许仅改 README 却继续将未验证的 `strict` 描述为安全保障。Beta 的这一策略必须经维护者明确签字并由 CI 可复查。
6. G4 基准不得造假；若当期固定节点仍是 WARN，可由维护者批准 **Beta/RC-only WARN** 并公开环境和限制；必须通过正确性基准，不允许数值漂移或已确认的灾难性回退。RC 允许经维护者批准的共享节点 WARN；最终 stable/LTS 必须使用专用固定节点，该共享节点例外不延续到 LTS。
7. 明确产品版本 `3.0.0-beta.1` 与协议独立版本；工具 SemVer 协商在真实测试版上可安装、拒绝不兼容范围、不会误把 `<1` 当成支持 3.x。
8. `README`、`CHANGELOG`、`COMPATIBILITY`、`docs/STATUS`、`docs/releasing`、协议入口页、迁移说明，均更新至同一事实基线；无“未实现却声称已发布”或“已实现仍称 Proposed”的矛盾。
9. 不发布未经实际构建/烟测的平台二进制；产物附 SHA256、SOURCE_SHA、构建工具链信息和已知限制。

**可延期至后续 Beta 或稳定版：** Windows Native/Worker ABI、macOS Provider Worker、strict L2、uv/npm 自动建环境、分布式 registry、JIT、内置 Knowledge/Proof/View、成熟异步/并发模型、未经承诺的平台安装包。明确注明项目默认 `project` 模式不是 OS 沙箱，Worker 也不是完整沙箱。

### 3.2 Stable / LTS（不可用 Beta 例外替代）

- Stable `3.0.0`：在公开支持的 Linux 平台上所有承诺功能有安全/兼容/性能证据；公开协议与变更策略清楚；有迁移说明和至少一次真实使用者反馈周期。平台可继续限定 Linux，不要求通过虚假的“全平台 LTS”门禁。
- LTS（建议后续 `3.2.x`）：冻结 `.mq.md` 语言语义、Analysis/Trace/Tool/Plugin/Provider 各自协议主版本和 lockfile 契约；提供安全修复流程、回归 CI、外部仓库兼容矩阵、从 3.0 及 2.9 迁移材料、发布渠道与公告模板。
- LTS 应提前承诺支持窗口（建议至少 12 个月，实际长短由维护资源决定），定义安全补丁与非破坏性修复回移规则，明确 API/ABI 哪些会不兼容；不因开发完成某一里程碑自动称 LTS。
- 默认不为旧 `cflmy/marqdo` 的历史语法或旧插件 ABI 建隐性兼容层。需要迁移的由独立迁移工具、教程和示例承担。

## 4. 3.0.0-beta.1 之前建议拆分的 5 个短 PR

**REL-01：全仓版本线迁移（强制）**
- 将 Cargo workspace 包的产品版本改为 `3.0.0-beta.1`，更新 Cargo.lock 与 crate 编译产物；不必将 *独立协议版本*、依赖包版本、官方 stdlib 版本一并改为 3。
- 扫描 `0.1.0`、`>=0.1,<1`、`M0.5`、`pre-1.0` 等在发行说明、JSON schema consumer、Tool fixture、示例、CI 中是 **产品版本**还是 **独立包/协议版本**。逐项更新，严禁盲目全局替换。
- `project.marqdo` 目前文档注明仅 informational；为未来稳定兼容，建议在 Beta 之前增加**显式版本范围的检查**，最少在不兼容时发出清晰诊断并在 Beta 正式版前确保 fail-closed；编写对应正负测试。如果无法实现，必须写明 Beta 不保证该字段生效。
- 为使用 `semver::VersionReq` 的 Tool manifest 添加 `>=3.0.0-beta.1,<4.0.0` 等真实候选测试，并确保带预发布后缀的版本匹配符合 SemVer 语义。

**REL-02：测试版支持矩阵与安全策略（强制）**
- 发布声明只选自己真实能够测试和修复的平台。建议首发 Linux x86_64，其他平台根据可运行烟测增列为 Experimental。
- 为 `strict` 选择：A. 当 SHA 的 G3 真实通过则在 Linux 标为实验性 probe-gated；B. 若无，则构建/配置和端到端测试保证 **fail closed**，公开标为不支持。禁止“默默降级到 project”。

**REL-03：全量发行归档与冻结二进制（强制）**
- `cargo build --release --locked -p marqdo-cli -p marqdo-provider-worker`。
- 打包 `marqdo` + `marqdo-provider-worker`、文档/许可证/协议/必要示例，清洁目录安装后测试；两者同目标 triple、版本、来源 SHA；检查归档 SHA256。
- 在干净外部仓库重复黑盒套件，证据绑定发行 SHA 和精确二进制 hash；至少做未授权和工件篡改的消极测试。

**REL-04：开发者入口文档（强制）**
- `docs/developers/README.md`、`quickstart.md`、`tools.md`、`libraries.md`、`extensions.md`、`analysis-trace.md`、`compatibility.md`；与 `spec/` 的规范性协议以链接方式衔接，避免复制分叉契约。
- 设置 `marqdo view/knowledge/prove` 作为 Tool 使用的示例，并将其作为外部仓库模板，而不是 CLI 内置命令。
- 修复 `spec/tool-v1.md` 的 Proposed 状态和 `spec/plugin-abi.md` 的 Native-only 旧说法；修复旧版发布指令、样例 version、已过时的 G3/G4 状态。

**REL-05：Beta Release Candidate 审批（强制）**
- 切 release candidate SHA，再跑所有必需门禁，不在验证期间继续修改同一版本的内容；必要变更形成下一 SHA 并重跑。
- 将稳定版发布策略与 Beta 策略分别标记和自动验证，不用一个 `DO_NOT_RELEASE` 字段悄悄代表两种不同门槛。
- 维护者明确批准 `v3.0.0-beta.1` 标签和 GitHub prerelease（`prerelease: true`），不要把测试版误当正式发布或 LTS。保留回滚/撤回方案与已知限制。

## 5. 建议的版本线（条件性，不是承诺日程）

| 版本 | 目的 | 进入条件 |
|---|---|---|
| `3.0.0-beta.1` | 发布真正可运行、可接第三方库的新语言基线 | 上述 Beta gate 全绿或明确允许的非安全性 WARN |
| `3.0.0-beta.2` | 修复外部消费者、协议示例及安装包问题 | 跨仓回归全部 PASS，收集第一轮外部反馈 |
| `3.0.0-rc.1` | 冻结最小公共契约和平台支持范围 | 不再新增破坏语义；发布契约全部文档化 |
| `3.0.0` | 3.x 稳定版 | 公开承诺范围内的 release gate 完整 PASS |
| `3.1.x` | Tools/Knowledge/View/Proof 实际生态检验 | 外部仓库无核心改动迭代，公共协议更成熟 |
| `3.2.x LTS`（候选） | 长期支持分支 | 真实跨版本兼容证明、安全更新流程、明确维护窗口 |

**建议首先冻结而不强求全部功能成熟的内容**：RFC-LANG-0002 的语义；Analysis v2、Trace v2、Tool v1、Plugin ABI v1、Provider v2、lock v4 的**当前定义**。未来不兼容修改应提升**对应协议版本**，不能因为产品叫 3.0 就把所有协议号写成 3。

## 6. 发版前必须人工审阅的风险

- 原始 `docs/releasing.md` 按稳定版执行，对 Beta 需要新增经批准的 `prerelease` profile，不能在脚本中硬关门禁。
- GitHub Push CI 成功但 Release Artifact job 因没有 tag/workflow_dispatch 而跳过，属于正常条件流程，不是产物已验证。
- 当前冻结二进制外部证据绑定 `6a7bb98`，与最新 `56189b1` **不一致**，需重跑。
- 原始 `spec/tool-v1.md` 文件标头仍写 Proposed（源码已实现）；`spec/plugin-abi.md` 第 9 节开头仍声称“Native only”；README 开头仍保留旧里程碑状态与测试数字。公共入口不能带这些明显冲突。
- 版本从 0.x 改 3.0 prerelease 后，插件 manifest `plugin.version` 与产品 version、官方 stdlib version、协议 schema version 并不是同一个版本轴；必须在发布和升级说明中画清楚。
- 第三方 GitHub/GitLab/Gitea 源、工具二进制和 Provider SHA256 只能验证内容完整性，不提供出版者身份保证；不能把它当签名来源认证。

## 6.1 实施状态（3.0.0-beta.1）

| 条目 | 状态 | 落点 |
|---|---|---|
| REL-01 版本线迁移 | 已实施 | workspace/CLI/Analysis `3.0.0-beta.1`；`project.marqdo` 强制校验；工具预发布 SemVer 正负测试 |
| REL-02 支持矩阵与 strict 策略 | 已实施 | Linux x86_64 支持；macOS/Windows 实验性不声称；strict 同 SHA VM 证据或 fail-closed 策略记录在 STATUS |
| REL-03 发行归档与冻结二进制 | 已实施 | `tools/release/package.py`、`tools/release/smoke.py`；归档含 CLI+worker、SOURCE_SHA、RELEASE_INFO、KNOWN_LIMITATIONS |
| REL-04 开发者入口文档 | 已实施 | `docs/developers/`、修正后的第三方接入手册、spec 状态清理 |
| REL-05 Beta RC 审批 | 已实施 | `tools/ci/release_evidence.py --profile beta`、`v3.0.0-beta.1` tag/prerelease、rollback 与已知限制 |

## 7. 最终判定公式

Beta = `同 SHA 主 CI PASS` + `同 SHA 冻结二进制外部扩展 PASS` + `Linux 安装包 PASS` + `版本兼容 PASS` + `清晰支持矩阵` + `严格安全门禁（G3 实证或 strict fail-closed 禁用）` + `维护者批准`。

Stable = 上述条件 + 所承诺功能的完整安全/性能门禁 + 公共接口与迁移政策冻结。

LTS = Stable + 已经历真实生态版本迭代 + 可持续的安全补丁/兼容性维护承诺。
