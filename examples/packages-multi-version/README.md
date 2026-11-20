# Multi-version package example

Two directories declare the same package name `geometry` at versions `1.0.0`
and `2.0.0`. They receive different package-instance ids and different
`ModuleId`s; importing both aliases is not an implicit override.

```sh
marqdo run examples/packages-multi-version/index.mq.md
marqdo lock
```

`marqdo lock` must be run from the example directory because it discovers the
project from the current working directory:

```sh
(cd examples/packages-multi-version && ../../target/debug/marqdo lock)
```

The lock records both content-addressed instances; no version silently wins.
