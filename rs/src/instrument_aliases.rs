/// Embedded instrument aliases for supported exchanges.
/// Generated from scripts/coins_aliases.json (filtered for okex, kraken, bitstamp).
/// Each entry: (base, target, exchange_id)

pub const COIN_ALIASES: &[(&str, &str, &str)] = &[
    // OKX (exchange_id = "okx")
    ("BTC", "EUR", "okx"),
    ("ETH", "EUR", "okx"),
    ("SOL", "EUR", "okx"),
    ("LTC", "EUR", "okx"),
    ("XLM", "EUR", "okx"),
    ("ADA", "EUR", "okx"),
    ("DOGE", "EUR", "okx"),
    ("BTC", "USDT", "okx"),
    ("BTC", "USDC", "okx"),
    ("ETH", "USDT", "okx"),
    ("ETH", "USD", "okx"),
    ("SOL", "USDT", "okx"),
    ("SOL", "USD", "okx"),
    ("LTC", "USDT", "okx"),
    ("XLM", "USDT", "okx"),
    ("ADA", "USDT", "okx"),
    ("ADA", "USD", "okx"),
    ("DOGE", "USDT", "okx"),
    ("DOGE", "USDC", "okx"),

    // Kraken (exchange_id = "kraken")
    ("XBT", "USD", "kraken"),
    ("XBT", "EUR", "kraken"),
    ("XBT", "USDC", "kraken"),
    ("ETH", "USD", "kraken"),
    ("ETH", "EUR", "kraken"),
    ("SOL", "USD", "kraken"),
    ("SOL", "EUR", "kraken"),
    ("LTC", "USD", "kraken"),
    ("LTC", "EUR", "kraken"),
    ("LTC", "GBP", "kraken"),
    ("LTC", "USDT", "kraken"),
    ("XLM", "USD", "kraken"),
    ("XLM", "EUR", "kraken"),
    ("ADA", "USD", "kraken"),
    ("ADA", "EUR", "kraken"),
    ("ADA", "USDC", "kraken"),
    ("XDG", "USD", "kraken"),
    ("XDG", "USDT", "kraken"),
    ("XDG", "EUR", "kraken"),

    // Bitstamp (exchange_id = "bitstamp")
    ("BTC", "USD", "bitstamp"),
    ("ETH", "USD", "bitstamp"),
    ("SOL", "USD", "bitstamp"),
    ("LTC", "USD", "bitstamp"),
    ("XLM", "USD", "bitstamp"),
    ("XLM", "EUR", "bitstamp"),
    ("ADA", "USD", "bitstamp"),
    ("ADA", "EUR", "bitstamp"),
    ("DOGE", "USD", "bitstamp"),
];
