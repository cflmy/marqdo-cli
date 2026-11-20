---
import:
  format: "../format.mq.md"
  stream: "./stream.mq.md"
  utf8: "../text/utf8.mq.md"
---

## 读取行

`()`

从已授权的 `stdio/stdin` 资源读取一行。按有界块读取并增量解码 UTF-8；
EOF 返回 `{ok:false, code:"io.eof"}`，无效 UTF-8 返回可恢复错误。

**流 = resource.open("stdio", "stdin", ["read"])**

**内存 = memory.new(1, 1)**

**字节 = []**

**结束 = false**

**错误 = ""**

- `not 结束`

  **读取 = resource.read(流, 内存, 0, 1)**

  1. `not 读取.ok`

     **结束 = true**

     **错误 = 读取.code**

  2. `读取.value.eof and 读取.value.count == 0`

     **结束 = true**

  3. `内存[0] == 10`

     **结束 = true**

  4. `true`

     **字节 = list.append(字节, 内存[0])**

**关闭 = resource.close(流)**

1. `错误 != ""`

   *{ok: false, code: 错误, message: "stdin read failed"}*

2. `not 关闭.ok`

   *关闭*

3. `长度(字节) == 0`

   *{ok: false, code: "io.eof", message: "end of input"}*

4. `true`

  **页 = (长度(字节) + 65535) // 65536**

  **数据 = memory.new(页, 页)**

  **下标 = 0**

  - `下标 < 长度(字节)`

    **`数据[下标] = 字节[下标]`**

    **下标 = 下标 + 1**

  **解码 = utf8.解码(数据, 0, 长度(字节))**

  1. `not 解码.ok`

     *解码*

  2. `true`

     *{ok: true, value: 解码.value}*

## 打印

`(值)`

**结果 = stream.写文本(format.显示(值) + "\n")**

1. `not 结果.ok`

   *结果*

2. `true`

   *{ok: true, value: None}*

## 写文本

`(文本)`

*stream.写文本(文本)*

## 写全部

`(流, 内存, 起点, 长度)`

*stream.写全部(流, 内存, 起点, 长度)*
