## @exec 分割

`params(文本: Text)`

这是 CSV 单行拆分的最小实现：只处理逗号分隔，不支持引号或转义；空字段保留。

`结果 = []`

`当前 = ""`

`点 = text.codepoints(文本)`

`下标 = 0`

`while 下标 < 长度(点)`

`字符 = text.from_codepoint(点[下标])`

`if 字符 == ","`

`结果 = list.append(结果, 当前)`

`当前 = ""`

`else`

`当前 = 当前 + 字符`

`end`

`下标 = 下标 + 1`

`end`

`return list.concat(结果, [当前])`

## @exec main

`return [分割("甲,乙,,丙"), 分割(""), 分割("a,b")]`
