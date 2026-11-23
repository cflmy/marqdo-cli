## @exec main

`轮次 = 0`

`合计 = 0`

`while 轮次 < 4`

`轮次 = 轮次 + 1`

`if 轮次 == 1`

`合计 = 合计 + 1`

`else if 1 / (轮次 - 1) > 0`

`if 轮次 == 2`

`continue`

`else`

`合计 = 合计 + 3`

`break`

`end`

`else if 1 / 0 > 0`

`return -1`

`else`

`return -2`

`end`

`end`

`return 合计`
