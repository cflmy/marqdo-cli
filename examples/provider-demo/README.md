# Provider v2 example (`demo.sensor`)

A resource provider is an **external** process/library pair: the CLI ships
`marqdo-provider-worker`, which loads this C ABI v1 library and speaks the
framed resource protocol. Nothing about `demo.sensor` lives in the Marqdo core,
and selecting the provider is not authorization.

```bash
./examples/provider-demo/build.sh          # cc + rustc needed for the host triple
marqdo run examples/provider-demo/demo.mq.md \
  --provider examples/provider-demo/build/provider.json \
  --allow-provider demo.sensor
# => 5
```

Negative checks (all must fail before the resource call succeeds):

```bash
# No grant:
marqdo run examples/provider-demo/demo.mq.md \
  --provider examples/provider-demo/build/provider.json
# A missing or non-sibling helper is fail-closed, never a PATH fallback.

# Unknown provider manifest version:
marqdo run examples/provider-demo/demo.mq.md --provider examples/provider-demo/provider.json
```

`provider.json` in this directory is a template with a zero digest; only the
generated `build/provider.json` names the real artifact and is runnable.
