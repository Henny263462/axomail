# Performance

Development targets (from the project brief), treated as measurable goals —
**none of these are claimed as achieved**:

| Goal | Target | Status |
| ---- | ------ | ------ |
| Warm start to visible main window | < 1 s | not yet measured |
| Local inbox usable without network | immediate | not implemented |
| Smooth scrolling | ≥ 50,000 indexed messages | not implemented |
| Idle memory | constant, documented | not yet measured |
| Sync memory | bounded (pagination/streaming) | not implemented |

## Method

Benchmarks and measurement scripts (cold/warm start, inbox open, message
load, list scrolling, local search, idle/sync memory, 1k/10k message sync,
large MIME parsing, large attachments) are added alongside the features
they measure, starting in phase 2. Results land in this directory as dated
markdown files with hardware and OS noted.

Rule: no performance number appears in README or marketing text before a
reproducible measurement exists here.

## Measurements

None yet.
