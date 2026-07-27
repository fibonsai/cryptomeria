# Graph Report - .  (2026-07-27)

## Corpus Check
- 83 files · ~216,880 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 638 nodes · 1355 edges · 36 communities (29 shown, 7 thin omitted)
- Extraction: 99% EXTRACTED · 1% INFERRED · 0% AMBIGUOUS · INFERRED: 12 edges (avg confidence: 0.8)
- Token cost: 47,712 input · 1,769 output

## Community Hubs (Navigation)
- Bitstamp WebSocket Client
- OKX Order Book
- Bitstamp Order Book
- Kraken WebSocket Client
- OKX WebSocket Client
- Bitstamp Message Types
- OKX Message Types
- Kraken Order Book
- Kraken Message Types
- Python LOB CLI
- Shared Traits & Metrics
- OpenCode Configuration
- Instrument Resolution
- Currency Fallback
- Instrument Parsing
- CLI Argument Parsing
- Instrument Formatting
- OpenCode Plugin Config
- Graphify Plugin
- Aliases Script
- Bitstamp ADR-017
- Bitstamp ADR-018
- Multi-Instrument ADR-024
- Execute Plan Command
- Python Package Config

## God Nodes (most connected - your core abstractions)
1. `OrderBook` - 25 edges
2. `BitstampClient` - 25 edges
3. `KrakenClient` - 24 edges
4. `OkxClient` - 24 edges
5. `OrderBook` - 23 edges
6. `OrderBook` - 23 edges
7. `price_level()` - 19 edges
8. `OkxWsMessage` - 19 edges
9. `LobMetrics` - 19 edges
10. `KrakenWsMessage` - 18 edges

## Surprising Connections (you probably didn't know these)
- `test_apply_level_null_price_skipped()` --calls--> `_apply_level()`  [EXTRACTED]
  python/tests/test_lob.py → python/cryptomeria/lob.py
- `test_apply_level_null_price_snapshot_skipped()` --calls--> `_apply_level()`  [EXTRACTED]
  python/tests/test_lob.py → python/cryptomeria/lob.py
- `test_apply_level_snapshot_inserts()` --calls--> `_apply_level()`  [EXTRACTED]
  python/tests/test_lob.py → python/cryptomeria/lob.py
- `test_apply_level_snapshot_overwrites()` --calls--> `_apply_level()`  [EXTRACTED]
  python/tests/test_lob.py → python/cryptomeria/lob.py
- `test_apply_level_update_nonzero_overwrites()` --calls--> `_apply_level()`  [EXTRACTED]
  python/tests/test_lob.py → python/cryptomeria/lob.py

## Import Cycles
- None detected.

## Hyperedges (group relationships)
- **OpenCode Agent Workflow** — opencode_commands_execute_plan [EXTRACTED 1.00]

## Communities (36 total, 7 thin omitted)

### Community 0 - "Bitstamp WebSocket Client"
Cohesion: 0.07
Nodes (44): Buffer, Client, LobLevel, BitstampClient, build_subscribe_msg(), instrument_to_channel(), Arc, AtomicU64 (+36 more)

### Community 1 - "OKX Order Book"
Cohesion: 0.10
Nodes (40): OrderBook, parse_level(), parse_levels(), price_level(), BTreeMap, Default, Item, Iterator (+32 more)

### Community 2 - "Bitstamp Order Book"
Cohesion: 0.10
Nodes (36): entry(), msg_from_entry(), ob_data(), OrderBook, OrderInfo, BTreeMap, Default, Item (+28 more)

### Community 3 - "Kraken WebSocket Client"
Cohesion: 0.07
Nodes (28): HashMap, build_subscribe_msg(), display_message(), KrakenClient, Arc, AtomicU64, Box, Error (+20 more)

### Community 4 - "OKX WebSocket Client"
Cohesion: 0.09
Nodes (30): build_subscribe_msg(), display_message(), OkxClient, Arc, AtomicU64, Box, Error, Option (+22 more)

### Community 5 - "Bitstamp Message Types"
Cohesion: 0.09
Nodes (35): BitstampWsMessage, deserialize_number_or_string(), deserialize_number_or_zero(), display_message(), LobLevel, MessageType, D, Error (+27 more)

### Community 6 - "OKX Message Types"
Cohesion: 0.10
Nodes (33): ChannelArg, format_top_levels(), LobData, LobLevel, MessageType, OkxWsMessage, Error, Option (+25 more)

### Community 7 - "Kraken Order Book"
Cohesion: 0.11
Nodes (24): OrderBook, parse_level(), parse_levels(), BTreeMap, Default, Item, Iterator, Option (+16 more)

### Community 8 - "Kraken Message Types"
Cohesion: 0.10
Nodes (33): deserialize_number_or_string(), format_top_levels(), KrakenWsMessage, LobData, LobLevel, MessageType, parse_kraken_timestamp(), D (+25 more)

### Community 9 - "Python LOB CLI"
Cohesion: 0.11
Nodes (38): command, _apply_level(), main(), Path, OKX LOB parquet stream reader.  Reads raw LOB parquet files (price_ask/price_bid, Read raw LOB parquet and write LOB2 snapshots (JSON arrays) to parquet., Rebuild LOB2 snapshots from OKX LOB parquet data., Apply a single price level update or snapshot to a levels dict. (+30 more)

### Community 10 - "Shared Traits & Metrics"
Cohesion: 0.11
Nodes (27): Duration, GaugeVec, IntGaugeVec, MetricFamily, Registry, backoff_delay(), ClientStatus, LobMetrics (+19 more)

### Community 12 - "OpenCode Configuration"
Cohesion: 0.10
Nodes (20): name, x-bf-cache-key, x-bf-cache-type, default, models, name, npm, options (+12 more)

### Community 13 - "Instrument Resolution"
Cohesion: 0.17
Nodes (12): map_exchange_to_id(), resolve_one_instrument(), test_resolve_instrument_at_overrides_exchange(), test_resolve_instrument_eur_no_fallback_bitstamp(), test_resolve_instrument_eur_no_fallback_kraken(), test_resolve_instrument_eur_resolved_okx(), test_resolve_instrument_no_aliases_fallback_bitstamp(), test_resolve_instrument_no_aliases_fallback_kraken() (+4 more)

### Community 14 - "Currency Fallback"
Cohesion: 0.22
Nodes (9): find_fallback_target(), test_find_fallback_target_exact_match(), test_find_fallback_target_no_fallback(), test_find_fallback_target_other_target_no_fallback(), test_find_fallback_target_usd_to_usdc(), test_find_fallback_target_usd_to_usdt(), test_find_fallback_target_usdc_to_usdt(), test_find_fallback_target_usdc_usdt_to_usd() (+1 more)

### Community 15 - "Instrument Parsing"
Cohesion: 0.33
Nodes (6): parse_instruments_list(), Vec, test_parse_instruments_format1(), test_parse_instruments_format2(), test_parse_instruments_format3(), test_parse_instruments_format4_hybrid()

### Community 16 - "CLI Argument Parsing"
Cohesion: 0.40
Nodes (5): CliArgs, parse_args(), parse_exchange_override(), Option, test_parse_exchange_override_with_at()

### Community 17 - "Instrument Formatting"
Cohesion: 0.40
Nodes (5): format_instrument(), print_instrument_table(), ResolvedInstrument, String, test_print_instrument_table_does_not_panic()

### Community 18 - "OpenCode Plugin Config"
Cohesion: 0.50
Nodes (3): plugin, $schema, .opencode/plugins/graphify.js

## Knowledge Gaps
- **20 isolated node(s):** `$schema`, `.opencode/plugins/graphify.js`, `$schema`, `.opencode/skills`, `npm` (+15 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **7 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `OkxWsMessage` connect `OKX Message Types` to `OKX Order Book`, `OKX WebSocket Client`?**
  _High betweenness centrality (0.241) - this node is a cross-community bridge._
- **Why does `KrakenWsMessage` connect `Kraken Message Types` to `Kraken WebSocket Client`, `Kraken Order Book`?**
  _High betweenness centrality (0.205) - this node is a cross-community bridge._
- **Why does `OrderBook` connect `Bitstamp Order Book` to `Kraken WebSocket Client`?**
  _High betweenness centrality (0.149) - this node is a cross-community bridge._
- **What connects `$schema`, `.opencode/plugins/graphify.js`, `$schema` to the rest of the system?**
  _20 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `Bitstamp WebSocket Client` be split into smaller, more focused modules?**
  _Cohesion score 0.0710085933966531 - nodes in this community are weakly interconnected._
- **Should `OKX Order Book` be split into smaller, more focused modules?**
  _Cohesion score 0.1016949152542373 - nodes in this community are weakly interconnected._
- **Should `Bitstamp Order Book` be split into smaller, more focused modules?**
  _Cohesion score 0.09898989898989899 - nodes in this community are weakly interconnected._