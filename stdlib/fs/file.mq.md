---
import:
  utf8: "../text/utf8.mq.md"
---

## 读文本

`(路径)`

以 UTF-8 读取一个有界的项目文件。文件在打开时整文件载入，上限 8 MiB；
结果使用可恢复 `Result` 形状。

**流 = resource.open("file", 路径, ["read"])**

**内存 = memory.new(128, 128)**

**读取 = resource.read(流, 内存, 0, 8388608)**

**关闭 = resource.close(流)**

1. `not 读取.ok`

   *读取*

2. `not 关闭.ok`

   *关闭*

3. `true`

   **文本 = utf8.解码(内存, 0, 读取.value.count)**

   1. `not 文本.ok`

      *文本*

   2. `true`

      *{ok: true, value: 文本.value}*

## 写文本

`(路径, 文本)`

把文本按 UTF-8 写入项目文件。路径必须已经通过 `fs_write` 授权；
文件只在 `resource.close` 成功时创建或覆盖，写入缓冲上限 8 MiB。

**内存 = memory.new(128, 128)**

**长度 = utf8.编码(文本, 内存, 0)**

**流 = resource.open("file", 路径, ["write"])**

**写入 = resource.write(流, 内存, 0, 长度)**

**关闭 = resource.close(流)**

1. `not 写入.ok`

   *写入*

2. `not 关闭.ok`

   *关闭*

3. `true`

   *{ok: true, value: {count: 写入.value.count}}*

## 复制文件

`(源, 目标)`

按字节复制一个有界的项目文件。源读取和目标写入分别需要
`fs_read`/`fs_write` 授权，结果值与 [`写文本`](#写文本) 相同。

**输入 = resource.open("file", 源, ["read"])**

**内存 = memory.new(128, 128)**

**读取 = resource.read(输入, 内存, 0, 8388608)**

**关闭输入 = resource.close(输入)**

1. `not 读取.ok`

   *读取*

2. `not 关闭输入.ok`

   *关闭输入*

3. `true`

   **输出 = resource.open("file", 目标, ["write"])**

   **写入 = resource.write(输出, 内存, 0, 读取.value.count)**

   **关闭输出 = resource.close(输出)**

   1. `not 写入.ok`

      *写入*

   2. `not 关闭输出.ok`

      *关闭输出*

   3. `true`

      *{ok: true, value: {count: 写入.value.count}}*

## 存在

`(路径)`

*resource.request("fs.exists", [路径])*

## 元数据

`(路径)`

*resource.request("fs.metadata", [路径])*

## 是文件

`(路径)`

**元数据 = resource.request("fs.metadata", [路径])**

1. `not 元数据.ok`

   *元数据*

2. `true`

   *{ok: true, value: 元数据.value.kind == "file"}*

## 是目录

`(路径)`

**元数据 = resource.request("fs.metadata", [路径])**

1. `not 元数据.ok`

   *元数据*

2. `true`

   *{ok: true, value: 元数据.value.kind == "directory"}*

## 列出目录

`(路径)`

*resource.request("fs.list", [路径])*

## 创建目录

`(路径)`

*resource.request("fs.mkdir", [路径])*

## 删除

`(路径)`

删除一个文件或空目录。非空目录返回 `fs.not_empty`，不递归删除。

*resource.request("fs.remove", [路径])*

## 重命名

`(源, 目标)`

目标已存在时返回 `fs.exists`，不覆盖。

*resource.request("fs.rename", [源, 目标])*

## 追加文本

`(路径, 文本)`

把文本追加到项目文件；文件不存在时创建。仍受 8 MiB 单次读取上限和
`fs_write` 授权约束。

*resource.request("fs.append", [路径, 文本])*

## 读取片段

`(路径, 偏移, 长度)`

从偏移处读取最多 `长度` 字节，按 UTF-8 解码后返回文本。偏移/长度必须
是非负整数，长度上限 8 MiB。

*resource.request("fs.read_at", [路径, 偏移, 长度])*

## 遍历

`(路径)`

递归列出目录下的普通文件，返回相对项目根的路径列表。跳过符号链接，
深度上限 64，条目上限 4096。

*resource.request("fs.walk", [路径])*
