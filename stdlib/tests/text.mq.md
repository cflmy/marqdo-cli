---
import:
  text: "std:text"
---

# @exec text

## main

**内存 = memory.new(1, 1)**

**长度 = text.utf8编码("é", 内存, 0)**

`return [长度, 内存[0], 内存[1]]`
