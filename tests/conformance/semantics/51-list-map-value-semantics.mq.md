## @exec main

Lists and maps keep value semantics: constructors return new values and the
inputs are unchanged.

`原列表 = [1, 2]`

`新列表 = list.append(原列表, 3)`

`原映射 = {a: 1, b: 2}`

`新映射 = map.set(原映射, "c", 3)`

`删除后 = map.remove(新映射, "a")`

`键 = map.keys(删除后)`

`return [原列表, 新列表, 原映射.b, 新映射.b, 删除后.b, 键, map.has(原映射, "c"), map.get(原映射, "z"), list.concat([1], [2, 3])]`
