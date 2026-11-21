
# @exec path
## 是否绝对

`params(路径)`

**点 = text.codepoints(路径)**

`if 长度(点) == 0`

`return false`

`else`
`if 点[0] == 47`

`return true`

`end`
`end`
`return false`

## 分割

`params(文本, 分隔码点)`

**点 = text.codepoints(文本)**

**结果 = []**

**当前 = ""**

**下标 = 0**

`while 下标 < 长度(点)`

`if 点[下标] == 分隔码点`

**结果 = list.append(结果, 当前)**

**当前 = ""**

`else`
`if true`

**当前 = 当前 + text.from_codepoint(点[下标])**

`end`
`end`
**下标 = 下标 + 1**

`end`
`return list.append(结果, 当前)`

## 连接

`params(片段列表)`

空片段被跳过，片段之间只插入一个 /。

**结果 = ""**

**下标 = 0**

`while 下标 < 长度(片段列表)`

**片段 = 文本(片段列表[下标])**

`if 片段 != ""`

`if 结果 == ""`

**结果 = 片段**

`else`
`if true`

**结果 = 结果 + "/" + 片段**

`end`
`end`
`else`
`if true`

**忽略 = 0**

`end`
`end`
**下标 = 下标 + 1**

`end`
`return 结果`

## 文件名

`params(路径)`

**片段 = 分割(路径, 47)**

`return 片段[长度(片段) - 1]`

## 目录名

`params(路径)`

**片段 = 分割(路径, 47)**

**结果 = []**

**下标 = 0**

`while 下标 < 长度(片段) - 1`

**结果 = list.append(结果, 片段[下标])**

**下标 = 下标 + 1**

`end`
`return 连接(结果)`

## 扩展名

`params(路径)`

**名字 = 文件名(路径)**

**点列表 = 分割(名字, 46)**

`if 长度(点列表) <= 1`

`return ""`

`end`
`return 点列表[长度(点列表) - 1]`

## 去尾

`params(列表)`

**结果 = []**

**下标 = 0**

`while 下标 < 长度(列表) - 1`

**结果 = list.append(结果, 列表[下标])**

**下标 = 下标 + 1**

`end`
`return 结果`

## 规范

`params(路径)`

词法规范化：折叠 .、空片段和重复 /，用 .. 消去前一片段；
绝对路径保留前导 /。不访问文件系统。

**片段 = 分割(路径, 47)**

**绝对 = 是否绝对(路径)**

**栈 = []**

**下标 = 0**

`while 下标 < 长度(片段)`

**段 = 片段[下标]**

`if 段 == "" or 段 == "."`

**忽略 = 0**

`else`
`if 段 == ".."`

`if 长度(栈) > 0 and 栈[长度(栈) - 1] != ".."`

**栈 = 去尾(栈)**

`else`
`if not 绝对`

**栈 = list.append(栈, 段)**

`else`
`if true`

**忽略绝对根 = 0**

`end`
`end`
`end`
`else`
`if true`

**栈 = list.append(栈, 段)**

`end`
`end`
`end`
**下标 = 下标 + 1**

`end`
`if 绝对`

`return "/" + 连接(栈)`

`end`
`return 连接(栈)`
