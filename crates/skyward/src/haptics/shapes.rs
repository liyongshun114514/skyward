//! The shape of the clicks and ticks, kept apart from the force-feedback
//! plumbing and with nothing of Android in it, so that a computer can make
//! and measure them: `crates/skyward/tests/plk110_haptics.rs` does.
//!
//! A made click drives the motor at its resonant frequency for swings of it,
//! then goes on with the same swing turned over: the motor is pushed one way
//! and then the other, which stops it instead of letting it ring on. The
//! driver plays the samples as they are, so the samples are the click.

use anyhow::{anyhow, Context, Result};
use skyward_config::Strength;
use std::f32::consts::TAU;
use std::fs;
use std::path::Path;
use std::time::Duration;

/// The rate the samples are played at, in samples a second. The
/// qcom-hv-haptics driver takes 8000, 16000, 24000, 32000, 44100 and 48000;
/// this is the lowest of them, where a click at the PLK110's 130 Hz still
/// has sixty-one samples to every swing of the motor.
pub const RATE: u32 = 8000;

/// The swings a click is made of at the ends of the slider. One swing at
/// 130 Hz is 7.7 ms, so the light click is 11.5 ms and the heavy one 17.3:
/// the phone's own click effects on the PLK110 run 12.0 to 16.3 ms, and a
/// click much longer than that stops being a tap and becomes a buzz.
pub const LIGHT_SWINGS: f32 = 1.5;
pub const HEAVY_SWINGS: f32 = 2.25;

/// Of a click's swings, this share drives the motor; the rest, turned over,
/// brakes it.
pub const DRIVE: f32 = 0.6;

/// A tick of the screen going off: one short click, played twice, the
/// second lighter and 55 ms after the first.
pub const TICK_SWINGS: f32 = 1.75;
pub const TICK_GAP: Duration = Duration::from_millis(55);
pub const TICK_SECOND: f32 = 0.79;

/// The ticks play at this share of the drive the driver gives samples at in
/// full, whatever the slider says: a tick says that the screen went off, and
/// is quieter than a press.
pub const TICKS_STRENGTH: f32 = 0.415;

/// The share of a click's full strength at the slider's `position`. The
/// slider's steps are each the same factor stronger, as the configuration
/// says, rather than each the same amount.
pub fn share(strength: &Strength, position: f32) -> f32 {
    strength.lightest * (strength.strongest / strength.lightest).powf(position.clamp(0.0, 1.0))
}

/// The swings a click at the slider's `position` is made of.
pub fn swings(position: f32) -> f32 {
    LIGHT_SWINGS + (HEAVY_SWINGS - LIGHT_SWINGS) * position.clamp(0.0, 1.0)
}

/// A click's samples at [`RATE`] for a motor that swings at `hz`, its length
/// following the slider.
pub fn click(hz: f32, position: f32) -> Vec<i8> {
    made(hz, swings(position))
}

/// A tick's samples at [`RATE`] for a motor that swings at `hz`.
pub fn tick(hz: f32) -> Vec<i8> {
    made(hz, TICK_SWINGS)
}

/// Swings of `hz` at [`RATE`], the first [`DRIVE`] of them driving the motor
/// and the rest turned over.
fn made(hz: f32, swings: f32) -> Vec<i8> {
    let at = |swings: f32| (swings * RATE as f32 / hz).round() as usize;
    let drive = at(swings * DRIVE);
    let all = at(swings);
    (0..all)
        .map(|i| {
            let wave = (TAU * hz * i as f32 / RATE as f32).sin() * f32::from(i8::MAX);
            (if i < drive { wave } else { -wave }).round() as i8
        })
        .collect()
}

/// The motor's resonant frequency in hertz, from the file the driver says it
/// in, in tenths of a hertz unless `in_hertz`, and from the period beside it
/// where that file says 0 or is not there.
///
/// The PLK110's driver writes its frequency from the closed-loop calibration,
/// which the phone's own vibrator service runs; a recovery runs none of it, so
/// the file may stay 0. The same driver then writes the calibrated period in
/// `lra_cal_cl_t_lra_us` and the open-loop one its device tree gives in
/// `t_lra_us`, and either is a frequency once a second is divided by it. So
/// the period is tried at once, before waiting on a calibration that will not
/// come, and the file is still asked again a moment apart for a driver that
/// measures its motor a little after its device appears.
pub fn resonance(file: &str, in_hertz: bool) -> Result<f32> {
    let mut why = None;
    for attempt in 0..15 {
        match frequency(file, in_hertz) {
            Ok(hz) if hz > 0.0 => return Ok(hz),
            Ok(_) => why = Some(anyhow!("{file} says the motor swings at 0 Hz")),
            Err(e) => why = Some(e),
        }
        if let Some(hz) = calibration(file) {
            return Ok(hz);
        }
        // A file that is not there will not appear by being asked again; the
        // period beside it has already answered, or there is nothing to read.
        if attempt == 0 && !Path::new(file).exists() {
            break;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    Err(why.unwrap_or_else(|| anyhow!("{file} says the motor swings at 0 Hz")))
}

/// The frequency a driver's file gives, in hertz.
fn frequency(file: &str, in_hertz: bool) -> Result<f32> {
    let text = fs::read_to_string(file).with_context(|| format!("read {file}"))?;
    let said: f32 = text.split_whitespace().next().and_then(|n| n.parse().ok()).with_context(|| format!("{file} holds {text:?}"))?;
    if !said.is_finite() {
        return Err(anyhow!("{file} holds {text:?}"));
    }
    Ok(if in_hertz { said } else { said / 10.0 })
}

/// The motor's period in the files beside the one that gives its frequency,
/// as a frequency: the calibrated period the driver measured, then the
/// open-loop one its device tree names. Both are microseconds a swing.
fn calibration(file: &str) -> Option<f32> {
    let dir = Path::new(file).parent()?;
    for name in ["lra_cal_cl_t_lra_us", "t_lra_us"] {
        let Ok(text) = fs::read_to_string(dir.join(name)) else { continue };
        let Some(us) = text.split_whitespace().next().and_then(|n| n.parse::<u32>().ok()) else { continue };
        if us > 0 {
            return Some(1_000_000.0 / us as f32);
        }
    }
    None
}
