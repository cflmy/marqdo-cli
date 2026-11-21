# @knowledge 设备测试

## @exec main

打开独立仓库提供的资源；此标题仅决定文档语义，不涉及 Provider 注册。

`流 = resource.open("demo.sensor", "unit-1", ["read"])`

`内存 = memory.new(1, 1)`

`读取 = resource.read(流, 内存, 0, 5)`

`关闭 = resource.close(流)`

`if not 读取.ok`

`return 读取`

`else`

`if not 关闭.ok`

`return 关闭`

`else`

`if true`

`return 读取.value.count`

`end`

`end`

`end`
