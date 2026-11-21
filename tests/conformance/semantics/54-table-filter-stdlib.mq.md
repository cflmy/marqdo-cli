## @exec 过滤

`params(数据: Table, 阈值: Int)`

只用 table 观察原语和 list 构造原语重建一个新表；输入表不被修改。

`列 = table.columns(数据)`

`结果行 = []`

`行号 = 0`

`while 行号 < 长度(数据)`

`分数 = table.cell(数据, 行号, 1)`

`if 分数 > 阈值`

`结果行 = list.append(结果行, table.row(数据, 行号))`

`end`

`行号 = 行号 + 1`

`end`

`return 表(列, 结果行)`

## @exec main

`原表 = 表(["名称", "分数"], [["甲", 87], ["乙", 92], ["丙", 71]])`

`结果 = 过滤(原表, 80)`

`return [原表, 结果]`
