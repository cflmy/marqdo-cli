# External kernel provider example

`mocklang` is not built into `marqdo-foreign`. The descriptor registers it at
run time without changing interpreter or host source:

```sh
marqdo run examples/kernel-mock/demo.mq.md \
  --kernel examples/kernel-mock/kernel.json \
  --allow-foreign
```

The `mocklang` executable receives the source file path as argv[1] and the
versioned request envelope on stdin. For `foreign.call` it must print exactly
one typed wire value on stdout. A missing provider or wire version is
`kernel.unavailable`; the `Code` value itself remains inert until the explicit
call.
