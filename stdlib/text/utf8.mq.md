
# @exec utf8
## 成功

`params(值)`

`return {ok: true, value: 值}`

## 失败

`params(代码, 消息)`

`return {ok: false, code: 代码, message: 消息}`

## 续字节

`params(字节)`

`return 字节 >= 128 and 字节 < 192`

## 编码

`params(文本, 内存, 基址)`

把 Unicode 标量序列按 UTF-8 写入 Memory，返回字节长度。
这是 .mq.md 自己实现的编码逻辑，不是 Rust 内建 UTF-8 转换。

**点 = text.codepoints(文本)**

**下标 = 0**

**偏移 = 0**

`while 下标 < 长度(点)`

**码点 = 点[下标]**

`if 码点 < 128`

**`内存[基址 + 偏移] = 码点`**

**偏移 = 偏移 + 1**

`else`
`if 码点 < 2048`

**`内存[基址 + 偏移] = 192 + 码点 // 64`**

**`内存[基址 + 偏移 + 1] = 128 + 码点 % 64`**

**偏移 = 偏移 + 2**

`else`
`if 码点 < 65536`

**`内存[基址 + 偏移] = 224 + 码点 // 4096`**

**`内存[基址 + 偏移 + 1] = 128 + 码点 // 64 % 64`**

**`内存[基址 + 偏移 + 2] = 128 + 码点 % 64`**

**偏移 = 偏移 + 3**

`else`
`if true`

**`内存[基址 + 偏移] = 240 + 码点 // 262144`**

**`内存[基址 + 偏移 + 1] = 128 + 码点 // 4096 % 64`**

**`内存[基址 + 偏移 + 2] = 128 + 码点 // 64 % 64`**

**`内存[基址 + 偏移 + 3] = 128 + 码点 % 64`**

**偏移 = 偏移 + 4**

`end`
`end`
`end`
`end`
**下标 = 下标 + 1**

`end`
`return 偏移`

## 解码2

`params(内存, 基址, 长度, 偏移, 文本)`

`if 偏移 + 1 >= 长度`

`return [文本, 偏移, "utf8.truncated"]`

`end`
**b1 = 内存[基址 + 偏移 + 1]**

`if not 续字节(b1)`

`return [文本, 偏移, "utf8.invalid_continuation"]`

`end`
**码点 = (内存[基址 + 偏移] - 192) * 64 + (b1 - 128)**

`if 码点 < 128`

`return [文本, 偏移, "utf8.invalid_scalar"]`

`end`
`return [文本 + text.from_codepoint(码点), 偏移 + 2, ""]`

## 解码3

`params(内存, 基址, 长度, 偏移, 文本)`

`if 偏移 + 2 >= 长度`

`return [文本, 偏移, "utf8.truncated"]`

`end`
**b1 = 内存[基址 + 偏移 + 1]**

**b2 = 内存[基址 + 偏移 + 2]**

`if not (续字节(b1) and 续字节(b2))`

`return [文本, 偏移, "utf8.invalid_continuation"]`

`end`
**码点 = (内存[基址 + 偏移] - 224) * 4096 + (b1 - 128) * 64 + (b2 - 128)**

`if 码点 < 2048 or (码点 >= 55296 and 码点 <= 57343)`

`return [文本, 偏移, "utf8.invalid_scalar"]`

`end`
`return [文本 + text.from_codepoint(码点), 偏移 + 3, ""]`

## 解码4

`params(内存, 基址, 长度, 偏移, 文本)`

`if 偏移 + 3 >= 长度`

`return [文本, 偏移, "utf8.truncated"]`

`end`
**b1 = 内存[基址 + 偏移 + 1]**

**b2 = 内存[基址 + 偏移 + 2]**

**b3 = 内存[基址 + 偏移 + 3]**

`if not (续字节(b1) and 续字节(b2) and 续字节(b3))`

`return [文本, 偏移, "utf8.invalid_continuation"]`

`end`
**码点 = (内存[基址 + 偏移] - 240) * 262144 + (b1 - 128) * 4096 + (b2 - 128) * 64 + (b3 - 128)**

`if 码点 < 65536 or 码点 > 1114111`

`return [文本, 偏移, "utf8.invalid_scalar"]`

`end`
`return [文本 + text.from_codepoint(码点), 偏移 + 4, ""]`

## 解码

`params(内存, 基址, 长度)`

**偏移 = 0**

**文本 = ""**

**错误 = ""**

`while 偏移 < 长度 and 错误 == ""`

**首 = 内存[基址 + 偏移]**

`if 首 < 128`

**文本 = 文本 + text.from_codepoint(首)**

**偏移 = 偏移 + 1**

`else`
`if 首 < 194`

**错误 = "utf8.invalid_lead"**

`else`
`if 首 < 224`

**结果 = 解码2(内存, 基址, 长度, 偏移, 文本)**

**文本 = 结果[0]**

**偏移 = 结果[1]**

**错误 = 结果[2]**

`else`
`if 首 < 240`

**结果 = 解码3(内存, 基址, 长度, 偏移, 文本)**

**文本 = 结果[0]**

**偏移 = 结果[1]**

**错误 = 结果[2]**

`else`
`if 首 < 245`

**结果 = 解码4(内存, 基址, 长度, 偏移, 文本)**

**文本 = 结果[0]**

**偏移 = 结果[1]**

**错误 = 结果[2]**

`else`
`if true`

**错误 = "utf8.invalid_lead"**

`end`
`end`
`end`
`end`
`end`
`end`
`end`
`if 错误 == ""`

`return 成功(文本)`

`else`
`if true`

`return 失败(错误, "UTF-8 解码失败")`

`end`
`end`
## utf8编码

`params(文本, 内存, 基址)`

`return 编码(文本, 内存, 基址)`

## utf8解码

`params(内存, 基址, 长度)`

`return 解码(内存, 基址, 长度)`
