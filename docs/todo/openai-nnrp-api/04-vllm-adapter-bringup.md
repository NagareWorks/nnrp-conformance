# vLLM Adapter Bring-Up

## Adapter Contract

- [ ] Add `vllm-nnrp-adapter` as the first expected API profile implementation.
- [ ] Define the adapter command shape that consumes API profile execution plans and emits API profile case results.
- [ ] Keep the command shape owned by the adapter repository while keeping plan/result JSON owned by the suite.
- [ ] Add local smoke fixtures that can run without a GPU-backed vLLM process.

## Level 1 Cases

- [ ] Validate `chat.completions.create` streaming text delta mapping.
- [ ] Validate `chat.completions.create` non-streaming completion body mapping.
- [ ] Validate invalid body rejection.
- [ ] Validate unsupported operation rejection.
- [ ] Validate usage summary shape.
- [ ] Validate tool-call delta pass-through when advertised.
- [ ] Validate cancellation behavior.
- [ ] Validate backend error mapping.
- [ ] Validate capability document shape.

## Benchmark Surface

- [ ] Add informational API profile benchmark scenarios after correctness recipes land.
- [ ] Separate adapter mapper overhead from real model generation time.
- [ ] Track streaming event throughput, p50/p95 event latency, cancellation latency, and end-to-end smoke latency.
- [ ] Keep API profile benchmark failures out of the protocol correctness gate unless a release explicitly opts into performance thresholds.

