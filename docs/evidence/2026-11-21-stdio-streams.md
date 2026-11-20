# Evidence: stdio stdin/stderr and stdlib streams

Date: 2026-11-21 (Asia/Shanghai)
Repository: `marqdo-cli`
Decision record: [ADR-0040](../adr/0040-stdio-stdin-stderr-and-stdlib-streams.md)

## Local gate

```text
cargo fmt --all -- --check
cargo test --workspace --no-fail-fast
```

Both ran locally in the writable `.cargo-home`. The workspace suite passed.

## Covered slices

* host: pure-mode stdin chunking and EOF, separate stdout/stderr buffers,
  strict refusal of stdin/stderr, closed-handle errors, output budget;
* CLI: piped stdin through `std:io/console.读取行`, separate JSON `stdout` and
  `stderr`, EOF without a panic;
* stdlib: `std::io/stream.写全部` loops on `{ok,count}`, rejects zero progress,
  `写文本` grows `Memory` from the UTF-8 byte length and checks close;
  `std::text/utf8.解码` rejects truncated, invalid-continuation, overlong,
  surrogate and out-of-range scalar input.

## Not covered

Directories, process/device resources, DNS/TLS/UDP, seek/mmap, async I/O,
interactive terminal line editing, Windows/macOS native lanes, and remote CI.
