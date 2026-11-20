## main

Two regions never alias even at the same offset. A copied name does share
the region identity, and equality compares that identity, not the bytes.

**甲 = memory.new(1, 1)**

**乙 = memory.new(1, 1)**

**`甲[0] = 5`**

**`乙[0] = 6`**

**别名 = 甲**

**`别名[1] = 7`**

*[甲[0], 乙[0], 甲[1], 甲 == 别名, 甲 == 乙]*
