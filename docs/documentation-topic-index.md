# Documentation Topic Index

## Extracted Docs

| Topic | File | Description |
|-------|------|-------------|
| Project Structure | [project-structure.md](project-structure.md) | Full directory tree with module descriptions |
| QuestDB Persistence | [questdb-persistence.md](questdb-persistence.md) | Configuration, data retention, schema reference |
| Exchange Comparison | [exchange-comparison.md](exchange-comparison.md) | LOB/trade strategies, delivery models, pros/cons |
| Grafana LOB Visualization | [grafana-lob-visualization.md](grafana-lob-visualization.md) | Metrics endpoint, dashboard layout, setup |
| LOB Data Processing | [lob-data-processing.md](lob-data-processing.md) | Parquet stream reader, key rules, CLI usage |

## Architecture Decision Records (ADRs)

### Core Architecture

| # | Title | File |
|---|-------|------|
| 001 | tokio-tungstenite for OKX WS | [ADR-001-20260716-okx-websocket-market-data-client.md](ADR-001-20260716-okx-websocket-market-data-client.md) |
| 002 | BTreeMap<OrderedFloat> for LOB2 | [ADR-002-20260716-lob2-in-memory-order-book-reconstruction.md](ADR-002-20260716-lob2-in-memory-order-book-reconstruction.md) |
| 003 | QuestDB with refinery for persistence | [ADR-003-20260716-questdb-persistence-with-refinery-migrations.md](ADR-003-20260716-questdb-persistence-with-refinery-migrations.md) |

### Exchange Integration

| # | Title | File |
|---|-------|------|
| 015 | Kraken exchange module | [ADR-015-20260722-kraken-exchange-module.md](ADR-015-20260722-kraken-exchange-module.md) |
| 016 | Exchange column in DB schema | [ADR-016-20260723-exchange-column-schema.md](ADR-016-20260723-exchange-column-schema.md) |
| 017 | Bitstamp with shared trait abstraction | [ADR-017-20260723-bitstamp-exchange-with-shared-trait-abstraction-layer.md](ADR-017-20260723-bitstamp-exchange-with-shared-trait-abstraction-layer.md) |
| 018 | Bitstamp diff_order_book reconciliation | [ADR-018-20260723-bitstamp-diff-order-book-reconciliation.md](ADR-018-20260723-bitstamp-diff-order-book-reconciliation.md) |
| 019 | Instrument mapping via config | [ADR-019-20260723-instrument-mapping-config.md](ADR-019-20260723-instrument-mapping-config.md) |
| 020 | `--list-instruments` CLI flag | [ADR-020-20260723-list-instruments-flag.md](ADR-020-20260723-list-instruments-flag.md) |
| 021 | Instrument fallback rules | [ADR-021-20260724-instrument-fallback-lowercase-inst-id.md](ADR-021-20260724-instrument-fallback-lowercase-inst-id.md) |
| 022 | Region-based exchange URLs | [ADR-022-20260724-region-based-exchange-urls.md](ADR-022-20260724-region-based-exchange-urls.md) |
| 024 | Multi-instrument support | [ADR-024-20260724-multi-instrument-support.md](ADR-024-20260724-multi-instrument-support.md) |

### Persistence & Storage

| # | Title | File |
|---|-------|------|
| 004 | Normalized LOB levels storage | [ADR-004-20260716-normalized-lob-levels-storage.md](ADR-004-20260716-normalized-lob-levels-storage.md) |
| 005 | QuestDB persistence cleanup | [ADR-005-20260720-questdb-persistence-cleanup.md](ADR-005-20260720-questdb-persistence-cleanup.md) |
| 008 | QuestDB TTL for automatic retention | [ADR-008-20260721-questdb-storage-policy.md](ADR-008-20260721-questdb-storage-policy.md) |
| 010 | Move TTL execution to startup | [ADR-010-20260721-move-ttl-execution-to-startup.md](ADR-010-20260721-move-ttl-execution-to-startup.md) |
| 023 | Consolidate refinery migrations | [ADR-023-20260724-consolidate-refinery-migrations.md](ADR-023-20260724-consolidate-refinery-migrations.md) |

### Metrics & Visualization

| # | Title | File |
|---|-------|------|
| 006 | Grafana LOB visualization | [ADR-006-20260721-grafana-lob-visualization.md](ADR-006-20260721-grafana-lob-visualization.md) |
| 007 | Data output flag | [ADR-007-20260721-data-output-flag.md](ADR-007-20260721-data-output-flag.md) |
| 009 | Grafana Infinity datasource | [ADR-009-20260721-grafana-infinity-datasource.md](ADR-009-20260721-grafana-infinity-datasource.md) |
| 011 | Serve /metrics as JSON | [ADR-011-20260721-serve-metrics-as-json.md](ADR-011-20260721-serve-metrics-as-json.md) |
| 013 | Restructure /metrics to single JSON object | [ADR-013-20260722-restructure-metrics-endpoint.md](ADR-013-20260722-restructure-metrics-endpoint.md) |
| 026 | Metrics instrument label format | [ADR-026-20260724-metrics-instrument-label-format.md](ADR-026-20260724-metrics-instrument-label-format.md) |

### Operations

| # | Title | File |
|---|-------|------|
| 012 | Exponential backoff for WS reconnect | [ADR-012-20260722-websocket-reconnect-with-exponential-backoff.md](ADR-012-20260722-websocket-reconnect-with-exponential-backoff.md) |
| 014 | Graceful shutdown for SIGINT/SIGTERM | [ADR-014-20260722-graceful-shutdown-signal-handling.md](ADR-014-20260722-graceful-shutdown-signal-handling.md) |

## GitHub Wiki

The [GitHub Wiki Topic Index](https://github.com/fibonsai/cryptomeria/wiki/Topic-Index) mirrors this index and provides the same content organized for wiki browsing.
