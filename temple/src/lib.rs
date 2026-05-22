//! PRECRIME temple ball.
//!
//! Wire format and UDP multicast send/receive helpers shared between PRECOG
//! (sender) and REPORT (receiver). The ball advertises one PRECOG source
//! per JSON message, sent every `BALL_PERIOD_SECS` on the temple channel.

pub mod ball;
pub mod recv;
pub mod send;

pub use ball::{Ball, BallV1, HwStats, RtpInfo, ThermalState, VideoInfo, WitnessStats, WitnessStatsPacket};
pub use recv::Receiver;
pub use send::Sender;

pub const DEFAULT_TEMPLE_GROUP: &str = "239.42.0.1";
pub const DEFAULT_TEMPLE_PORT: u16 = 9999;
pub const BALL_PERIOD_SECS: u64 = 2;
pub const BALL_EVICTION_SECS: u64 = 6;
