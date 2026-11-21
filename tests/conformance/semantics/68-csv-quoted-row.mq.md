## @exec 解析

`params(文本: Text)`

单行 CSV：逗号分隔、双引号包裹、"" 表示一个字面双引号。
不处理换行、多行字段或首尾空白。

`点 = text.codepoints(文本)`

`下标 = 0`

`字段 = ""`

`字段表 = []`

`引号内 = false`

`while 下标 < 长度(点)`

`字符 = text.from_codepoint(点[下标])`

`if 引号内 and 字符 == "\""`

`if 下标 + 1 < 长度(点) and text.from_codepoint(点[下标 + 1]) == "\""`

`字段 = 字段 + "\""`

`下标 = 下标 + 2`

`else`

`引号内 = false`

`下标 = 下标 + 1`

`end`

`else`

`if 引号内`

`字段 = 字段 + 字符`

`下标 = 下标 + 1`

`else`

`if 字符 == "\""`

`引号内 = true`

`下标 = 下标 + 1`

`else`

`if 字符 == ","`

`字段表 = list.append(字段表, 字段)`

`字段 = ""`

`下标 = 下标 + 1`

`else`

`字段 = 字段 + 字符`

`下标 = 下标 + 1`

`end`

`end`

`end`

`end`

`end`

`return list.concat(字段表, [字段])`

## @exec main

`return [解析("甲,\"乙,丙\",\"说\"\"好\""), 解析("a,,c"), 解析("\"\""), 解析("\"a\"\"b\",c")]`
