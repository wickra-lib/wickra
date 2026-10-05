//! Binance spot REST historical kline fetcher.
//!
//! Where [`super::binance`] streams *live* klines over a WebSocket, this module
//! pulls *historical* candles from Binance's public REST endpoint
//! (`GET /api/v3/klines`) with a single blocking request. It is the native,
//! dependency-free replacement for hand-rolled `urllib` / `jackson` / `jsonlite`
//! download helpers in every binding.
//!
//! Example (requires the `live-binance` feature):
//!
//! ```no_run
//! use wickra_data::live::binance::Interval;
//! use wickra_data::live::binance_rest::fetch_klines;
//! # fn run() -> wickra_data::Result<()> {
//! let candles = fetch_klines("BTCUSDT", Interval::OneHour, 500, None, None)?;
//! println!("got {} candles", candles.len());
//! # Ok(()) }
//! ```

use std::fmt::Write as _;

use serde::Deserialize;

use super::binance::Interval;
use crate::error::{Error, Result};
use wickra_core::Candle;

/// Binance allows at most 1000 klines per REST request.
const MAX_LIMIT: u16 = 1000;

/// Endpoint configuration for [`fetch_klines_with_config`]. The default points
/// at Binance's public production REST host; tests and Testnet users override
/// `base_url` to aim the request elsewhere.
#[derive(Debug, Clone)]
pub struct BinanceRestConfig {
    /// REST host **without** path, e.g. `"https://api.binance.com"`. The
    /// `/api/v3/klines` path and query string are appended internally.
    pub base_url: String,
}

impl Default for BinanceRestConfig {
    fn default() -> Self {
        Self {
            base_url: "https://api.binance.com".to_string(),
        }
    }
}

/// One row of Binance's `/api/v3/klines` response. The wire format is a fixed
/// 12-element JSON array of mixed types; only the first seven fields carry the
/// OHLCV candle, the rest are ignored. Deserializing positionally lets serde
/// reject a malformed shape before we touch the numbers.
#[derive(Debug, Deserialize)]
struct RawRestKline(
    i64,                   // open time (ms)
    String,                // open
    String,                // high
    String,                // low
    String,                // close
    String,                // volume
    serde::de::IgnoredAny, // close time
    serde::de::IgnoredAny, // quote asset volume
    serde::de::IgnoredAny, // number of trades
    serde::de::IgnoredAny, // taker buy base volume
    serde::de::IgnoredAny, // taker buy quote volume
    serde::de::IgnoredAny, // unused
);

/// Parse one of the five OHLCV string fields into an `f64`, tagging the field
/// name on failure so a bad payload is diagnosable.
fn parse_field(raw: &str, field: &str) -> Result<f64> {
    raw.parse()
        .map_err(|_| Error::Malformed(format!("bad {field} '{raw}'")))
}

impl RawRestKline {
    fn into_candle(self) -> Result<Candle> {
        let open = parse_field(&self.1, "open")?;
        let high = parse_field(&self.2, "high")?;
        let low = parse_field(&self.3, "low")?;
        let close = parse_field(&self.4, "close")?;
        let volume = parse_field(&self.5, "volume")?;
        Ok(Candle::new(open, high, low, close, volume, self.0)?)
    }
}

/// Fetch historical klines from Binance's production endpoint.
///
/// `symbol` is upper-cased to match Binance's convention; `limit` must be in
/// `1..=1000`. `start_ms` / `end_ms` are optional inclusive Unix-millisecond
/// bounds (`None` lets Binance pick the most recent `limit` candles). Returns
/// the candles in ascending open-time order.
///
/// # Errors
/// Returns [`Error::Malformed`] for an out-of-range `limit` or an unparseable
/// price field, [`Error::Http`] for a transport or non-2xx response,
/// [`Error::Json`] for a malformed body, and [`Error::Core`] when a row's OHLC
/// values violate candle invariants.
pub fn fetch_klines(
    symbol: &str,
    interval: Interval,
    limit: u16,
    start_ms: Option<i64>,
    end_ms: Option<i64>,
) -> Result<Vec<Candle>> {
    fetch_klines_with_config(
        symbol,
        interval,
        limit,
        start_ms,
        end_ms,
        &BinanceRestConfig::default(),
    )
}

/// Like [`fetch_klines`] but against a custom [`BinanceRestConfig`] (Testnet or
/// a local mock server).
///
/// # Errors
/// See [`fetch_klines`].
pub fn fetch_klines_with_config(
    symbol: &str,
    interval: Interval,
    limit: u16,
    start_ms: Option<i64>,
    end_ms: Option<i64>,
    config: &BinanceRestConfig,
) -> Result<Vec<Candle>> {
    if limit == 0 || limit > MAX_LIMIT {
        return Err(Error::Malformed(format!(
            "limit must be in 1..={MAX_LIMIT}, got {limit}"
        )));
    }
    let mut url = format!(
        "{}/api/v3/klines?symbol={}&interval={}&limit={}",
        config.base_url,
        symbol.to_uppercase(),
        interval.as_str(),
        limit
    );
    if let Some(start) = start_ms {
        let _ = write!(url, "&startTime={start}");
    }
    if let Some(end) = end_ms {
        let _ = write!(url, "&endTime={end}");
    }

    // ureq 2.x does not auto-configure native-tls, so build an agent with an
    // explicit native-tls connector (verifies against the OS trust store; no
    // bundled CA roots). The connector is cheap and a one-shot fetch needs no
    // pooling, so we build it per call.
    let connector = native_tls::TlsConnector::new()
        .map_err(|e| Error::Malformed(format!("native-tls init failed: {e}")))?;
    let agent = ureq::AgentBuilder::new()
        .tls_connector(std::sync::Arc::new(connector))
        .build();
    let body = agent.get(&url).call().map_err(Box::new)?.into_string()?;
    let rows: Vec<RawRestKline> = serde_json::from_str(&body)?;
    rows.into_iter().map(RawRestKline::into_candle).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread::JoinHandle;

    /// A canonical single-row klines response (12-element array, OHLCV in the
    /// first seven slots) — mirrors Binance's documented wire format.
    fn sample_response() -> String {
        r#"[[1700000000000,"30000.0","30100.0","29950.0","30050.0","12.5",1700000059999,"375000.0",50,"6.25","187500.0","0"]]"#.to_string()
    }

    /// Spawn a one-shot mock HTTP server: accept a single connection, read the
    /// request, and reply `200 OK` with `body`. Returns the base URL plus a
    /// [`JoinHandle`] the test joins so the handler thread always reaches its
    /// closing brace (every step `.unwrap()`s — a failure is a test bug).
    fn mock_http(status_line: &'static str, body: String) -> (String, JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf).unwrap();
            let response = format!(
                "{status_line}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).unwrap();
        });
        (format!("http://{addr}"), handle)
    }

    #[test]
    fn fetch_parses_a_real_klines_response() {
        let (base, handle) = mock_http("HTTP/1.1 200 OK", sample_response());
        let candles = fetch_klines_with_config(
            "btcusdt",
            Interval::OneHour,
            1,
            Some(1_700_000_000_000),
            Some(1_700_000_059_999),
            &BinanceRestConfig { base_url: base },
        )
        .unwrap();
        handle.join().unwrap();
        assert_eq!(candles.len(), 1);
        assert_eq!(candles[0].open, 30_000.0);
        assert_eq!(candles[0].high, 30_100.0);
        assert_eq!(candles[0].low, 29_950.0);
        assert_eq!(candles[0].close, 30_050.0);
        assert_eq!(candles[0].volume, 12.5);
        assert_eq!(candles[0].timestamp, 1_700_000_000_000);
    }

    #[test]
    fn fetch_handles_an_empty_result() {
        let (base, handle) = mock_http("HTTP/1.1 200 OK", "[]".to_string());
        let candles = fetch_klines_with_config(
            "BTCUSDT",
            Interval::OneMinute,
            10,
            None,
            None,
            &BinanceRestConfig { base_url: base },
        )
        .unwrap();
        handle.join().unwrap();
        assert_eq!(candles.len(), 0);
    }

    #[test]
    fn fetch_rejects_a_zero_limit() {
        let err = fetch_klines("BTCUSDT", Interval::OneHour, 0, None, None).unwrap_err();
        assert!(matches!(err, Error::Malformed(_)));
    }

    #[test]
    fn fetch_rejects_a_limit_above_the_cap() {
        let err =
            fetch_klines("BTCUSDT", Interval::OneHour, MAX_LIMIT + 1, None, None).unwrap_err();
        assert!(matches!(err, Error::Malformed(_)));
    }

    #[test]
    fn fetch_surfaces_a_transport_error_for_an_unreachable_host() {
        // Port 1 is privileged and unbound — the connection is refused.
        let err = fetch_klines_with_config(
            "BTCUSDT",
            Interval::OneHour,
            1,
            None,
            None,
            &BinanceRestConfig {
                base_url: "http://127.0.0.1:1".to_string(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, Error::Http(_)));
    }

    #[test]
    fn fetch_surfaces_a_json_error_for_a_malformed_body() {
        let (base, handle) = mock_http("HTTP/1.1 200 OK", "not json".to_string());
        let err = fetch_klines_with_config(
            "BTCUSDT",
            Interval::OneHour,
            1,
            None,
            None,
            &BinanceRestConfig { base_url: base },
        )
        .unwrap_err();
        handle.join().unwrap();
        assert!(matches!(err, Error::Json(_)));
    }

    #[test]
    fn fetch_rejects_an_unparseable_price_field() {
        let body =
            r#"[[1700000000000,"not-a-number","0","0","0","0",0,"0",0,"0","0","0"]]"#.to_string();
        let (base, handle) = mock_http("HTTP/1.1 200 OK", body);
        let err = fetch_klines_with_config(
            "BTCUSDT",
            Interval::OneHour,
            1,
            None,
            None,
            &BinanceRestConfig { base_url: base },
        )
        .unwrap_err();
        handle.join().unwrap();
        assert!(matches!(err, Error::Malformed(_)));
    }

    #[test]
    fn fetch_rejects_a_row_that_violates_candle_invariants() {
        // high (1) below low (100) — Candle::new rejects it.
        let body = r#"[[1700000000000,"50","1","100","50","1",0,"0",0,"0","0","0"]]"#.to_string();
        let (base, handle) = mock_http("HTTP/1.1 200 OK", body);
        let err = fetch_klines_with_config(
            "BTCUSDT",
            Interval::OneHour,
            1,
            None,
            None,
            &BinanceRestConfig { base_url: base },
        )
        .unwrap_err();
        handle.join().unwrap();
        assert!(matches!(err, Error::Core(_)));
    }

    #[test]
    fn rest_config_default_targets_production() {
        assert_eq!(
            BinanceRestConfig::default().base_url,
            "https://api.binance.com"
        );
    }
}
