
# @exec regex
## 成功

`params(值)`

`return {ok: true, value: 值}`

## 失败

`params(代码, 消息)`

`return {ok: false, code: 代码, message: 消息}`

## 包含

`params(列表, 值)`

**结果 = false**

**下标 = 0**

`while 下标 < 长度(列表) and not 结果`

`if 列表[下标] == 值`

**结果 = true**

`else`
`if true`

**忽略 = 0**

`end`
`end`
**下标 = 下标 + 1**

`end`
`return 结果`

## 去头

`params(列表)`

**结果 = []**

**下标 = 1**

`while 下标 < 长度(列表)`

**结果 = list.append(结果, 列表[下标])**

**下标 = 下标 + 1**

`end`
`return 结果`

## 设置最后

`params(列表, 值)`

**结果 = []**

**下标 = 0**

`while 下标 < 长度(列表)`

`if 下标 + 1 == 长度(列表)`

**结果 = list.append(结果, 值)**

`else`
`if true`

**结果 = list.append(结果, 列表[下标])**

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

## 是数字

`params(码点)`

`return 码点 >= 48 and 码点 <= 57`

## 字面令牌

`params(码点)`

包含数量词，默认恰好一次。

`return {kind: "literal", value: 码点, neg: false, min: 1, max: 1}`

## 任意令牌

`params()`

`return {kind: "any", value: 0, neg: false, min: 1, max: 1}`

## 类令牌

`params(集合, 否定)`

`return {kind: "class", value: 集合, neg: 否定, min: 1, max: 1}`

## 锚点令牌

`params(方向)`

`return {kind: "anchor", value: 方向, neg: false, min: 1, max: 1}`

## 设置量词

`params(令牌, 码点)`

`if 码点 == 42`

`return {kind: 令牌.kind, value: 令牌.value, neg: 令牌.neg, min: 0, max: -1}`

`else`
`if 码点 == 43`

`return {kind: 令牌.kind, value: 令牌.value, neg: 令牌.neg, min: 1, max: -1}`

`end`
`end`
`return {kind: 令牌.kind, value: 令牌.value, neg: 令牌.neg, min: 0, max: 1}`

## 编译类

`params(点, 位置)`

**当前 = 位置 + 1**

**否定 = false**

`if 当前 < 长度(点) and 点[当前] == 94`

**否定 = true**

**当前 = 当前 + 1**

`else`
`if true`

**忽略否定 = 0**

`end`
`end`
**集合 = []**

**完成 = false**

**错误 = ""**

`while 当前 < 长度(点) and not 完成 and 错误 == ""`

**码点 = 点[当前]**

`if 码点 == 93`

**完成 = true**

**当前 = 当前 + 1**

`else`
`if 码点 == 92`

`if 当前 + 1 >= 长度(点)`

**错误 = "regex.invalid"**

`else`
`if true`

**集合 = list.append(集合, 点[当前 + 1])**

**当前 = 当前 + 2**

`end`
`end`
`else`
`if 当前 + 2 < 长度(点) and 点[当前 + 1] == 45 and 点[当前 + 2] != 93`

**开始 = 码点**

**结束 = 点[当前 + 2]**

`if 开始 > 结束`

**错误 = "regex.invalid"**

`else`
`if true`

**值 = 开始**

`while 值 <= 结束`

**集合 = list.append(集合, 值)**

**值 = 值 + 1**

`end`
**当前 = 当前 + 3**

`end`
`end`
`else`
`if true`

**集合 = list.append(集合, 码点)**

**当前 = 当前 + 1**

`end`
`end`
`end`
`end`
`end`
`if not 完成`

`return 失败("regex.invalid", "字符类未闭合")`

`else`
`if 错误 != ""`

`return 失败(错误, "字符类范围无效")`

`end`
`end`
`return 成功({set: 集合, neg: 否定, next: 当前})`

## 编译

`params(点, 位置)`

**令牌表 = []**

**当前 = 位置**

**错误 = ""**

**代码 = ""**

`while 当前 < 长度(点) and 错误 == ""`

**码点 = 点[当前]**

`if 码点 == 94`

**令牌表 = list.append(令牌表, 锚点令牌("start"))**

**当前 = 当前 + 1**

`else`
`if 码点 == 36`

**令牌表 = list.append(令牌表, 锚点令牌("end"))**

**当前 = 当前 + 1**

`else`
`if 码点 == 91`

**类结果 = 编译类(点, 当前)**

`if not 类结果.ok`

**错误 = 类结果.code**

**代码 = 类结果.message**

`else`
`if true`

**令牌表 = list.append(令牌表, 类令牌(类结果.value.set, 类结果.value.neg))**

**当前 = 类结果.value.next**

`end`
`end`
`else`
`if 码点 == 92`

`if 当前 + 1 >= 长度(点)`

**错误 = "regex.invalid"**

**代码 = "转义缺少字符"**

`else`
`if true`

**令牌表 = list.append(令牌表, 字面令牌(点[当前 + 1]))**

**当前 = 当前 + 2**

`end`
`end`
`else`
`if 码点 == 46`

**令牌表 = list.append(令牌表, 任意令牌())**

**当前 = 当前 + 1**

`else`
`if 码点 == 40 or 码点 == 41 or 码点 == 124`

**错误 = "regex.unsupported"**

**代码 = "不支持分组或选择"**

`else`
`if 码点 == 42 or 码点 == 43 or 码点 == 63`

**错误 = "regex.invalid"**

**代码 = "量词没有前一项"**

`else`
`if true`

**令牌表 = list.append(令牌表, 字面令牌(码点))**

**当前 = 当前 + 1**

`end`
`end`
`end`
`end`
`end`
`end`
`end`
`end`
`while 错误 == "" and 当前 < 长度(点) and (点[当前] == 42 or 点[当前] == 43 or 点[当前] == 63)`

**令牌表 = 设置最后(令牌表, 设置量词(令牌表[长度(令牌表) - 1], 点[当前]))**

**当前 = 当前 + 1**

`while 当前 < 长度(点) and (点[当前] == 42 or 点[当前] == 43 or 点[当前] == 63)`

**错误 = "regex.invalid"**

**代码 = "连续量词无效"**

`end`
`end`
`end`
`if 错误 != ""`

`return 失败(错误, 代码)`

`end`
`return 成功(令牌表)`

## 单字符匹配

`params(令牌, 码点)`

**类别 = 令牌.kind**

`if 类别 == "any"`

`return true`

`else`
`if 类别 == "literal"`

`return 令牌.value == 码点`

`else`
`if 类别 == "class"`

`return 包含(令牌.value, 码点) != 令牌.neg`

`end`
`end`
`end`
`return false`

## 尝试匹配

`params(令牌, 下标, 当前, 点, 位置, 次数, 要求结束)`

**下限 = 当前.min**

**上限 = 当前.max**

`if 次数 >= 下限 and 匹配序列(令牌, 下标 + 1, 点, 位置, 要求结束)`

`return true`

`else`
`if 上限 >= 0 and 次数 >= 上限`

`return false`

`else`
`if 位置 < 长度(点) and 单字符匹配(当前, 点[位置])`

`return 尝试匹配(令牌, 下标, 当前, 点, 位置 + 1, 次数 + 1, 要求结束)`

`end`
`end`
`end`
`return false`

## 匹配序列

`params(令牌, 下标, 点, 位置, 要求结束)`

`if 下标 >= 长度(令牌)`

`if 要求结束`

`return 位置 == 长度(点)`

`end`
`return true`

`end`
**当前 = 令牌[下标]**

`if 当前.kind == "anchor"`

`if 当前.value == "start" and 位置 == 0`

`return 匹配序列(令牌, 下标 + 1, 点, 位置, 要求结束)`

`else`
`if 当前.value == "end" and 位置 == 长度(点)`

`return 匹配序列(令牌, 下标 + 1, 点, 位置, 要求结束)`

`end`
`end`
`return false`

`end`
`return 尝试匹配(令牌, 下标, 当前, 点, 位置, 0, 要求结束)`

## 匹配

`params(模式, 文本)`

编译后要求整个文本匹配。支持 ^、$、.、*、+、?、
[...] 和 \\ 转义；不支持分组和选择。

**编译结果 = 编译(text.codepoints(模式), 0)**

`if not 编译结果.ok`

`return 编译结果`

`end`
`return {ok: true, value: 匹配序列(编译结果.value, 0, text.codepoints(文本), 0, true)}`

## 搜索

`params(模式, 文本)`

**编译结果 = 编译(text.codepoints(模式), 0)**

`if not 编译结果.ok`

`return 编译结果`

`end`
**令牌 = 编译结果.value**

**点 = text.codepoints(文本)**

**下标 = 0**

**结果 = false**

`while 下标 <= 长度(点) and not 结果`

`if 匹配序列(令牌, 0, 点, 下标, false)`

**结果 = true**

`else`
`if true`

**忽略 = 0**

`end`
`end`
**下标 = 下标 + 1**

`end`
`return 成功(结果)`

## 扩展匹配

`params(模式, 文本)`

使用宿主正则引擎（支持分组、选择、\\d 等）判断是否匹配。模式无效时
返回 regex.invalid。仅项目模式；纯 .mq.md 子集仍由 匹配/搜索 提供。

`return resource.request("regex.match", [模式, 文本])`

## 扩展搜索

`params(模式, 文本)`

`return resource.request("regex.match", [模式, 文本])`

## 捕获

`params(模式, 文本)`

返回 {ok:true, value:{matched:Bool, groups:List<Text|None>}}。下标 0 是
完整匹配；未参与匹配的分组是 None。

`return resource.request("regex.captures", [模式, 文本])`
