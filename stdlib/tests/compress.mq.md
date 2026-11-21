---
import:
  compress: "std:compress"
---

## main

**压缩结果 = compress.压缩("hello 世界")**

**解压结果 = compress.解压(压缩结果.value)**

*[压缩结果.ok, 长度(压缩结果.value) > 0, 解压结果.ok, 解压结果.value]*
