
# @exec format
## 字节长度

`params(文本)`

**点 = text.codepoints(文本)**

**总字节 = 0**

**下标 = 0**

`while 下标 < 长度(点)`

**码点 = 点[下标]**

`if 码点 < 128`

**总字节 = 总字节 + 1**

`else`
`if 码点 < 2048`

**总字节 = 总字节 + 2**

`else`
`if 码点 < 65536`

**总字节 = 总字节 + 3**

`else`
`if true`

**总字节 = 总字节 + 4**

`end`
`end`
`end`
`end`
**下标 = 下标 + 1**

`end`
`return 总字节`

## 显示

`params(值)`

按语言值语义格式化，用于与内置 打印 的输出对照。
覆盖 Unit/None/Bool/Int/Float/Text/List/Map/Table/Formula/Code/
FunctionRef/PluginHandle/Memory/Resource。

**类别 = 类型(值)**

`if 类别 == "Unit"`

`return "Unit"`

`else`
`if 类别 == "None"`

`return "None"`

`else`
`if 类别 == "Text"`

`return 值`

`else`
`if 类别 == "List"`

`return 列表显示(值)`

`else`
`if 类别 == "Map"`

`return 映射显示(值)`

`else`
`if 类别 == "Table"`

`return 表格显示(值)`

`else`
`if 类别 == "Formula"`

`return "Formula(" + 转义文本(值.latex) + ")"`

`else`
`if 类别 == "Code"`

`return "Code(" + 转义文本(值.language) + ", " + 文本(字节长度(值.source)) + " bytes)"`

`else`
`if 类别 == "FunctionRef"`

`return "FunctionRef(" + 值.module + "::" + 值.name + ")"`

`else`
`if 类别 == "PluginHandle"`

`return "PluginHandle(" + 值.plugin + "::" + 值.export + ")"`

`else`
`if 类别 == "Memory"`

`return "Memory"`

`else`
`if 类别 == "Resource"`

`return "Resource"`

`else`
`if true`

`return 文本(值)`

`end`
`end`
`end`
`end`
`end`
`end`
`end`
`end`
`end`
`end`
`end`
`end`
`end`
## 列表显示

`params(列表)`

**结果 = "["**

**下标 = 0**

`while 下标 < 长度(列表)`

`if 下标 > 0`

**结果 = 结果 + ", "**

`else`
`if true`

**忽略 = 0**

`end`
`end`
**结果 = 结果 + 值显示(列表[下标])**

**下标 = 下标 + 1**

`end`
`return 结果 + "]"`

## 映射显示

`params(映射)`

**键 = map.keys(映射)**

**结果 = "{"**

**下标 = 0**

`while 下标 < 长度(键)`

`if 下标 > 0`

**结果 = 结果 + ", "**

`else`
`if true`

**忽略 = 0**

`end`
`end`
**当前键 = 键[下标]**

**结果 = 结果 + 当前键 + ": " + 值显示(map.get(映射, 当前键))**

**下标 = 下标 + 1**

`end`
`return 结果 + "}"`

## 表格显示

`params(表格)`

**列 = table.columns(表格)**

**结果 = "Table[" + 连接文本(列, " | ") + "]{"**

**行号 = 0**

`while 行号 < 长度(表格)`

`if 行号 > 0`

**结果 = 结果 + "; "**

`else`
`if true`

**忽略 = 0**

`end`
`end`
**行 = table.row(表格, 行号)**

**单元格 = []**

**列号 = 0**

`while 列号 < 长度(行)`

**单元格 = list.append(单元格, 显示(行[列号]))**

**列号 = 列号 + 1**

`end`
**结果 = 结果 + 连接文本(单元格, " | ")**

**行号 = 行号 + 1**

`end`
`return 结果 + "}"`

## 连接文本

`params(列表, 分隔)`

**结果 = ""**

**下标 = 0**

`while 下标 < 长度(列表)`

`if 下标 > 0`

**结果 = 结果 + 分隔**

`else`
`if true`

**忽略 = 0**

`end`
`end`
**结果 = 结果 + 列表[下标]**

**下标 = 下标 + 1**

`end`
`return 结果`

## 值显示

`params(值)`

列表/映射/表格内部使用 repr 语义：Text 带引号并转义，其它值递归显示。

**类别 = 类型(值)**

`if 类别 == "Text"`

`return 转义文本(值)`

`else`
`if true`

`return 显示(值)`

`end`
`end`
## 转义文本

`params(文本)`

**点 = text.codepoints(文本)**

**`结果 = "\""`**

**下标 = 0**

`while 下标 < 长度(点)`

**码点 = 点[下标]**

`if 码点 == 34`

**`结果 = 结果 + "\\\""`**

`else`
`if 码点 == 92`

**`结果 = 结果 + "\\\\"`**

`else`
`if 码点 == 10`

**`结果 = 结果 + "\\n"`**

`else`
`if 码点 == 9`

**`结果 = 结果 + "\\t"`**

`else`
`if 码点 == 13`

**`结果 = 结果 + "\\r"`**

`else`
`if true`

**结果 = 结果 + text.from_codepoint(码点)**

`end`
`end`
`end`
`end`
`end`
`end`
**下标 = 下标 + 1**

`end`
``return 结果 + "\""``
