//! Optional diagnostic timing and real interface RX counters.
//! Global interface RX includes foreign traffic; never per-process attribution.
use std::time::{Duration, Instant};
pub(crate) struct Start {
    instant: Instant,
    rx: Option<u64>,
}
fn rx() -> Option<u64> {
    let interface = std::env::var("OXBLEND_RX_INTERFACE").ok()?;
    if interface.is_empty() || interface.contains('/') {
        return None;
    }
    std::fs::read_to_string(format!("/sys/class/net/{interface}/statistics/rx_bytes"))
        .ok()?
        .trim()
        .parse()
        .ok()
}
impl Start {
    pub(crate) fn new() -> Self {
        Self {
            instant: Instant::now(),
            rx: rx(),
        }
    }
    pub(crate) fn elapsed(&self) -> Duration {
        self.instant.elapsed()
    }
    pub(crate) fn log_rx(&self, name: &str) {
        if std::env::var_os("OXBLEND_PHASES").is_some() {
            match (self.rx, rx()) {
                (Some(before), Some(after)) if after >= before => {
                    eprintln!("OXBLEND_RX {name} {before} {after} {}", after - before)
                }
                _ => eprintln!("OXBLEND_RX {name} unavailable"),
            }
        }
    }
}
