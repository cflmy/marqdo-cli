## main

This function demonstrates an addressable byte region next to ordinary prose.
A copied name shares the same region identity.

**区域 = memory.new(1, 4)**

**`区域[8] = 42`**

**另一个名字 = 区域**

**`另一个名字[9] = 99`**

*区域[8] + 区域[9]*
