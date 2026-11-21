
# @exec json
## 嵌套深度

`params(文本)`

按未转义字符串之外的 {/[ 计算最大嵌套深度。

**点 = text.codepoints(文本)**

**下标 = 0**

**深度 = 0**

**最大 = 0**

**字符串内 = false**

**转义 = false**

`while 下标 < 长度(点)`

**码点 = 点[下标]**

`if 字符串内`

`if 转义`

**转义 = false**

`else`
`if 码点 == 92`

**转义 = true**

`else`
`if 码点 == 34`

**字符串内 = false**

`else`
`if true`

**忽略 = 0**

`end`
`end`
`end`
`end`
`else`
`if 码点 == 34`

**字符串内 = true**

`else`
`if 码点 == 123 or 码点 == 91`

**深度 = 深度 + 1**

`if 深度 > 最大`

**最大 = 深度**

`else`
`if true`

**忽略 = 0**

`end`
`end`
`else`
`if 码点 == 125 or 码点 == 93`

`if 深度 > 0`

**深度 = 深度 - 1**

`else`
`if true`

**忽略 = 0**

`end`
`end`
`else`
`if true`

**忽略 = 0**

`end`
`end`
`end`
`end`
`end`
**下标 = 下标 + 1**

`end`
`return 最大`

## 解析

`params(文本)`

解析一个完整的 JSON 文本。值之后只允许空白；多余输入返回
json.trailing，嵌套超过 128 层返回 json.depth。成功结果只保留
{ok, value}，不暴露内部游标。

**点 = text.codepoints(文本)**

`if 嵌套深度(文本) > 128`

`return 失败("json.depth", "嵌套深度超过 128")`

`end`
**结果 = 解析值(点, 0)**

`if not 结果.ok`

`return 结果`

`end`
**位置 = 跳过空白(点, 结果.next)**

`if 位置 != 长度(点)`

`return 失败("json.trailing", "值后存在多余输入")`

`end`
`return {ok: true, value: 结果.value}`

## 成功值

`params(值, 下一位置)`

`return {ok: true, value: 值, next: 下一位置}`

## 失败

`params(代码, 消息)`

`return {ok: false, code: 代码, message: 消息}`

## 是空白

`params(码点)`

`return 码点 == 32 or 码点 == 9 or 码点 == 10 or 码点 == 13`

## 是数字

`params(码点)`

`return 码点 >= 48 and 码点 <= 57`

## 跳过空白

`params(点, 位置)`

**当前 = 位置**

`while 当前 < 长度(点) and 是空白(点[当前])`

**当前 = 当前 + 1**

`end`
`return 当前`

## 匹配

`params(点, 位置, 文本)`

**目标 = text.codepoints(文本)**

**下标 = 0**

**结果 = true**

`while 下标 < 长度(目标) and 结果`

`if 位置 + 下标 >= 长度(点)`

**结果 = false**

`else`
`if 点[位置 + 下标] != 目标[下标]`

**结果 = false**

`else`
`if true`

**继续 = 0**

`end`
`end`
`end`
**下标 = 下标 + 1**

`end`
`return 结果`

## 片段

`params(点, 起点, 终点)`

**结果 = ""**

**下标 = 起点**

`while 下标 < 终点`

**结果 = 结果 + text.from_codepoint(点[下标])**

**下标 = 下标 + 1**

`end`
`return 结果`

## 解析数字

`params(点, 位置)`

JSON 数字：可选负号、整数位、可选小数位、可选指数；含 ./e 返回 Float。

**开始 = 位置**

**当前 = 位置**

`if text.from_codepoint(点[当前]) == "-"`

**当前 = 当前 + 1**

`else`
`if true`

**非负 = 0**

`end`
`end`
**整数位 = 0**

`while 当前 < 长度(点) and 是数字(点[当前])`

**整数位 = 整数位 + 1**

**当前 = 当前 + 1**

`end`
**有小数 = false**

**小数位 = 0**

`if 当前 < 长度(点) and text.from_codepoint(点[当前]) == "."`

**有小数 = true**

**当前 = 当前 + 1**

`while 当前 < 长度(点) and 是数字(点[当前])`

**小数位 = 小数位 + 1**

**当前 = 当前 + 1**

`end`
`else`
`if true`

**忽略 = 0**

`end`
`end`
**指数位 = 0**

**指数标记 = false**

`if 当前 < 长度(点) and (text.from_codepoint(点[当前]) == "e" or text.from_codepoint(点[当前]) == "E")`

**有小数 = true**

**指数标记 = true**

**当前 = 当前 + 1**

`if 当前 < 长度(点) and (text.from_codepoint(点[当前]) == "+" or text.from_codepoint(点[当前]) == "-")`

**当前 = 当前 + 1**

`else`
`if true`

**忽略指数符号 = 0**

`end`
`end`
`while 当前 < 长度(点) and 是数字(点[当前])`

**指数位 = 指数位 + 1**

**当前 = 当前 + 1**

`end`
`else`
`if 整数位 == 0`

`return 失败("json.invalid_number", "数字缺少整数位")`

`else`
`if 有小数 and not 指数标记 and 小数位 == 0`

`return 失败("json.invalid_number", "小数缺少位数")`

`else`
`if 指数标记 and 指数位 == 0`

`return 失败("json.invalid_number", "指数缺少位数")`

`end`
`end`
`end`
`end`
**原文 = 片段(点, 开始, 当前)**

`if 有小数`

`return 成功值(小数(原文), 当前)`

`end`
`return 成功值(整数(原文), 当前)`

## 十六进制值

`params(码点)`

`if 码点 >= 48 and 码点 <= 57`

`return 码点 - 48`

`else`
`if 码点 >= 65 and 码点 <= 70`

`return 码点 - 55`

`else`
`if 码点 >= 97 and 码点 <= 102`

`return 码点 - 87`

`end`
`end`
`end`
`return 0 - 1`

## 解析字符串

`params(点, 位置)`

**当前 = 位置 + 1**

**结果 = ""**

**完成 = false**

**错误 = ""**

`while 当前 < 长度(点) and not 完成 and 错误 == ""`

**码点 = 点[当前]**

`if 码点 == 34`

**完成 = true**

**当前 = 当前 + 1**

`else`
`if 码点 == 92`

`if 当前 + 1 >= 长度(点)`

**错误 = "json.invalid_escape"**

`else`
`if true`

**转义码点 = 点[当前 + 1]**

`if 转义码点 == 34`

**结果 = 结果 + text.from_codepoint(34)**

**当前 = 当前 + 2**

`else`
`if 转义码点 == 92`

**结果 = 结果 + text.from_codepoint(92)**

**当前 = 当前 + 2**

`else`
`if 转义码点 == 47`

**结果 = 结果 + text.from_codepoint(47)**

**当前 = 当前 + 2**

`else`
`if 转义码点 == 98`

**结果 = 结果 + text.from_codepoint(8)**

**当前 = 当前 + 2**

`else`
`if 转义码点 == 102`

**结果 = 结果 + text.from_codepoint(12)**

**当前 = 当前 + 2**

`else`
`if 转义码点 == 110`

**结果 = 结果 + text.from_codepoint(10)**

**当前 = 当前 + 2**

`else`
`if 转义码点 == 114`

**结果 = 结果 + text.from_codepoint(13)**

**当前 = 当前 + 2**

`else`
`if 转义码点 == 116`

**结果 = 结果 + text.from_codepoint(9)**

**当前 = 当前 + 2**

`else`
`if 转义码点 == 117`

`if 当前 + 5 >= 长度(点)`

**错误 = "json.invalid_escape"**

`else`
`if true`

**数字 = 十六进制值(点[当前 + 2]) * 4096 + 十六进制值(点[当前 + 3]) * 256 + 十六进制值(点[当前 + 4]) * 16 + 十六进制值(点[当前 + 5])**

`if 数字 < 0 or (数字 >= 55296 and 数字 <= 57343)`

**错误 = "json.invalid_escape"**

`else`
`if true`

**结果 = 结果 + text.from_codepoint(数字)**

**当前 = 当前 + 6**

`end`
`end`
`end`
`end`
`else`
`if true`

**错误 = "json.invalid_escape"**

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
`else`
`if true`

**结果 = 结果 + text.from_codepoint(码点)**

**当前 = 当前 + 1**

`end`
`end`
`end`
`end`
`if 错误 != ""`

`return 失败("json.invalid_string", "字符串转义无效")`

`else`
`if not 完成`

`return 失败("json.unexpected_eof", "字符串未闭合")`

`end`
`end`
`return 成功值(结果, 当前)`

## 解析对象

`params(点, 位置)`

`return 解析对象项(点, 位置 + 1, {})`

## 解析对象项

`params(点, 位置, 已解析)`

**位置 = 跳过空白(点, 位置)**

`if 位置 >= 长度(点)`

`return 失败("json.unexpected_eof", "对象未闭合")`

`end`
**当前 = text.from_codepoint(点[位置])**

`if 当前 == "}"`

`return 成功值(已解析, 位置 + 1)`

`end`
**键结果 = 解析字符串(点, 位置)**

`if not 键结果.ok`

`return 键结果`

`end`
**键 = 键结果.value**

**位置 = 跳过空白(点, 键结果.next)**

`if 位置 >= 长度(点)`

`return 失败("json.unexpected_eof", "对象缺少冒号")`

`else`
`if text.from_codepoint(点[位置]) != ":"`

`return 失败("json.invalid_object", "对象缺少冒号")`

`end`
`end`
**位置 = 跳过空白(点, 位置 + 1)**

**值结果 = 解析值(点, 位置)**

`if not 值结果.ok`

`return 值结果`

`end`
**新的已解析 = map.set(已解析, 键, 值结果.value)**

**位置 = 跳过空白(点, 值结果.next)**

`if 位置 >= 长度(点)`

`return 失败("json.unexpected_eof", "对象未闭合")`

`end`
**分隔 = text.from_codepoint(点[位置])**

`if 分隔 == ","`

`return 解析对象项(点, 位置 + 1, 新的已解析)`

`else`
`if 分隔 == "}"`

`return 成功值(新的已解析, 位置 + 1)`

`else`
`if true`

`return 失败("json.invalid_object", "对象缺少逗号或 }")`

`end`
`end`
`end`
## 解析值

`params(点, 位置)`

**位置 = 跳过空白(点, 位置)**

`if 位置 >= 长度(点)`

`return 失败("json.unexpected_eof", "需要值")`

`end`
**首 = text.from_codepoint(点[位置])**

**首码点 = 点[位置]**

`if 首码点 == 34`

`return 解析字符串(点, 位置)`

`else`
`if 首码点 == 123`

`return 解析对象(点, 位置)`

`else`
`if 首 == "["`

`return 解析数组(点, 位置)`

`else`
`if 首 == "-" or 是数字(点[位置])`

`return 解析数字(点, 位置)`

`else`
`if 首 == "t"`

`if 匹配(点, 位置, "true")`

`return 成功值(true, 位置 + 4)`

`else`
`if true`

`return 失败("json.invalid_literal", "无效字面量")`

`end`
`end`
`else`
`if 首 == "f"`

`if 匹配(点, 位置, "false")`

`return 成功值(false, 位置 + 5)`

`else`
`if true`

`return 失败("json.invalid_literal", "无效字面量")`

`end`
`end`
`else`
`if 首 == "n"`

`if 匹配(点, 位置, "null")`

`return 成功值(None, 位置 + 4)`

`else`
`if true`

`return 失败("json.invalid_literal", "无效字面量")`

`end`
`end`
`else`
`if true`

`return 失败("json.unexpected_token", "不支持的值")`

`end`
`end`
`end`
`end`
`end`
`end`
`end`
`end`
## 解析数组

`params(点, 位置)`

`return 解析数组项(点, 位置 + 1, [])`

## 解析数组项

`params(点, 位置, 已解析)`

**位置 = 跳过空白(点, 位置)**

`if 位置 >= 长度(点)`

`return 失败("json.unexpected_eof", "数组未闭合")`

`end`
**当前 = text.from_codepoint(点[位置])**

`if 当前 == "]"`

`return 成功值(已解析, 位置 + 1)`

`end`
**项 = 解析值(点, 位置)**

`if not 项.ok`

`return 项`

`end`
**新的已解析 = list.append(已解析, 项.value)**

**位置 = 跳过空白(点, 项.next)**

`if 位置 >= 长度(点)`

`return 失败("json.unexpected_eof", "数组未闭合")`

`end`
**分隔 = text.from_codepoint(点[位置])**

`if 分隔 == ","`

`return 解析数组项(点, 位置 + 1, 新的已解析)`

`else`
`if 分隔 == "]"`

`return 成功值(新的已解析, 位置 + 1)`

`else`
`if true`

`return 失败("json.invalid_array", "数组缺少逗号或 ]")`

`end`
`end`
`end`
## 格式化文本

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

## 格式化列表

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
**结果 = 结果 + 格式化(列表[下标])**

**下标 = 下标 + 1**

`end`
`return 结果 + "]"`

## 格式化对象

`params(映射)`

**键列表 = map.keys(映射)**

**结果 = "{"**

**下标 = 0**

`while 下标 < 长度(键列表)`

`if 下标 > 0`

**结果 = 结果 + ", "**

`else`
`if true`

**忽略 = 0**

`end`
`end`
**当前键 = 键列表[下标]**

**结果 = 结果 + 格式化文本(当前键) + ": " + 格式化(map.get(映射, 当前键))**

**下标 = 下标 + 1**

`end`
`return 结果 + "}"`

## 格式化

`params(值)`

**类别 = 类型(值)**

`if 类别 == "Text"`

`return 格式化文本(值)`

`else`
`if 类别 == "None"`

`return "null"`

`else`
`if 类别 == "Bool" or 类别 == "Int" or 类别 == "Float"`

`return 文本(值)`

`else`
`if 类别 == "List"`

`return 格式化列表(值)`

`else`
`if 类别 == "Map"`

`return 格式化对象(值)`

`end`
`end`
`end`
`end`
`end`
`return "null"`

## 解析多个

`params(文本)`

解析连续多个 JSON 值（空白分隔），返回 {ok:true, value:List}。
任一项失败时返回该项的错误，不返回部分结果。

**点 = text.codepoints(文本)**

**位置 = 跳过空白(点, 0)**

**结果 = []**

`while 位置 < 长度(点)`

**项 = 解析值(点, 位置)**

`if not 项.ok`

`return 项`

`end`
**结果 = list.append(结果, 项.value)**

**位置 = 跳过空白(点, 项.next)**

`end`
`return {ok: true, value: 结果}`
