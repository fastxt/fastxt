/*
    Fastxt
    Copyright (C) 2020  Yi Wang

    This program is free software: you can redistribute it and/or modify
    it under the terms of the GNU Affero General Public License as published by
    the Free Software Foundation, either version 3 of the License, or
    (at your option) any later version.

    This program is distributed in the hope that it will be useful,
    but WITHOUT ANY WARRANTY; without even the implied warranty of
    MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
    GNU Affero General Public License for more details.

    You should have received a copy of the GNU Affero General Public License
    along with this program.  If not, see <https://www.gnu.org/licenses/>.
*/

//! Hybrid logical clock stamps used to order changes across devices.
//!
//! A stamp is `{millis:013x}-{counter:04x}-{node}`: wall-clock milliseconds,
//! a counter that breaks ties within one millisecond, and the device id. The
//! fixed-width hex fields make plain string comparison match stamp order, so
//! stamps can be compared in SQL and in Rust alike.
//!
//! A device's clock never goes backwards and always moves past every stamp it
//! has seen from other devices, so a local edit made after a sync always wins
//! over the synced version it replaces, even if the wall clock is behind.

use std::time::{SystemTime, UNIX_EPOCH};

/// Node id used for stamps backfilled from `created_at` on pre-0.6 databases.
/// All zeros, so any real device stamp in the same millisecond sorts after it.
pub const LEGACY_NODE: &str = "00000000000000000000000000000000";

/// A per-device hybrid logical clock.
#[derive(Debug, Clone)]
pub struct Clock {
    node: String,
    millis: u64,
    counter: u32,
}

impl Clock {
    /// A clock for device `node` that has seen nothing yet.
    #[must_use]
    pub fn new(node: impl Into<String>) -> Self {
        Clock {
            node: node.into(),
            millis: 0,
            counter: 0,
        }
    }

    /// Advance past `stamp` (a stamp read from the database or a peer).
    /// Empty or malformed stamps are ignored.
    pub fn observe(&mut self, stamp: &str) {
        if let Some((millis, counter)) = parse(stamp)
            && (millis, counter) > (self.millis, self.counter)
        {
            self.millis = millis;
            self.counter = counter;
        }
    }

    /// A new stamp, strictly greater than every stamp issued or observed.
    pub fn tick(&mut self) -> String {
        let now = now_millis();
        if now > self.millis {
            self.millis = now;
            self.counter = 0;
        } else if self.counter == 0xffff {
            self.millis += 1;
            self.counter = 0;
        } else {
            self.counter += 1;
        }
        format_stamp(self.millis, self.counter, &self.node)
    }

    /// A new stamp that is also greater than `prev`.
    pub fn tick_after(&mut self, prev: &str) -> String {
        self.observe(prev);
        self.tick()
    }
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

fn format_stamp(millis: u64, counter: u32, node: &str) -> String {
    format!("{millis:013x}-{counter:04x}-{node}")
}

/// Parse the millisecond and counter fields of a stamp.
#[must_use]
pub fn parse(stamp: &str) -> Option<(u64, u32)> {
    let mut parts = stamp.splitn(3, '-');
    let millis = u64::from_str_radix(parts.next()?, 16).ok()?;
    let counter = u32::from_str_radix(parts.next()?, 16).ok()?;
    parts.next()?;
    Some((millis, counter))
}

/// The stamp a pre-0.6 note gets during migration: its creation time, counter
/// zero, legacy node. Deterministic, so two devices that already share a note
/// compute the same stamp and do not re-transfer it.
#[must_use]
pub fn legacy_stamp(created_at: &str) -> String {
    let millis = chrono::NaiveDateTime::parse_from_str(created_at, "%Y-%m-%d %H:%M:%S")
        .map(|t| t.and_utc().timestamp_millis())
        .ok()
        .and_then(|m| u64::try_from(m).ok())
        .unwrap_or(0);
    format_stamp(millis, 0, LEGACY_NODE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ticks_are_strictly_increasing() {
        let mut c = Clock::new("a");
        let mut prev = c.tick();
        for _ in 0..1000 {
            let next = c.tick();
            assert!(next > prev, "{next} should sort after {prev}");
            prev = next;
        }
    }

    #[test]
    fn observe_moves_past_remote_stamps_from_the_future() {
        let mut c = Clock::new("a");
        let future = format_stamp(now_millis() + 3_600_000, 7, "b");
        c.observe(&future);
        assert!(c.tick() > future);
    }

    #[test]
    fn tick_after_beats_the_previous_stamp_even_with_a_slow_clock() {
        let mut c = Clock::new("a");
        let prev = format_stamp(now_millis() + 60_000, 0xffff, "zzzz");
        assert!(c.tick_after(&prev) > prev);
    }

    #[test]
    fn empty_and_garbage_stamps_are_ignored() {
        let mut c = Clock::new("a");
        c.observe("");
        c.observe("not-a-stamp");
        assert!(parse(&c.tick()).is_some());
    }

    #[test]
    fn legacy_stamp_is_deterministic_and_ordered() {
        let a = legacy_stamp("2020-01-01 00:00:00");
        assert_eq!(a, legacy_stamp("2020-01-01 00:00:00"));
        assert!(legacy_stamp("2020-01-01 00:00:01") > a);
        assert_eq!(parse(&a), Some((1_577_836_800_000, 0)));
        assert!(a.ends_with(LEGACY_NODE));
    }
}
