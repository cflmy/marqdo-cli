---
import:
  text_lib: "std:text"
---

# @exec text

## main

**内存 = memory.new(1, 1)**

**长度 = text_lib.utf8编码("é", 内存, 0)**

`return [长度, 内存[0], 内存[1]]`
