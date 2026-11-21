---
import:
  fs: "std:fs/file"
---

## main

**写入 = fs.写文本("out/fs-a.txt", "你好\n")**

**读取 = fs.读文本("out/fs-a.txt")**

**复制 = fs.复制文件("out/fs-a.txt", "out/fs-b.txt")**

**复制读取 = fs.读文本("out/fs-b.txt")**

*[写入.value.count, 读取.value, 复制.value.count, 复制读取.value]*
