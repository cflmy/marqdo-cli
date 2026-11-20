---
import:
  format: "./format.mq.md"
  text: "./text.mq.md"
---

## 写文本

`(文本)`

把文本编码为 UTF-8 并写入已授权的 stdout 资源流。

**内存 = memory.new(1, 1)**

**长度 = text.utf8编码(文本, 内存, 0)**

**流 = resource.open("stdio", "stdout", ["write"])**

> resource.write(流, 内存, 0, 长度)

> resource.close(流)

*None*

## 写值

`(值)`

按语言值语义格式化后输出，并追加换行。

**文字 = format.显示(值) + "\n"**

> 写文本(文字)

*None*
