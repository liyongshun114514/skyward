//! The clicks and ticks the PLK110's linear motor is played, made and
//! measured on a computer: the shapes live in `src/haptics/shapes.rs`, which
//! needs nothing of Android, and this walks them at the 130 Hz the phone's
//! driver reports.
//!
//! The lengths are the phone's own: its click effects in
//! `/odm/etc/vibrator/816/def` run 11.96 ms (effect_1), 14.50 ms (effect_2)
//! and 16.33 ms (effect_0) at 24000 samples a second.

#[path = "../src/haptics/shapes.rs"]
mod shapes;

use skyward_config::Strength;
use std::f32::consts::TAU;
use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

const HZ: f32 = 130.0;

fn ms(samples: usize) -> f32 {
    samples as f32 * 1000.0 / shapes::RATE as f32
}

fn write(dir: &Path, name: &str, text: &str) {
    fs::write(dir.join(name), text).unwrap();
}

#[test]
fn a_click_is_short_enough_to_be_a_tap_on_this_motor() {
    let light = shapes::click(HZ, 0.0);
    let heavy = shapes::click(HZ, 1.0);
    assert!((11.0..13.0).contains(&ms(light.len())), "the light click is {:.2} ms", ms(light.len()));
    assert!((16.0..18.0).contains(&ms(heavy.len())), "the heavy click is {:.2} ms", ms(heavy.len()));
    // The phone's own clicks run 11.96 to 16.33 ms; a click of three and a
    // half swings at 130 Hz would be 26.9 ms, long enough to feel as a buzz.
    assert!(ms(heavy.len()) < 20.0);
}

#[test]
fn the_slider_lengthens_the_click_without_changing_its_frequency() {
    let lengths: Vec<usize> = [0.0f32, 0.25, 0.5, 0.75, 1.0].iter().map(|&p| shapes::click(HZ, p).len()).collect();
    assert!(lengths.windows(2).all(|w| w[0] <= w[1]), "the click only grows: {lengths:?}");
    assert!(lengths[0] < lengths[4]);
    let per_swing = lengths[0] as f32 / shapes::LIGHT_SWINGS;
    assert!((per_swing - shapes::RATE as f32 / HZ).abs() < 1.0, "{} samples to a swing at {HZ} Hz", per_swing);
    let per_swing = lengths[4] as f32 / shapes::HEAVY_SWINGS;
    assert!((per_swing - shapes::RATE as f32 / HZ).abs() < 1.0, "{} samples to a swing at {HZ} Hz", per_swing);
}

#[test]
fn a_click_drives_the_motor_then_turns_the_same_swing_over() {
    let samples = shapes::click(HZ, 1.0);
    let swings = shapes::swings(1.0);
    let drive = (swings * shapes::DRIVE * shapes::RATE as f32 / HZ).round() as usize;
    let sine = |i: usize| (TAU * HZ * i as f32 / shapes::RATE as f32).sin();
    assert_eq!(samples[0], 0, "it starts at rest");
    assert_eq!(*samples.iter().max().unwrap(), i8::MAX, "it drives to the full");
    assert_eq!(*samples.iter().min().unwrap(), -i8::MAX, "and turns over to the full the other way");
    assert!((f32::from(samples[drive]) * sine(drive)) <= 0.0, "the samples after the drive are the swing turned over");
    let driven: f32 = samples[..drive].iter().map(|&s| f32::from(s)).sum();
    let braked: f32 = samples[drive..].iter().map(|&s| f32::from(s)).sum();
    assert!(driven.abs() > 0.0 && braked.abs() > 0.0);
}

#[test]
fn a_tick_is_one_short_click_and_the_two_are_fifty_five_milliseconds_apart() {
    let tick = shapes::tick(HZ);
    assert!((13.0..14.0).contains(&ms(tick.len())), "a tick is {:.2} ms", ms(tick.len()));
    assert_eq!(shapes::TICK_GAP, Duration::from_millis(55));
    assert!((shapes::TICK_SECOND - 0.79).abs() < f32::EPSILON);
    const { assert!(shapes::TICKS_STRENGTH > 0.0 && shapes::TICKS_STRENGTH < 1.0, "a tick is not the full drive") };
}

#[test]
fn a_faster_motor_ticks_in_proportionally_fewer_samples() {
    assert_eq!(shapes::tick(130.0).len(), shapes::tick(260.0).len() * 2);
}

#[test]
fn the_driver_says_zero_so_the_calibrated_period_beside_it_is_used_at_once() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "lra_frequency_hz", "0\n");
    write(dir.path(), "lra_cal_cl_t_lra_us", "7640\n");
    let file = dir.path().join("lra_frequency_hz");
    let started = Instant::now();
    let hz = shapes::resonance(file.to_str().unwrap(), true).unwrap();
    assert!((hz - 130.9).abs() < 0.1, "{hz} Hz");
    assert!(started.elapsed() < Duration::from_millis(500), "no wait for a calibration a recovery never runs");
}

#[test]
fn a_frequency_file_that_is_not_there_leaves_the_open_loop_period() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "t_lra_us", "7692\n");
    let file = dir.path().join("lra_frequency_hz");
    let started = Instant::now();
    let hz = shapes::resonance(file.to_str().unwrap(), true).unwrap();
    assert!((hz - 130.0).abs() < 0.1, "{hz} Hz");
    assert!(started.elapsed() < Duration::from_millis(500));
}

#[test]
fn a_driver_that_says_the_frequency_itself_and_one_that_says_tenths_are_both_read() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("lra_frequency_hz");
    write(dir.path(), "lra_frequency_hz", "130\n");
    assert_eq!(shapes::resonance(file.to_str().unwrap(), true).unwrap(), 130.0);
    write(dir.path(), "lra_frequency_hz", "1724\n");
    assert!((shapes::resonance(file.to_str().unwrap(), false).unwrap() - 172.4).abs() < 1e-4);
}

#[test]
fn a_motor_that_says_nothing_at_all_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("lra_frequency_hz");
    let started = Instant::now();
    assert!(shapes::resonance(file.to_str().unwrap(), true).is_err());
    assert!(started.elapsed() < Duration::from_millis(500), "a missing file is not waited on");
}

#[test]
fn the_slider_spans_the_strengths_each_step_the_same_factor() {
    let s = Strength { lightest: 0.25, strongest: 1.0 };
    assert_eq!((shapes::share(&s, 0.0), shapes::share(&s, 1.0)), (0.25, 1.0));
    assert!((shapes::share(&s, 0.5) - 0.5).abs() < 1e-6);
    assert!((shapes::share(&s, 0.25) / shapes::share(&s, 0.0) - shapes::share(&s, 1.0) / shapes::share(&s, 0.75)).abs() < 1e-6);
    assert_eq!((shapes::share(&s, -1.0), shapes::share(&s, 2.0)), (0.25, 1.0), "no further than its ends");
}
