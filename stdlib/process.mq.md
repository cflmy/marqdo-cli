
# @exec process
## 运行

`params(程序, 参数列表)`

在项目模式下运行一个显式授权的可执行文件。程序路径必须与
--allow-process PROGRAM 精确匹配；不经过 shell，清空环境变量，
工作目录是项目根，stdin 为空。5 秒超时、1 MiB stdout/stderr 上限，
结果形状为 {status, stdout, stderr}。

`return resource.request("process.run", [程序, 参数列表])`
