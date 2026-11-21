
# @exec csv
## 成功

`params(值)`

`return {ok: true, value: 值}`

## 失败

`params(代码, 消息)`

`return {ok: false, code: 代码, message: 消息}`

## 解析行

`params(文本)`

解析单行 CSV：逗号分隔、双引号包裹、"" 表示一个字面双引号。
不处理换行、多行字段或首尾空白。未闭合引号和未加引号字段中的双引号
返回可恢复错误。

**点 = text.codepoints(文本)**

**下标 = 0**

**字段 = ""**

**字段表 = []**

**引号内 = false**

**错误 = ""**

`while 下标 < 长度(点) and 错误 == ""`

**字符 = text.from_codepoint(点[下标])**

`if 引号内 and 字符 == "\""`

`if 下标 + 1 < 长度(点) and text.from_codepoint(点[下标 + 1]) == "\""`

**`字段 = 字段 + "\""`**

**下标 = 下标 + 2**

`else`
`if true`

**引号内 = false**

**下标 = 下标 + 1**

`end`
`end`
`else`
`if 引号内`

**字段 = 字段 + 字符**

**下标 = 下标 + 1**

`else`
`if 字符 == "\""`

`if 字段 == ""`

**引号内 = true**

**下标 = 下标 + 1**

`else`
`if true`

**错误 = "csv.invalid_quote"**

`end`
`end`
`else`
`if 字符 == ","`

**字段表 = list.append(字段表, 字段)**

**字段 = ""**

**下标 = 下标 + 1**

`else`
`if true`

**字段 = 字段 + 字符**

**下标 = 下标 + 1**

`end`
`end`
`end`
`end`
`end`
`end`
`if 错误 != ""`

`return 失败(错误, "未加引号字段中出现双引号")`

`else`
`if 引号内`

`return 失败("csv.unterminated_quote", "引号未闭合")`

`end`
`end`
`return 成功(list.concat(字段表, [字段]))`

## 格式化字段

`params(文本)`

含逗号、双引号、换行或回车的字段加引号，内部双引号写成 ""。

**点 = text.codepoints(文本)**

**下标 = 0**

**需要引号 = false**

`while 下标 < 长度(点) and not 需要引号`

**码点 = 点[下标]**

`if 码点 == 34 or 码点 == 44 or 码点 == 10 or 码点 == 13`

**需要引号 = true**

`else`
`if true`

**忽略 = 0**

`end`
`end`
**下标 = 下标 + 1**

`end`
`if not 需要引号`

`return 文本`

`end`
**`结果 = "\""`**

**下标 = 0**

`while 下标 < 长度(点)`

`if 点[下标] == 34`

**`结果 = 结果 + "\"\""`**

`else`
`if true`

**结果 = 结果 + text.from_codepoint(点[下标])**

`end`
`end`
**下标 = 下标 + 1**

`end`
``return 结果 + "\""``

## 格式化行

`params(列表)`

**结果 = ""**

**下标 = 0**

`while 下标 < 长度(列表)`

`if 下标 > 0`

**结果 = 结果 + ","**

`else`
`if true`

**忽略 = 0**

`end`
`end`
**结果 = 结果 + 格式化字段(文本(列表[下标]))**

**下标 = 下标 + 1**

`end`
`return 结果`

## 解析

`params(文本)`

解析多行 CSV（逗号分隔、双引号、"" 转义、CRLF/LF 行尾，引号内可含
换行）。返回 {ok:true, value:List<List<Text>>}；未闭合引号和不合法
引号返回可恢复错误。

**点 = text.codepoints(文本)**

**下标 = 0**

**字段 = ""**

**当前行 = []**

**行 = []**

**引号内 = false**

**错误 = ""**

`while 下标 < 长度(点) and 错误 == ""`

**字符 = text.from_codepoint(点[下标])**

`if 引号内 and 字符 == "\""`

`if 下标 + 1 < 长度(点) and text.from_codepoint(点[下标 + 1]) == "\""`

**`字段 = 字段 + "\""`**

**下标 = 下标 + 2**

`else`
`if true`

**引号内 = false**

**下标 = 下标 + 1**

`end`
`end`
`else`
`if 引号内`

**字段 = 字段 + 字符**

**下标 = 下标 + 1**

`else`
`if 字符 == "\""`

`if 字段 == ""`

**引号内 = true**

**下标 = 下标 + 1**

`else`
`if true`

**错误 = "csv.invalid_quote"**

`end`
`end`
`else`
`if 字符 == ","`

**当前行 = list.append(当前行, 字段)**

**字段 = ""**

**下标 = 下标 + 1**

`else`
`if 字符 == "\n"`

**当前行 = list.append(当前行, 字段)**

**行 = list.append(行, 当前行)**

**当前行 = []**

**字段 = ""**

**下标 = 下标 + 1**

`else`
`if 字符 == "\r"`

**下标 = 下标 + 1**

`else`
`if true`

**字段 = 字段 + 字符**

**下标 = 下标 + 1**

`end`
`end`
`end`
`end`
`end`
`end`
`end`
`end`
`if 错误 != ""`

`return 失败(错误, "未加引号字段中出现双引号")`

`else`
`if 引号内`

`return 失败("csv.unterminated_quote", "引号未闭合")`

`else`
`if 字段 != "" or 长度(当前行) > 0`

**当前行 = list.append(当前行, 字段)**

**行 = list.append(行, 当前行)**

`end`
`end`
`end`
`return 成功(行)`

## 格式化

`params(行列表)`

**结果 = ""**

**行号 = 0**

`while 行号 < 长度(行列表)`

`if 行号 > 0`

**结果 = 结果 + "\n"**

`else`
`if true`

**忽略 = 0**

`end`
`end`
**结果 = 结果 + 格式化行(行列表[行号])**

**行号 = 行号 + 1**

`end`
`return 结果`
