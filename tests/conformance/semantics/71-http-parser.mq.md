## @exec 切片

`params(点: List, 起: Int, 止: Int)`

`结果 = ""`

`下标 = 起`

`while 下标 < 止`

`结果 = 结果 + text.from_codepoint(点[下标])`

`下标 = 下标 + 1`

`end`

`return 结果`

## @exec 找子串

`params(点: List, 子点: List, 起点: Int)`

`结果 = 0 - 1`

`下标 = 起点`

`while 下标 + 长度(子点) <= 长度(点) and 结果 < 0`

`偏移 = 0`

`匹配 = true`

`while 偏移 < 长度(子点) and 匹配`

`if 点[下标 + 偏移] != 子点[偏移]`

`匹配 = false`

`else`

`忽略 = 0`

`end`

`偏移 = 偏移 + 1`

`end`

`if 匹配`

`结果 = 下标`

`else`

`忽略 = 0`

`end`

`下标 = 下标 + 1`

`end`

`return 结果`

## @exec 找字符

`params(点: List, 码点: Int, 起点: Int)`

`结果 = 0 - 1`

`下标 = 起点`

`while 下标 < 长度(点) and 结果 < 0`

`if 点[下标] == 码点`

`结果 = 下标`

`else`

`忽略 = 0`

`end`

`下标 = 下标 + 1`

`end`

`return 结果`

## @exec 十六进制值

`params(点: List)`

`数值 = 0`

`下标 = 0`

`while 下标 < 长度(点)`

`码点 = 点[下标]`

`if 码点 >= 48 and 码点 <= 57`

`数值 = 数值 * 16 + 码点 - 48`

`else`

`if 码点 >= 65 and 码点 <= 70`

`数值 = 数值 * 16 + 码点 - 55`

`else`

`if 码点 >= 97 and 码点 <= 102`

`数值 = 数值 * 16 + 码点 - 87`

`else`

`忽略 = 0`

`end`

`end`

`end`

`下标 = 下标 + 1`

`end`

`return 数值`

## @exec 解码分块

`params(体: Text)`

`点 = text.codepoints(体)`

`结果 = ""`

`位置 = 0`

`while 位置 < 长度(点)`

`行尾 = 找子串(点, text.codepoints("\r\n"), 位置)`

`while 行尾 < 0`

`return ""`

`end`

`大小文本 = 切片(点, 位置, 行尾)`

`大小 = 十六进制值(text.codepoints(大小文本))`

`if 大小 == 0`

`位置 = 长度(点)`

`else`

`结果 = 结果 + 切片(点, 行尾 + 2, 行尾 + 2 + 大小)`

`位置 = 行尾 + 2 + 大小 + 2`

`end`

`end`

`return 结果`

## @exec 解析响应

`params(文本: Text)`

`点 = text.codepoints(文本)`

`分隔 = text.codepoints("\r\n\r\n")`

`头部结束 = 找子串(点, 分隔, 0)`

`头部 = 切片(点, 0, 头部结束)`

`体 = 切片(点, 头部结束 + 4, 长度(点))`

`头部点 = text.codepoints(头部)`

`行列表 = []`

`行起 = 0`

`下一行 = 找子串(头部点, text.codepoints("\r\n"), 0)`

`while 下一行 >= 0`

`行列表 = list.append(行列表, 切片(头部点, 行起, 下一行))`

`行起 = 下一行 + 2`

`下一行 = 找子串(头部点, text.codepoints("\r\n"), 行起)`

`end`

`行列表 = list.append(行列表, 切片(头部点, 行起, 长度(头部点)))`

`状态行 = 行列表[0]`

`状态点 = text.codepoints(状态行)`

`第一空格 = 找字符(状态点, 32, 0)`

`第二空格 = 找字符(状态点, 32, 第一空格 + 1)`

`状态 = 整数(切片(状态点, 第一空格 + 1, 第二空格))`

`原因 = 切片(状态点, 第二空格 + 1, 长度(状态点))`

`头 = {}`

`行号 = 1`

`while 行号 < 长度(行列表)`

`行点 = text.codepoints(行列表[行号])`

`冒号 = 找字符(行点, 58, 0)`

`if 冒号 >= 0`

`名 = 切片(行点, 0, 冒号)`

`值 = 切片(行点, 冒号 + 2, 长度(行点))`

`头 = map.set(头, 名, 值)`

`else`

`忽略 = 0`

`end`

`行号 = 行号 + 1`

`end`

`传输 = map.get(头, "Transfer-Encoding")`

`解码后 = 体`

`if 传输 == "chunked"`

`解码后 = 解码分块(体)`

`else`

`解码后 = 体`

`end`

`return {状态: 状态, 原因: 原因, 头: 头, 体: 解码后}`

## @exec 构建请求

`params(方法: Text, 路径: Text, 头列表: List, 体: Text)`

`请求 = 方法 + " " + 路径 + " HTTP/1.1\r\n"`

`行号 = 0`

`while 行号 < 长度(头列表)`

`请求 = 请求 + 头列表[行号][0] + ": " + 头列表[行号][1] + "\r\n"`

`行号 = 行号 + 1`

`end`

`return 请求 + "\r\n" + 体`

## @exec main

`响应文本 = "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n"`

`响应 = 解析响应(响应文本)`

`请求 = 构建请求("GET", "/health", [["Accept", "text/plain"], ["Host", "example.test"]], "")`

`return [响应.状态, 响应.原因, 响应.头["Content-Type"], 响应.体, 请求]`
