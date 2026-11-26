#!/usr/bin/env bash
# Fast public-contract lane: the real CLI against the frozen Analysis /
# Observation / Tool fixtures, with one explicit verified stdlib Bundle.
# Kept as a script so the CI command lines stay short and reviewable.
set -eux
bundle="${MARQDO_TEST_STDLIB_BUNDLE:?set MARQDO_TEST_STDLIB_BUNDLE}"
export MARQDO_STDLIB_DIR="$bundle"
bin="./target/debug/marqdo"
"$bin" check tests/consumers/fixtures/observation/basic.mq.md --emit /tmp/analysis.json
"$bin" run tests/consumers/fixtures/observation/basic.mq.md --output jsonl --observation-file /tmp/observation.json > /tmp/observation.jsonl
python3 tests/consumers/analysis_consumer.py /tmp/analysis.json
python3 tests/consumers/observation_consumer.py --validate-schema /tmp/observation.jsonl
python3 tests/consumers/observation_consumer.py --snapshot --analysis /tmp/analysis.json /tmp/observation.json
"$bin" check tests/conformance/semantics/85-nested-loop-control.mq.md --emit /tmp/control-analysis.json
"$bin" run tests/conformance/semantics/85-nested-loop-control.mq.md --output jsonl --observation-file /tmp/control-observation.json > /tmp/control-events.jsonl
python3 tests/consumers/validate_protocols.py /tmp/control-analysis.json /tmp/control-observation.json examples/tool-unknown-name/tool.json
python3 tests/consumers/analysis_consumer.py /tmp/control-analysis.json --require-transfers
python3 tests/consumers/observation_consumer.py --snapshot --analysis /tmp/control-analysis.json /tmp/control-observation.json
python3 tests/consumers/observation_consumer.py --validate-schema --analysis /tmp/control-analysis.json /tmp/control-events.jsonl
python3 tools/ci/verify_observation_stream.py /tmp/control-events.jsonl /tmp/control-observation.json
python3 tools/ci/assert_stale_analysis.py /tmp/control-analysis.json /tmp/control-events.jsonl
