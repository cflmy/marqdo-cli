# @knowledge 数学

## @exec 阶乘

定义：

$$
n! = \begin{cases}1,& n\le1\\n(n-1)!,& n>1\end{cases}
$$

`if n <= 1`

递归终点，返回 1。

`return 1`

`else`

计算更小的问题。

`结果 = n * 阶乘(n=n-1)`

`return 结果`

`end`
