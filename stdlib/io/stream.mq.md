---
import:
  format: "../format.mq.md"
  utf8: "../text/utf8.mq.md"
---

## 页数

`(字节)`

**页 = (字节 + 65535) // 65536**

1. `页 == 0`

   *1*

2. `true`

   *页*

## 写全部

`(流, 内存, 起点, 长度)`

把 `长度` 字节从 `内存` 写到资源流。短写只返回实际数量，调用方继续；
零进度返回 `io.write_zero`，不静默吞掉失败。

**已写 = 0**

**失败 = false**

**本次 = {ok: true, value: {count: 0}}**

**错误结果 = {ok: false, code: "io.write_zero", message: "resource.write made no progress"}**

- `已写 < 长度`

  **剩余 = 长度 - 已写**

  **本次 = resource.write(流, 内存, 起点 + 已写, 剩余)**

  1. `not 本次.ok`

     **失败 = true**

     **错误结果 = 本次**

     **已写 = 长度**

  2. `本次.value.count == 0`

     **失败 = true**

     **已写 = 长度**

  3. `true`

     **已写 = 已写 + 本次.value.count**

1. `失败`

   *错误结果*

2. `已写 < 长度`

   *错误结果*

3. `true`

   *{ok: true, value: {count: 已写}}*

## 写文本

`(文本)`

把文本编码为 UTF-8 并写入已授权的 stdout 资源流。内存按实际字节长度增长，
写入循环检查 `{ok,count}`；只有全部字节写完并关闭成功才返回成功。

**字节数 = format.字节长度(文本)**

**内存 = memory.new(页数(字节数), 页数(字节数))**

**长度 = utf8.编码(文本, 内存, 0)**

**流 = resource.open("stdio", "stdout", ["write"])**

**写结果 = 写全部(流, 内存, 0, 长度)**

**关闭 = resource.close(流)**

1. `not 写结果.ok`

   *写结果*

2. `not 关闭.ok`

   *关闭*

3. `true`

   *{ok: true, value: None}*
