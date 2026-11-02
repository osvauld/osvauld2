//! Rate-limiting a stranger's code guesses, as pure state the host ticks with its own clock:
//! 300 ms before answering a wrong code, three wrong codes close the connection, thirty a
//! minute across all connections and the host answers nothing until the window moves on.
//! The numbers are Tinline's (`docs/group-core.md` § invite links); a 12-byte code behind
//! them is not guessable in a lifetime.

use std::collections::{HashMap, VecDeque};
use std::time::Duration;

pub const WRONG_DELAY: Duration = Duration::from_millis(300);
pub const PER_CONNECTION: u32 = 3;
pub const PER_MINUTE: usize = 30;
const MINUTE_MS: u64 = 60_000;

#[derive(Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Answer `Refused` after this pause.
    Delay(Duration),
    /// The third wrong code on this connection: close it, no answer.
    Close,
}

#[derive(Default)]
pub struct Codes {
    wrong_by_conn: HashMap<u64, u32>,
    wrong_times_ms: VecDeque<u64>,
}

impl Codes {
    /// Whether a code on `conn` may be checked at all right now. `false` means the host has
    /// seen [`PER_MINUTE`] wrong codes in the last minute: refuse without looking.
    pub fn allowed(&mut self, now_ms: u64) -> bool {
        self.trim(now_ms);
        self.wrong_times_ms.len() < PER_MINUTE
    }

    /// Records a wrong code on `conn`.
    pub fn wrong(&mut self, conn: u64, now_ms: u64) -> Verdict {
        self.trim(now_ms);
        self.wrong_times_ms.push_back(now_ms);
        let n = self.wrong_by_conn.entry(conn).or_default();
        *n += 1;
        if *n >= PER_CONNECTION {
            self.wrong_by_conn.remove(&conn);
            Verdict::Close
        } else {
            Verdict::Delay(WRONG_DELAY)
        }
    }

    /// The connection went away: its count goes with it (the per-minute window stays).
    pub fn forget(&mut self, conn: u64) {
        self.wrong_by_conn.remove(&conn);
    }

    fn trim(&mut self, now_ms: u64) {
        while let Some(&t) = self.wrong_times_ms.front() {
            if now_ms.saturating_sub(t) >= MINUTE_MS {
                self.wrong_times_ms.pop_front();
            } else {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests;
