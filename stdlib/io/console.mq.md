---
import:
  format: "../format.mq.md"
  stream: "./stream.mq.md"
  utf8: "../text/utf8.mq.md"
---

# @exec console

## 读取行

`params()`

从已授权的 stdio/stdin 资源读取一行。按有界块读取并增量解码 UTF-8；
EOF 返回 {ok:false, code:"io.eof"}，无效 UTF-8 返回可恢复错误。

**流 = resource.open("stdio", "stdin", ["read"])**

**内存 = memory.new(1, 1)**

**字节 = []**

**结束 = false**

**错误 = ""**

`while not 结束`

**读取 = resource.read(流, 内存, 0, 1)**

`if not 读取.ok`

**结束 = true**

**错误 = 读取.code**

`else`
`if 读取.value.eof and 读取.value.count == 0`

**结束 = true**

`else`
`if 内存[0] == 10`

**结束 = true**

`else`
`if true`

**字节 = list.append(字节, 内存[0])**

`end`
`end`
`end`
`end`
`end`
**关闭 = resource.close(流)**

`if 错误 != ""`

`return {ok: false, code: 错误, message: "stdin read failed"}`

`else`
`if not 关闭.ok`

`return 关闭`

`else`
`if 长度(字节) == 0`

`return {ok: false, code: "io.eof", message: "end of input"}`

`else`
`if true`

**数据长度 = 长度(字节)**

`if 数据长度 > 0 and 字节[数据长度 - 1] == 13`

**数据长度 = 数据长度 - 1**

`else`
`if true`

**忽略 = 0**

`end`
`end`
**页 = (数据长度 + 65535) // 65536**

**数据 = memory.new(页, 页)**

**下标 = 0**

`while 下标 < 数据长度`

**`数据[下标] = 字节[下标]`**

**下标 = 下标 + 1**

`end`
**解码 = utf8.解码(数据, 0, 数据长度)**

`if not 解码.ok`

`return 解码`

`else`
`if true`

`return {ok: true, value: 解码.value}`

`end`
`end`
`end`
`end`
`end`
`end`
## 打印

`params(值)`

**结果 = stream.写文本(format.显示(值) + "\n")**

`if not 结果.ok`

`return 结果`

`else`
`if true`

`return {ok: true, value: None}`

`end`
`end`
## 写文本

`params(文本)`

`return stream.写文本(文本)`

## 写全部

`params(流, 内存, 起点, 长度)`

`return stream.写全部(流, 内存, 起点, 长度)`
