# Embedding

## @exec sum_to

`params(n: Int)`

`i = 0`

`total = 0`

`while i <= n`

`total = total + i`

`i = i + 1`

`end`

`return total`

## @exec types

`params(seed: Int)`

`record = {"i": seed, "b": true, "t": "héllo", "l": [1, 2, 3], "n": None}`

`return record`

## @exec recurse

`params(n: Int)`

`if n <= 0`

`return 0`

`end`

`return recurse(n=n - 1)`

## @exec oob

`m = memory.new(1, 1)`

`return m[70000]`

## @exec read_file

`r = resource.open("file", "embedding-probe.txt", ["read"])`

`return r`
