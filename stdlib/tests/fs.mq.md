---
import:
  fs: "std:fs/file"
---

## main

**创建 = fs.创建目录("out")**

**写入 = fs.写文本("out/fs-a.txt", "你好\n")**

**存在 = fs.存在("out/fs-a.txt")**

**元数据 = fs.元数据("out/fs-a.txt")**

**列表 = fs.列出目录("out")**

**是文件 = fs.是文件("out/fs-a.txt")**

**是目录 = fs.是目录("out")**

**重命名 = fs.重命名("out/fs-a.txt", "out/fs-b.txt")**

**读取 = fs.读文本("out/fs-b.txt")**

**删除 = fs.删除("out/fs-b.txt")**

**删除目录 = fs.删除("out")**

*[创建.ok, 写入.value.count, 存在.value, 元数据.value.kind, 元数据.value.size, 列表.value, 是文件.value, 是目录.value, 重命名.ok, 读取.value, 删除.ok, 删除目录.ok]*
