# vLLM Adapter Bring-Up

## Adapter Contract

- [x] Add `vllm-nnrp-adapter` as the first expected API profile implementation.
- [x] Define the adapter command shape that consumes API profile execution plans and emits API profile case results.
- [x] Keep the command shape owned by the adapter repository while keeping plan/result JSON owned by the suite.
- [x] Add local smoke fixtures that can run without a GPU-backed vLLM process.

## Level 1 Cases

- [x] Validate `chat.completions.create` streaming text delta mapping.
- [x] Validate `chat.completions.create` non-streaming completion body mapping.
- [x] Validate invalid body rejection.
- [x] Validate unsupported operation rejection.
- [x] Validate usage summary shape.
- [x] Validate tool-call delta pass-through when advertised.
- [x] Validate cancellation behavior.
- [x] Validate backend error mapping.
- [x] Validate capability document shape.

## Benchmark Surface

- [ ] Add informational API profile benchmark scenarios after correctness recipes land.
- [ ] Separate adapter mapper overhead from real model generation time.
- [ ] Track streaming event throughput, p50/p95 event latency, cancellation latency, and end-to-end smoke latency.
- [ ] Keep API profile benchmark failures out of the protocol correctness gate unless a release explicitly opts into performance thresholds.
