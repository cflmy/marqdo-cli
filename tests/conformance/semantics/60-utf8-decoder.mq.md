## @exec 成功

`params(值)`

`return {ok: true, value: 值}`

## @exec 失败

`params(代码, 消息)`

`return {ok: false, code: 代码, message: 消息}`

## @exec 续字节

`params(字节: Int)`

`return 字节 >= 128 and 字节 < 192`

## @exec 解码2

`params(内存: Memory, 基址: Int, 长度: Int, 偏移: Int, 文本: Text)`

`if 偏移 + 1 >= 长度`

`return [文本, 偏移, "utf8.truncated"]`

`end`

`b1 = 内存[基址 + 偏移 + 1]`

`if not 续字节(b1)`

`return [文本, 偏移, "utf8.invalid_continuation"]`

`end`

`码点 = (内存[基址 + 偏移] - 192) * 64 + (b1 - 128)`

`if 码点 < 128`

`return [文本, 偏移, "utf8.invalid_scalar"]`

`end`

`return [文本 + text.from_codepoint(码点), 偏移 + 2, ""]`

## @exec 解码3

`params(内存: Memory, 基址: Int, 长度: Int, 偏移: Int, 文本: Text)`

`if 偏移 + 2 >= 长度`

`return [文本, 偏移, "utf8.truncated"]`

`end`

`b1 = 内存[基址 + 偏移 + 1]`

`b2 = 内存[基址 + 偏移 + 2]`

`if not (续字节(b1) and 续字节(b2))`

`return [文本, 偏移, "utf8.invalid_continuation"]`

`end`

`码点 = (内存[基址 + 偏移] - 224) * 4096 + (b1 - 128) * 64 + (b2 - 128)`

`if 码点 < 2048 or (码点 >= 55296 and 码点 <= 57343)`

`return [文本, 偏移, "utf8.invalid_scalar"]`

`end`

`return [文本 + text.from_codepoint(码点), 偏移 + 3, ""]`

## @exec 解码4

`params(内存: Memory, 基址: Int, 长度: Int, 偏移: Int, 文本: Text)`

`if 偏移 + 3 >= 长度`

`return [文本, 偏移, "utf8.truncated"]`

`end`

`b1 = 内存[基址 + 偏移 + 1]`

`b2 = 内存[基址 + 偏移 + 2]`

`b3 = 内存[基址 + 偏移 + 3]`

`if not (续字节(b1) and 续字节(b2) and 续字节(b3))`

`return [文本, 偏移, "utf8.invalid_continuation"]`

`end`

`码点 = (内存[基址 + 偏移] - 240) * 262144 + (b1 - 128) * 4096 + (b2 - 128) * 64 + (b3 - 128)`

`if 码点 < 65536 or 码点 > 1114111`

`return [文本, 偏移, "utf8.invalid_scalar"]`

`end`

`return [文本 + text.from_codepoint(码点), 偏移 + 4, ""]`

## @exec 解码

`params(内存: Memory, 基址: Int, 长度: Int)`

`偏移 = 0`

`文本 = ""`

`错误 = ""`

`while 偏移 < 长度 and 错误 == ""`

`首 = 内存[基址 + 偏移]`

`if 首 < 128`

`文本 = 文本 + text.from_codepoint(首)`

`偏移 = 偏移 + 1`

`else`

`if 首 < 194`

`错误 = "utf8.invalid_lead"`

`else`

`if 首 < 224`

`结果 = 解码2(内存, 基址, 长度, 偏移, 文本)`

`文本 = 结果[0]`

`偏移 = 结果[1]`

`错误 = 结果[2]`

`else`

`if 首 < 240`

`结果 = 解码3(内存, 基址, 长度, 偏移, 文本)`

`文本 = 结果[0]`

`偏移 = 结果[1]`

`错误 = 结果[2]`

`else`

`if 首 < 245`

`结果 = 解码4(内存, 基址, 长度, 偏移, 文本)`

`文本 = 结果[0]`

`偏移 = 结果[1]`

`错误 = 结果[2]`

`else`

`错误 = "utf8.invalid_lead"`

`end`

`end`

`end`

`end`

`end`

`end`

`if 错误 == ""`

`return 成功(文本)`

`else`

`return 失败(错误, "UTF-8 解码失败")`

`end`

## @exec 写字节

`params(内存: Memory, 基址: Int, 字节: List)`

`下标 = 0`

`while 下标 < 长度(字节)`

`内存[基址 + 下标] = 字节[下标]`

`下标 = 下标 + 1`

`end`

`return None`

## @exec main

`内存 = memory.new(1, 1)`

`写字节(内存, 0, [65, 195, 169, 228, 184, 173])`

`写字节(内存, 16, [228, 184])`

`写字节(内存, 32, [228, 65, 173])`

`写字节(内存, 48, [237, 160, 128])`

`写字节(内存, 64, [240, 157, 132, 158])`

`return [解码(内存, 0, 6), 解码(内存, 16, 2), 解码(内存, 32, 3), 解码(内存, 48, 3), 解码(内存, 64, 4)]`
