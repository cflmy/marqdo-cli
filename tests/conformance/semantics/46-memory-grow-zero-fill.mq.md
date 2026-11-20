## main

A zero-page region is legal. Growing it returns the previous page count,
preserves old bytes and zero-fills every new byte.

**内存 = memory.new(0, 2)**

**旧页数 = memory.grow(内存, 1)**

**内存[0] = 9**

**页数 = memory.size(内存)**

*[旧页数, 页数, 内存[0], 内存[65535]]*
