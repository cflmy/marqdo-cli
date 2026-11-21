---
import:
  format: "../format.mq.md"
  utf8: "../text/utf8.mq.md"
---

# @exec stream

## 页数

`params(字节)`

**页 = (字节 + 65535) // 65536**

`if 页 == 0`

`return 1`

`else`
`if true`

`return 页`

`end`
`end`
## 写全部

`params(流, 内存, 起点, 长度)`

把 长度 字节从 内存 写到资源流。短写只返回实际数量，调用方继续；
零进度返回 io.write_zero，不静默吞掉失败。

**已写 = 0**

**失败 = false**

**本次 = {ok: true, value: {count: 0}}**

**错误结果 = {ok: false, code: "io.write_zero", message: "resource.write made no progress"}**

`while 已写 < 长度`

**剩余 = 长度 - 已写**

**本次 = resource.write(流, 内存, 起点 + 已写, 剩余)**

`if not 本次.ok`

**失败 = true**

**错误结果 = 本次**

**已写 = 长度**

`else`
`if 本次.value.count == 0`

**失败 = true**

**已写 = 长度**

`else`
`if true`

**已写 = 已写 + 本次.value.count**

`end`
`end`
`end`
`end`
`if 失败`

`return 错误结果`

`else`
`if 已写 < 长度`

`return 错误结果`

`else`
`if true`

`return {ok: true, value: {count: 长度}}`

`end`
`end`
`end`
## 写文本

`params(文本)`

把文本编码为 UTF-8 并写入已授权的 stdout 资源流。内存按实际字节长度增长，
写入循环检查 {ok,count}；只有全部字节写完并关闭成功才返回成功。

**字节数 = format.字节长度(文本)**

**内存 = memory.new(页数(字节数), 页数(字节数))**

**长度 = utf8.编码(文本, 内存, 0)**

**流 = resource.open("stdio", "stdout", ["write"])**

**写结果 = 写全部(流, 内存, 0, 长度)**

**关闭 = resource.close(流)**

`if not 写结果.ok`

`return 写结果`

`else`
`if not 关闭.ok`

`return 关闭`

`else`
`if true`

`return {ok: true, value: {count: 长度}}`
`end`
`end`
`end`
