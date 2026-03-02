//! Stable duration type for protocol wire format.
//!
//! [`DurationMs`] serializes as a plain integer (milliseconds), not as
//! serde's internal `{"secs": N, "nanos": N}` format. This gives a
//! stable, portable, human-readable wire format that will not break
//! if serde changes its internal Duration representation.

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Duration in milliseconds with a stable JSON serialization format.
///
/// Serializes as a plain `u64` integer representing milliseconds.
/// This is the canonical wire format for all durations in the protocol.
///
/// # Examples
///
/// ```
/// use layer0::DurationMs;
///
/// let d = DurationMs::from_millis(1500);
/// assert_eq!(d.as_millis(), 1500);
///
/// let json = serde_json::to_string(&d).unwrap();
/// assert_eq!(json, "1500");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DurationMs(u64);

impl DurationMs {
    /// Zero duration.
    pub const ZERO: Self = Self(0);

    /// Create from milliseconds.
    pub fn from_millis(ms: u64) -> Self {
        Self(ms)
    }

    /// Create from seconds.
    pub fn from_secs(secs: u64) -> Self {
        Self(secs.saturating_mul(1000))
    }

    /// Get the value in milliseconds.
    pub fn as_millis(&self) -> u64 {
        self.0
    }

    /// Convert to `std::time::Duration`.
    pub fn to_std(&self) -> Duration {
        Duration::from_millis(self.0)
    }
}

impl From<Duration> for DurationMs {
    fn from(d: Duration) -> Self {
        Self(d.as_millis() as u64)
    }
}

impl From<DurationMs> for Duration {
    fn from(d: DurationMs) -> Self {
        Duration::from_millis(d.0)
    }
}

impl Default for DurationMs {
    fn default() -> Self {
        Self::ZERO
    }
}

impl std::fmt::Display for DurationMs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}ms", self.0)
    }
}
