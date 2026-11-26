# Language-only program

This fixture uses only the language itself: no `std:` import, no host
capability, no foreign kernel and no plugin. It is the program the
independent-build lane runs through a real CLI built from a copy without the
`stdlib/` tree (ADR-0070).

## @exec main

`值 = 1 + 2`

`return 值`
