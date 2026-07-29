# Documentation Topic Index

> The [GitHub Wiki](https://github.com/fibonsai/cryptomeria/wiki) is the canonical source for all documentation. This file is a local mirror for offline reference — links below point to the wiki.

## Extracted Docs

| Topic | Wiki Page | Description |
|-------|-----------|-------------|
| Project Structure | [Project-Structure](https://github.com/fibonsai/cryptomeria/wiki/Project-Structure) | Full directory tree with module descriptions |
| QuestDB Persistence | [QuestDB-Persistence](https://github.com/fibonsai/cryptomeria/wiki/QuestDB-Persistence) | Configuration, data retention, schema reference |
| Exchange Comparison | [Exchange-Comparison](https://github.com/fibonsai/cryptomeria/wiki/Exchange-Comparison) | LOB/trade strategies, delivery models, pros/cons |
| Grafana LOB Visualization | [Grafana-LOB-Visualization](https://github.com/fibonsai/cryptomeria/wiki/Grafana-LOB-Visualization) | Metrics endpoint, dashboard layout, setup |
| LOB Data Processing | [LOB-Data-Processing](https://github.com/fibonsai/cryptomeria/wiki/LOB-Data-Processing) | Parquet stream reader, key rules, CLI usage |

## Architecture Decision Records (ADRs)

All ADRs are published on the [GitHub Wiki Topic Index](https://github.com/fibonsai/cryptomeria/wiki/Topic-Index#architecture-decision-records-adrs), organized by category.

### Core Architecture

| # | Title | Wiki Page |
|---|-------|-----------|
| 001 | tokio-tungstenite for OKX WS | [ADR-001](https://github.com/fibonsai/cryptomeria/wiki/ADR-001-20260716-okx-websocket-market-data-client) |
| 002 | BTreeMap<OrderedFloat> for LOB2 | [ADR-002](https://github.com/fibonsai/cryptomeria/wiki/ADR-002-20260716-lob2-in-memory-order-book-reconstruction) |
| 003 | QuestDB with refinery for persistence | [ADR-003](https://github.com/fibonsai/cryptomeria/wiki/ADR-003-20260716-questdb-persistence-with-refinery-migrations) |

### Exchange Integration

| # | Title | Wiki Page |
|---|-------|-----------|
| 015 | Kraken exchange module | [ADR-015](https://github.com/fibonsai/cryptomeria/wiki/ADR-015-20260722-kraken-exchange-module) |
| 016 | Exchange column in DB schema | [ADR-016](https://github.com/fibonsai/cryptomeria/wiki/ADR-016-20260723-exchange-column-schema) |
| 017 | Bitstamp with shared trait abstraction | [ADR-017](https://github.com/fibonsai/cryptomeria/wiki/ADR-017-20260723-bitstamp-exchange-with-shared-trait-abstraction-layer) |
| 018 | Bitstamp diff_order_book reconciliation | [ADR-018](https://github.com/fibonsai/cryptomeria/wiki/ADR-018-20260723-bitstamp-diff-order-book-reconciliation) |
| 019 | Instrument mapping via config | [ADR-019](https://github.com/fibonsai/cryptomeria/wiki/ADR-019-20260723-instrument-mapping-config) |
| 020 | `--list-instruments` CLI flag | [ADR-020](https://github.com/fibonsai/cryptomeria/wiki/ADR-020-20260723-list-instruments-flag) |
| 021 | Instrument fallback rules | [ADR-021](https://github.com/fibonsai/cryptomeria/wiki/ADR-021-20260724-instrument-fallback-lowercase-inst-id) |
| 022 | Region-based exchange URLs | [ADR-022](https://github.com/fibonsai/cryptomeria/wiki/ADR-022-20260724-region-based-exchange-urls) |
| 024 | Multi-instrument support | [ADR-024](https://github.com/fibonsai/cryptomeria/wiki/ADR-024-20260724-multi-instrument-support) |

### Persistence & Storage

| # | Title | Wiki Page |
|---|-------|-----------|
| 004 | Normalized LOB levels storage | [ADR-004](https://github.com/fibonsai/cryptomeria/wiki/ADR-004-20260716-normalized-lob-levels-storage) |
| 005 | QuestDB persistence cleanup | [ADR-005](https://github.com/fibonsai/cryptomeria/wiki/ADR-005-20260720-questdb-persistence-cleanup) |
| 008 | QuestDB TTL for automatic retention | [ADR-008](https://github.com/fibonsai/cryptomeria/wiki/ADR-008-20260721-questdb-storage-policy) |
| 010 | Move TTL execution to startup | [ADR-010](https://github.com/fibonsai/cryptomeria/wiki/ADR-010-20260721-move-ttl-execution-to-startup) |
| 023 | Consolidate refinery migrations | [ADR-023](https://github.com/fibonsai/cryptomeria/wiki/ADR-023-20260724-consolidate-refinery-migrations) |
| 032 | Dual-protocol QuestDB migration (PG wire + ILP) | [ADR-032](https://github.com/fibonsai/cryptomeria/wiki/ADR-032-20260729-dual-protocol-questdb-migration-strategy) |
| 033 | HTTP-only versioned migration runner | [ADR-033](https://github.com/fibonsai/cryptomeria/wiki/ADR-033-20260729-http-only-versioned-migration-runner) |

### Metrics & Visualization

| # | Title | Wiki Page |
|---|-------|-----------|
| 006 | Grafana LOB visualization | [ADR-006](https://github.com/fibonsai/cryptomeria/wiki/ADR-006-20260721-grafana-lob-visualization) |
| 007 | Data output flag | [ADR-007](https://github.com/fibonsai/cryptomeria/wiki/ADR-007-20260721-data-output-flag) |
| 009 | Grafana Infinity datasource | [ADR-009](https://github.com/fibonsai/cryptomeria/wiki/ADR-009-20260721-grafana-infinity-datasource) |
| 011 | Serve /metrics as JSON | [ADR-011](https://github.com/fibonsai/cryptomeria/wiki/ADR-011-20260721-serve-metrics-as-json) |
| 013 | Restructure /metrics to single JSON object | [ADR-013](https://github.com/fibonsai/cryptomeria/wiki/ADR-013-20260722-restructure-metrics-endpoint) |
| 026 | Metrics instrument label format | [ADR-026](https://github.com/fibonsai/cryptomeria/wiki/ADR-026-20260724-metrics-instrument-label-format) |

### Operations

| # | Title | Wiki Page |
|---|-------|-----------|
| 012 | Exponential backoff for WS reconnect | [ADR-012](https://github.com/fibonsai/cryptomeria/wiki/ADR-012-20260722-websocket-reconnect-with-exponential-backoff) |
| 014 | Graceful shutdown for SIGINT/SIGTERM | [ADR-014](https://github.com/fibonsai/cryptomeria/wiki/ADR-014-20260722-graceful-shutdown-signal-handling) |
| 028 | Upload ADRs to GitHub Wiki | [ADR-028](https://github.com/fibonsai/cryptomeria/wiki/ADR-028-20260727-upload-adrs-to-github-wiki) |
| 029 | Apache 2.0 license with brand protection | [ADR-029](https://github.com/fibonsai/cryptomeria/wiki/ADR-029-20260727-apache-license-brand-protection) |
| 030 | GitHub Actions CI for automated tests and lint | [ADR-030](https://github.com/fibonsai/cryptomeria/wiki/ADR-030-20260727-github-actions-ci) |
| 034 | Structured logging facade with rasant | [ADR-034](https://github.com/fibonsai/cryptomeria/wiki/ADR-034-20260729-logging-facade-rasant) |

## Governance Files

| File | Description |
|------|-------------|
| [CONTRIBUTIONS.md](CONTRIBUTIONS.md) | Contribution guidelines and code standards |
| [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) | Community code of conduct |
| [SECURITY.md](SECURITY.md) | Security policy and vulnerability reporting |
| [LICENSE](LICENSE) | Apache License 2.0 with brand protection terms |
