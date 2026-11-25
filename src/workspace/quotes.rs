//! Live prices from Yahoo Finance's public chart endpoint (no key), read through curl: the day in
//! five-minute steps every minute, the longer ranges every half hour.

use super::connect::J;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Quote {
    pub name: String,
    pub price: f32,
    /// Where the range started: yesterday's close for the day, the first close otherwise.
    pub base: f32,
    pub closes: Vec<f32>,
    /// Open, high, low, close per bar, bars with gaps left out.
    pub ohlc: Vec<[f32; 4]>,
    pub high: f32,
    pub low: f32,
    pub volume: f64,
    pub year: (f32, f32),
}

/// Yahoo's (range, interval) for each of the card's ranges: 1D 5D 1M 6M 1Y.
const RANGES: [(&str, &str); 5] = [("1d", "5m"), ("5d", "30m"), ("1mo", "1d"), ("6mo", "1d"), ("1y", "1d")];

pub fn parse(body: &str, range: usize) -> Option<Quote> {
    let j = J::parse(body)?;
    let r = j.get("chart")?.get("result")?.at(0)?;
    let m = r.get("meta")?;
    let n = |k: &str| m.get(k).and_then(J::num).unwrap_or(0.0);
    let q = r.get("indicators")?.get("quote")?.at(0)?;
    let col = |k: &str| q.get(k).and_then(J::arr).cloned().unwrap_or_default();
    let (o, h, l, c) = (col("open"), col("high"), col("low"), col("close"));
    let ohlc: Vec<[f32; 4]> = (0..c.len()).filter_map(|i| Some([o.get(i)?.num()? as f32, h.get(i)?.num()? as f32, l.get(i)?.num()? as f32, c[i].num()? as f32])).collect();
    let closes: Vec<f32> = c.iter().filter_map(J::num).map(|v| v as f32).collect();
    let price = n("regularMarketPrice") as f32;
    let base = if range == 0 { n("chartPreviousClose") as f32 } else { *closes.first()? };
    let name = m.get("shortName").or(m.get("longName")).and_then(J::str).unwrap_or_default();
    (price > 0.0 && base > 0.0).then(|| Quote {
        name: name.trim_end_matches(", Inc.").trim_end_matches(" Corporation").trim_end_matches(" Inc.").into(),
        price, base, closes, ohlc,
        high: n("regularMarketDayHigh") as f32, low: n("regularMarketDayLow") as f32,
        volume: n("regularMarketVolume"), year: (n("fiftyTwoWeekLow") as f32, n("fiftyTwoWeekHigh") as f32),
    })
}

pub fn fetch(sym: &str, range: usize) -> Option<Quote> {
    let (r, i) = RANGES[range.min(4)];
    let url = format!("https://query1.finance.yahoo.com/v8/finance/chart/{sym}?interval={i}&range={r}");
    let o = Command::new("curl").args(["-s", "-m", "10", "-A", "Mozilla/5.0", &url]).stderr(Stdio::null()).output().ok()?;
    parse(&String::from_utf8_lossy(&o.stdout), range)
}

/// (symbol, range, quote) as they arrive.
pub fn watch(syms: Vec<String>) -> Receiver<(String, usize, Quote)> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut long_at: Option<Instant> = None;
        loop {
            let long = long_at.is_none_or(|t| t.elapsed() > Duration::from_secs(1800));
            for s in &syms {
                for range in 0..if long { 5 } else { 1 } {
                    if let Some(q) = fetch(s, range) {
                        if tx.send((s.clone(), range, q)).is_err() {
                            return;
                        }
                    }
                }
            }
            if long {
                long_at = Some(Instant::now());
            }
            thread::sleep(Duration::from_secs(60));
        }
    });
    rx
}

#[cfg(test)]
mod tests {
    #[test]
    fn reads_chart_shape() {
        let body = r#"{"chart":{"result":[{"meta":{"shortName":"NVIDIA Corporation","regularMarketPrice":207.7,"chartPreviousClose":213.66,"regularMarketDayHigh":209.05,"regularMarketDayLow":205.71,"regularMarketVolume":22856730,"fiftyTwoWeekHigh":243.37,"fiftyTwoWeekLow":164.27},"indicators":{"quote":[{"open":[206.0,null,207.0],"high":[207.0,null,208.0],"low":[205.5,null,206.9],"close":[206.28,null,207.7]}]}}],"error":null}}"#;
        let q = super::parse(body, 0).unwrap();
        assert_eq!((q.name.as_str(), q.closes.len(), q.base), ("NVIDIA", 2, 213.66));
        assert!(q.price < q.base);
        assert_eq!(q.ohlc, vec![[206.0, 207.0, 205.5, 206.28], [207.0, 208.0, 206.9, 207.7]]);
        assert_eq!(super::parse(body, 2).unwrap().base, 206.28);
    }
}
