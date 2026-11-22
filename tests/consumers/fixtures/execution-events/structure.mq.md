# @knowledge Execution Events fixture C

## @exec g

`params(n: Int)`

`return n + 1`

## @exec f

`params(n: Int)`

`if n > 0`

`return g(n=n)`

`end`

`return n`

## @exec main

`i = 0`

`while i < 2`

`i = i + 1`

`end`

`a = f(n=2)`

`b = f(n=0)`

`return a + b`
