# Observation fixtures

These are the minimal executable fixtures from RFC-OBS-0001 §13 plus the
schema-validation fixtures used by
[`observation_consumer.py`](../../observation_consumer.py).

| File | Purpose |
|---|---|
| `basic.mq.md` | Fixture A: live stdout ordering and return value. |
| `realtime.mq.md` | Fixture B: `READY` must be observable before the run exits. |
| `structure.mq.md` | Fixture C: `main -> f -> g`, repeated calls, branch, loop. |
| `values.mq.md` | Fixture D: default privacy and exact value selection. |
| `valid/*.jsonl` | Schema-valid streams (minimal and full event table). |
| `invalid/*.json` | One-object negative fixtures: unknown major, missing/`negative` seq, bad chunk/span/terminal. |

The `.mq.md` files are inputs to the released CLI:

```bash
marqdo run tests/consumers/fixtures/observation/basic.mq.md --output jsonl
python3 tests/consumers/observation_consumer.py /tmp/events.jsonl
```
