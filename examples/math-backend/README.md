# Example optional math ABI backend

This native plugin shows how a hot-path math algorithm can be supplied by an
explicitly selected and trusted ABI backend after the core `math.*` call surface was
removed (ADR-0061). It is optional: the pure `../math-library/` package works
without it.

```bash
./build.sh
marqdo check demo.mq.md --plugin build
marqdo run demo.mq.md --plugin build --trust-native-plugin --allow-plugin math.backend
# 42
marqdo run demo.mq.md --plugin build          # permission.denied (no grant)
```

`math.compute` is the capability category the backend requests; the core does not
ship any math solver or evaluator.
