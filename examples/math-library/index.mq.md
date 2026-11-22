# @knowledge 数学算法

以下函数是纯 Marqdo 算法，不依赖已移除的宿主 `math.*` 内核。

## @exec 最大公约数

欧几里得算法，非负整数输入。参数以 `甲`、`乙` 传入，返回最大公约数。

`a = 甲`

`b = 乙`

`while b != 0`

`余数 = a % b`

`a = b`

`b = 余数`

`end`

`return a`
