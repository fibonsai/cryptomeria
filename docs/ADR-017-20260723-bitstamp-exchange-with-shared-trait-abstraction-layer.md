# ADR-017: Bitstamp exchange with shared trait abstraction layer

## Context

The project supports market data ingestion from public WebSocket APIs (OKX and Kraken). Adding a third exchange (Bitstamp) revealed code duplication and tight coupling between exchange-specific implementations. Specifically:

- `LobMetrics`, `backoff_delay`, `signal_sleep`, and `start_metrics_server` were defined in `okx/ws.rs` and either duplicated or re-imported by `kraken/ws.rs`
- Both `OkxClient` and `KrakenClient` had identical builder patterns (`with_sender`, `with_retention_window`, `with_metrics_port`, `with_data_output`)
- Both `OrderBook` types used `BTreeMap<OrderedFloat<f64>, f64>` with identical method signatures but no shared trait
- Adding a third exchange would compound the duplication without an abstraction layer

## Options Considered

### Option 1: Continue per-exchange copy-paste

Duplicate the client and order book code for each exchange, as was done for Kraken after OKX.

- **Pros**: Simple, minimal upfront design cost
- **Cons**: High maintenance burden; bugs fixed in one exchange may not be fixed in others; growing codebase becomes unwieldy

### Option 2: Extract shared traits and utilities

Define `OrderBook` and `ExchangeClientBuilder` traits in a shared `traits/` module, move `LobMetrics` and utilities (`backoff_delay`, `signal_sleep`, `start_metrics_server`) there, and have each exchange implement the traits via delegation to inherent methods.

- **Pros**: DRY — shared logic lives in one place; new exchanges follow a consistent pattern; traits act as documentation for what each exchange must provide
- **Cons**: Trait implementation requires explicit delegation boilerplate (empty `impl Trait for Type {}` does not auto-match inherent methods); some methods (`process_msg`) remain exchange-specific due to differing WS message formats

### Option 3: Full generic trait with associated types

Make the `OrderBook` trait fully generic with an associated `Message` type, so `process_msg` is also part of the trait.

- **Pros**: Complete type safety and polymorphism
- **Cons**: Requires the `ExchangeClient` trait to also be generic, propagating type parameters throughout the codebase; increases complexity without practical benefit since each exchange only uses its own types

## Decision

Adopt **Option 2**: extract shared traits (`OrderBook`, `ExchangeClientBuilder`), utilities (`backoff_delay`, `signal_sleep`), and `LobMetrics` (including `start_metrics_server`) into a `rs/src/traits/` module. Each exchange module:
- Implements `OrderBook` via explicit delegation to inherent methods
- Implements `ExchangeClientBuilder` via explicit delegation to builder methods
- Imports `LobMetrics`, `backoff_delay`, `signal_sleep` from `traits` instead of defining them locally

Exchange-specific methods (`process_msg`, `run`) remain on each exchange's inherent impl since the WS message formats differ.

## Consequences

**Positive**:
- Adding future exchanges requires only the exchange-specific types, order book logic, and WS client — shared utilities come from `traits/`
- `LobMetrics` and `start_metrics_server` are no longer tied to `OkxClient`, making them usable by any exchange
- Codebase is more readable and maintainable — shared patterns are explicit

**Negative**:
- Trait impl requires delegation boilerplate (~8 lines per trait per exchange)
- `process_msg` is not part of the shared trait, so each exchange defines its own dispatch logic
- The `OkxClient::start_metrics_server` public API is removed in favor of `LobMetrics::start_metrics_server`

## Status

Accepted
