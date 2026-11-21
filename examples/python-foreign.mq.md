## @knowledge Python 校验

```python
import json, math, sys

envelope = json.load(sys.stdin)
r = int(envelope["input"]["entries"]["r"]["decimal"])
area = math.pi * r ** 2
print(json.dumps({"type": "map", "entries": {"area": {"type": "float", "value": area}}}))
```

## @exec main

`结果 = foreign.call(ref("#python-校验"), {"r": 3})`

> **打印(结果.area)**
