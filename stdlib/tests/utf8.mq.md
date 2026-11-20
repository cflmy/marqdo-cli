---
import:
  utf8: "std:text/utf8"
---

## main

**内存 = memory.new(1, 1)**

**长度 = utf8.编码("é", 内存, 0)**

*[长度, utf8.解码(内存, 0, 长度).value, utf8.解码(内存, 0, 1).ok]*
