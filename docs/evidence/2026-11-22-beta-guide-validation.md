# Third-party integration guide validation — `3.0.0-beta.1`

Date: 2026-11-22. Product: `3.0.0-beta.1`. Source SHA:
`2c7c4f31e7a388d3fcfa717a77067e696ed13854`.

Goal: every command/snippet in
[`MARQDO_THIRD_PARTY_INTEGRATION_GUIDE.md`](../MARQDO_THIRD_PARTY_INTEGRATION_GUIDE.md)
and [`developers/`](../developers/README.md) must work from the released
archive without reading the core source or rebuilding the CLI.

## Binary and archive

```text
archive  dist/marqdo-3.0.0-beta.1-x86_64-unknown-linux-gnu.tar.gz
sha256   507e4cd358f75f517433935ce90d5fc2eb5cecda4f19121a68b136c83b15f317
marqdo   sha256 062cb56e226bee55c21d238e195920caa985e62bb3877ba7b07abcc04dc0fe3f
worker   sha256 7266db75f52e2954b29f8f018d957e1cae3110a239f98dfe1f34b73c90c96ba2
```

The archive was extracted into a fresh directory outside the source tree; all
commands below used only the extracted `marqdo` (and its sibling
`marqdo-provider-worker`). The recorded console log is
[`2026-11-22-beta-guide-validation.log`](2026-11-22-beta-guide-validation.log).

## Executed matrix

| Guide section | Command / flow | Result |
|---|---|---|
| §1 install | `sha256sum -c`, extract, `marqdo --version`, `--help` | PASS: `marqdo 3.0.0-beta.1` |
| §2 pure library | local Git repo with `index.mq.md` + `params`; consumer with pinned `rev`, front-matter `import: quantum: "@quantum"`; `lock`, `sync --locked`, `check --refs`, `call main` | PASS: `lock` writes lock v4; call returns `42` |
| §1.3 `project.marqdo` | project with `marqdo: ">=0.1,<1"` then `check` | PASS (fail-closed): exit 2, `package.marqdo_incompatible` names the running core |
| §3 Tool v1 | independent tool package, `tool install`, `tool list`, dispatch `view index.mq.md --json`, `tool remove` | PASS: install/list/remove 0; dispatch preserves argv/stdout/exit code and receives `MARQDO_BIN` + protocol env vars; tool's exit code 7 propagated |
| §3.1 Analysis v2 | `check --emit`, parse schema/version, producer version | PASS: `marqdo.analysis` v2, `producer.version = 3.0.0-beta.1` |
| §3.2 Trace v2 | `run --trace-file`, compare `source.analysis_digest` with SHA-256 of the Analysis file bytes | PASS: digest matched byte-for-byte; `marqdo.trace` v2, 182 events on the example |
| §4 Plugin ABI | `plugin inspect` static; `check --plugin` without loading; native run with `--trust-native-plugin`; native without trust | PASS: inspect/check/run 0; untrusted native refused with the documented `--trust-native-plugin` error |
| §5 Provider v2 | `run --provider … --allow-provider demo.sensor`, read trace effects; no-grant run; missing helper (smoke) | PASS: provider reads `count=5`; no grant is `permission.denied`; missing sibling helper is `ext.worker_unavailable` with no PATH fallback |
| §6 Foreign kernel | `run --kernel … --allow-foreign`; tampered `digest` | PASS: `42`; tampered digest fails with `kernel.unavailable` integrity error before execution |
| §7 Extensions / lock v4 | project with `extensions.plugins.from=./adapter/plugin.yaml`; `lock`, `inspect --extensions --json`, `run --allow-plugin` | PASS: identity `plugin demo.solver locked x86_64-unknown-linux-gnu`; worker run returns `42`; undeclared grant is unnecessary but a grant is still required |
| §1.4 strict | `inspect --platform --json`; `run --mode strict`; strict with `--trace-file` | PASS on this Linux host: probe reports Bubblewrap available, strict run succeeds; `--trace-file` refused with exit 2 |
| §3 Tool compatibility | stable-only range `>=3.0.0,<4.0.0` on the beta core | PASS (expected rejection): `requires marqdo … this core is 3.0.0-beta.1` |

## Facts verified against the implementation

- Tool install is whole-bundle hashed and re-verified before dispatch; a
  tampered bundle, traversal entrypoint and reserved command are rejected.
- `check --emit` writes Analysis v2 only after a successful check; `--refs` is
  an independent link validation and can delete a previous emit target on
  failure. The guide tells consumers to read the artifact only after exit 0.
- Plugin calls are named-argument only; `--allow-plugin` matches the manifest
  `plugin.name`; artifact target triple must equal the host triple.
- The provider helper is a CLI release artifact resolved only as a sibling; the
  provider package supplies `provider.json` + the C ABI library.
- Extension selection records manifest/artifact identity in lock v4 and never
  grants execution by itself.
- Analysis `producer.version` follows the product version; stdlib/package
  versions stay independent (`0.1.0`).

## Not executed / explicitly unverified

- HTTPS Git clone/fetch: the pure-library flow used a local repository; network
  authentication was not exercised. The 40/64-character lowercase rev rule and
  lock behaviour are code-tested.
- macOS/Windows plugin, provider worker and strict paths: not executed; the
  release claims Linux x86_64 only and `COMPATIBILITY.md` marks them
  unsupported/experimental.
- Provider wrong-target and kernel timeout/abnormal-exit variants: covered by
  the external conformance suite's digests/schema negatives and the spec, not
  by this specific manual run.

Verdict: the handbook and the developer pages are executable as written for the
`3.0.0-beta.1` Linux x86_64 release; no unresolved BLOCKER/MAJOR guide defect
remains from the independent audit.
