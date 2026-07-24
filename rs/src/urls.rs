use std::collections::HashMap;
use std::sync::LazyLock;

type EndpointDict = HashMap<&'static str, &'static str>;
type ExchangeDict = HashMap<&'static str, EndpointDict>;
type RegionDict = HashMap<&'static str, ExchangeDict>;

static EXCHANGE_URL: LazyLock<RegionDict> = LazyLock::new(|| {
    let mut map: RegionDict = HashMap::new();

    let mut global: ExchangeDict = HashMap::new();

    let mut okx_global = HashMap::new();
    okx_global.insert("websocket", "wss://ws.okx.com:8443/ws/v5/public");
    okx_global.insert("rest", "");
    global.insert("okx", okx_global);

    let mut kraken_global = HashMap::new();
    kraken_global.insert("websocket", "wss://ws.kraken.com/v2");
    kraken_global.insert("rest", "");
    global.insert("kraken", kraken_global);

    let mut bitstamp_global = HashMap::new();
    bitstamp_global.insert("websocket", "wss://ws.bitstamp.net");
    bitstamp_global.insert("rest", "https://www.bitstamp.net/api/v2");
    global.insert("bitstamp", bitstamp_global);

    map.insert("global", global);

    let mut europe: ExchangeDict = HashMap::new();

    let mut okx_europe = HashMap::new();
    okx_europe.insert("websocket", "wss://wseea.okx.com:8443/ws/v5/public");
    okx_europe.insert("rest", "");
    europe.insert("okx", okx_europe);

    let mut kraken_europe = HashMap::new();
    kraken_europe.insert("websocket", "wss://ws.kraken.com/v2");
    kraken_europe.insert("rest", "");
    europe.insert("kraken", kraken_europe);

    let mut bitstamp_europe = HashMap::new();
    bitstamp_europe.insert("websocket", "wss://ws.bitstamp.net");
    bitstamp_europe.insert("rest", "https://www.bitstamp.net/api/v2");
    europe.insert("bitstamp", bitstamp_europe);

    map.insert("europe", europe);

    map
});

pub fn websocket_url(region: &str, exchange: &str) -> &'static str {
    EXCHANGE_URL[region][exchange]["websocket"]
}

pub fn rest_url(region: &str, exchange: &str) -> &'static str {
    EXCHANGE_URL[region][exchange]["rest"]
}
