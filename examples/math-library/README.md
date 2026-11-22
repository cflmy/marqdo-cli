# Example pure math library

A `.mq.md` package that provides math algorithms with ordinary language
functions only. It needs no host `math.*` builtin, no ABI plugin and no core
change.

```bash
marqdo call ../math-library-demo.mq.md main
# 6
```

For an optional accelerated backend, see `../math-backend/`; the library itself
does not require it.
