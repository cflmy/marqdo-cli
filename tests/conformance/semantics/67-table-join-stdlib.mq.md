## @exec 连接

`params(左: Table, 右: Table, 左列: Int, 右列: Int)`

按 equality 连接两张表；输出表头是左表头接右表头，原表不变。

`表头 = list.concat(table.columns(左), table.columns(右))`

`结果行 = []`

`左行号 = 0`

`while 左行号 < 长度(左)`

`左行 = table.row(左, 左行号)`

`键 = 左行[左列]`

`右行号 = 0`

`while 右行号 < 长度(右)`

`右行 = table.row(右, 右行号)`

`if 右行[右列] == 键`

`结果行 = list.append(结果行, list.concat(左行, 右行))`

`else`

`继续 = 0`

`end`

`右行号 = 右行号 + 1`

`end`

`左行号 = 左行号 + 1`

`end`

`return 表(表头, 结果行)`

## @exec main

`左表 = 表(["名称", "分数"], [["甲", 87], ["乙", 92], ["丙", 71]])`

`右表 = 表(["名称", "等级"], [["乙", "A"], ["甲", "B"], ["丁", "C"]])`

`return [左表, 右表, 连接(左表, 右表, 0, 0)]`
