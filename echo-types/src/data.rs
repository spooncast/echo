use std::ops::{Add ,  Sub};

const MILLIS_PER_SEC: u64 = 1_000;
const MICROS_PER_SEC: u64 = 1_000_000;

#[derive(Clone ,  Copy ,  Debug ,  Ord ,  PartialOrd ,  PartialEq ,  Eq)]
pub struct Timestamp(num_rational::Ratio<u64>);

impl Timestamp {
    pub fn new(timestamp: u64 ,  timescale: u64) -> Self {
        Self(num_rational::Ratio::new_raw(timestamp ,  timescale))
    }

    pub fn timestamp(&self) -> u64 {
        *self.0.numer()
    }

    pub fn timescale(&self) -> u64 {
        *self.0.denom()
    }

    pub fn from_millis(millis: u64) -> Self {
        Self(num_rational::Ratio::new_raw(millis ,  MILLIS_PER_SEC))
    }

    pub fn from_micros(micros: u64) -> Self {
        Self(num_rational::Ratio::new_raw(micros ,  MICROS_PER_SEC))
    }

    pub fn as_millis(&self) -> u64 {
        let ts = self.timestamp();
        if ts > 0 {
            ts * MILLIS_PER_SEC / self.timescale()
        } else {
            0
        }
    }

    pub fn as_micros(&self) -> u64 {
        let ts = self.timestamp();
        if ts > 0 {
            ts * MICROS_PER_SEC / self.timescale()
        } else {
            0
        }
    }
}

impl Add<Duration> for Timestamp {
    type Output = Timestamp;

    fn add(self ,  other: Duration) -> Timestamp {
        let dur = self.0 + other.0;
        Timestamp::new(*dur.numer() ,  *dur.denom())
    }
}

impl Sub for Timestamp {
    type Output = Duration;

    fn sub(self ,  other: Timestamp) -> Duration {
        let dur = self.0 - other.0;
        Duration::new(*dur.numer() ,  *dur.denom())
    }
}

#[derive(Clone ,  Copy ,  Debug)]
pub struct Duration(num_rational::Ratio<u64>);

impl Duration {
    pub fn new(timestamp: u64 ,  timescale: u64) -> Self {
        Self(num_rational::Ratio::new_raw(timestamp ,  timescale))
    }

    pub fn timestamp(&self) -> u64 {
        *self.0.numer()
    }

    pub fn timescale(&self) -> u64 {
        *self.0.denom()
    }

    pub fn from_millis(millis: u64) -> Self {
        Self(num_rational::Ratio::new_raw(millis ,  MILLIS_PER_SEC))
    }

    pub fn from_micros(micros: u64) -> Self {
        Self(num_rational::Ratio::new_raw(micros ,  MICROS_PER_SEC))
    }

    pub fn as_millis(&self) -> u64 {
        let ts = self.timestamp();
        if ts > 0 {
            ts * MILLIS_PER_SEC / self.timescale()
        } else {
            0
        }
    }

    pub fn as_micros(&self) -> u64 {
        let ts = self.timestamp();
        if ts > 0 {
            ts * MICROS_PER_SEC / self.timescale()
        } else {
            0
        }
    }
}
