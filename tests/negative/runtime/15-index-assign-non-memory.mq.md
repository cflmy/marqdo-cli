## main

List values keep their v1 value semantics: index assignment is not defined
for them and must not silently turn a list into shared mutable memory.

**列表 = [1, 2]**

**`列表[0] = 9`**
