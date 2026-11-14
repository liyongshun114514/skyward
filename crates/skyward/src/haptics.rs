use anyhow::{anyhow, ensure, Context, Result};
use evdev::raw_stream::RawDevice;
use skyward_config::Strength;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::os::fd::AsRawFd;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[path = "haptics/shapes.rs"]
mod shapes;

// linux/input-event-codes.h
const EV_FF: u16 = 0x15;
const FF_PERIODIC: u16 = 0x51;
const FF_CUSTOM: u16 = 0x5d;

// linux/input.h: _IOW('E', nr, size)
const fn iow(nr: u32, size: u32) -> u32 {
    (1u32 << 30) | (size << 16) | ((b'E' as u32) << 8) | nr
}
const EVIOCSFF: u32 = iow(0x80, std::mem::size_of::<libc::ff_effect>() as u32);
const EVIOCRMFF: u32 = iow(0x81, std::mem::size_of::<libc::c_int>() as u32);
// What the kernel makes of the two on a 64-bit phone. If either stops being
// these, the effect the driver is handed is not the one it reads.
const _: () = assert!(EVIOCSFF == 0x4030_4580);
const _: () = assert!(EVIOCRMFF == 0x4004_4581);

/// The vibration motor, played through the kernel's force-feedback events.
/// The driver fills its queue with a made click's samples as the click is
/// loaded, refilling it for no second play, and keeps the click playing after
/// its samples have run out, even when told to stop, until the click is taken
/// away. So each click is loaded as it is played, at the strength the slider
/// gives, and taken away once it has played out.
///
/// The PLK110's motor is `qcom-hv-haptics`' input device: it takes only a
/// periodic effect whose waveform is [`FF_CUSTOM`], with the samples in the
/// 24-byte `qcom_haptics_custom_fifo_data` the driver copies out of the
/// caller: an index it does not use, the length in bytes, the rate, and a
/// pointer to signed 8-bit samples. The rate is one of 8000, 16000, 24000,
/// 32000, 44100 and 48000, and the magnitude scales the FIFO's drive, so the
/// strength slider is what sets how hard the motor is pushed.
pub struct Haptics {
    dev: File,
    /// What the motor swings at, in hertz.
    hz: f32,
    click: Click,
    strength: Strength,
    /// The click playing, by its slot and its count, until it is taken
    /// away; a click taken away early by the next is not taken away again.
    playing: Arc<Mutex<Option<(i16, u64)>>>,
    clicks: u64,
}

/// What a click is.
enum Click {
    /// A waveform the driver keeps, by its number. The driver says how long
    /// it plays as it loads it.
    Kept(i16),
    /// Samples made at the motor's resonant frequency, which the driver
    /// plays as they are, at `rate` a second.
    Samples { data: Vec<i8>, rate: u32 },
    Made,
}

impl Haptics {
    /// The force-feedback device and the configured click. Loading a click
    /// fills the driver's queue with what it plays, so the click is loaded
    /// only as it is played.
    pub fn open(cfg: &skyward_config::Haptics) -> Result<Haptics> {
        let (path, name, dev) = motor()?;
        let hz = shapes::resonance(&cfg.resonance, cfg.resonance_in_hertz)?;
        ensure!((20.0..=1000.0).contains(&hz), "the motor is said to swing at {hz} Hz, which no motor does");
        let click = match cfg.click {
            Some(id) => {
                eprintln!("haptics {} ({name}): clicks with waveform {id}, ticks made for a motor swinging at {hz:.1} Hz", path.display());
                Click::Kept(id)
            }
            None => {
                eprintln!("haptics {} ({name}): clicks and ticks made for a motor swinging at {hz:.1} Hz", path.display());
                Click::Made
            }
        };
        Ok(Haptics { dev, hz, click, strength: cfg.strength, playing: Arc::new(Mutex::new(None)), clicks: 0 })
    }

    /// One short click at the strength the slider's `position` gives, after
    /// taking away one still playing. Gives how long it plays.
    pub fn click(&mut self, position: f32) -> Result<Duration> {
        let click = match &self.click {
            Click::Kept(id) => Click::Kept(*id),
            _ => Click::Samples { data: shapes::click(self.hz, position), rate: shapes::RATE },
        };
        self.start(click, shapes::share(&self.strength, position))
    }

    /// The ticks of the screen going off: two short clicks at the motor's
    /// own frequency, the second lighter, as strong as the phone's own system
    /// plays them whatever the slider says. Gives how long they take.
    pub fn ticks(&mut self) -> Result<Duration> {
        let first = self.start(Click::Samples { data: shapes::tick(self.hz), rate: shapes::RATE }, shapes::TICKS_STRENGTH)?;
        std::thread::sleep(shapes::TICK_GAP);
        let second = self.start(Click::Samples { data: shapes::tick(self.hz), rate: shapes::RATE }, shapes::TICKS_STRENGTH * shapes::TICK_SECOND)?;
        Ok(first + shapes::TICK_GAP + second)
    }

    /// Play `click` at `share` of its full strength, after taking away what
    /// still plays.
    fn start(&mut self, click: Click, share: f32) -> Result<Duration> {
        let mut playing = self.playing.lock().expect("no thread panics while it holds the click playing");
        if let Some((slot, _)) = playing.take() {
            erase(&self.dev, slot).context("take away the click still playing")?;
        }
        let loaded = upload(&self.dev, &click, share).context("load the click")?;
        play(&mut self.dev, loaded.slot).context("play the click")?;
        self.clicks += 1;
        let this = (loaded.slot, self.clicks);
        *playing = Some(this);
        drop(playing);
        let dev = self.dev.try_clone().context("share the motor")?;
        let length = loaded.length;
        // Timed from now: a thread may wait a while before it first runs.
        let until = Instant::now() + length;
        let playing = self.playing.clone();
        std::thread::spawn(move || {
            std::thread::sleep(until.saturating_duration_since(Instant::now()));
            let mut playing = playing.lock().expect("no thread panics while it holds the click playing");
            if *playing == Some(this) {
                *playing = None;
                if let Err(e) = erase(&dev, this.0) {
                    eprintln!("take away the click: {e}");
                }
            }
        });
        Ok(length)
    }
}

/// Play the configured click once at the slider's `position`, or else the
/// ticks of the screen going off, saying how strongly and for how long, for
/// trying out a motor.
pub fn probe(cfg: &skyward_config::Haptics, position: f32, ticks: bool) -> Result<()> {
    let mut haptics = Haptics::open(cfg)?;
    let (what, strength, length) = if ticks {
        ("ticks", shapes::TICKS_STRENGTH, haptics.ticks()?)
    } else {
        ("click", shapes::share(&cfg.strength, position), haptics.click(position)?)
    };
    // Once it has played out, what played goes with the device as the
    // command ends.
    std::thread::sleep(length);
    println!("played the {what} at {strength:.3} of the full strength, {} ms long", length.as_millis());
    Ok(())
}

/// The input device that takes made clicks, opened for writing. The PLK110
/// has one such device, `qcom-hv-haptics` on event6; a phone may have other
/// force-feedback devices (a uinput one, or a motor of another kind) that
/// take a periodic effect without the driver's custom samples, so the one
/// named for the motor is preferred and the first that takes the samples is
/// the fallback.
fn motor() -> Result<(PathBuf, String, File)> {
    let mut fallback = None;
    for (path, dev) in evdev::raw_stream::enumerate() {
        if !takes_custom(&dev) {
            continue;
        }
        let name = dev.name().unwrap_or("?").to_string();
        let file = match OpenOptions::new().read(true).write(true).open(&path) {
            Ok(file) => file,
            Err(e) => {
                eprintln!("haptics {}: {e}", path.display());
                continue;
            }
        };
        if is_motor_name(&name) {
            return Ok((path, name, file));
        }
        fallback.get_or_insert((path, name, file));
    }
    fallback.ok_or_else(|| anyhow!("no input device takes the driver's custom force-feedback effects"))
}

/// Whether the device takes a periodic effect made of custom samples, which
/// is what both the click and the ticks are. The driver advertises
/// [`FF_PERIODIC`] and [`FF_CUSTOM`] together; one that advertises only the
/// first would refuse the effect this motor is played with.
fn takes_custom(dev: &RawDevice) -> bool {
    dev.supported_ff().is_some_and(|ff| ff.contains(evdev::FFEffectCode::FF_PERIODIC) && ff.contains(evdev::FFEffectCode::FF_CUSTOM))
}

/// Whether a device's name says it is the phone's vibration motor.
fn is_motor_name(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name.contains("haptic") || name.contains("vibrat")
}

/// A click as the driver loaded it: the slot the kernel keeps it in, and
/// how long it plays.
struct Loaded {
    slot: i16,
    length: Duration,
}

/// Samples handed to the driver to play as they are, as its
/// `custom_fifo_data` lays them out. The driver reads the length at 4, the
/// rate at 8 and the samples through the pointer at 16, and takes only a
/// 24-byte structure: any other length it refuses, though the upload still
/// succeeds, and nothing plays.
#[repr(C)]
struct Samples {
    /// Unused by the driver.
    index: u32,
    length: u32,
    rate: u32,
    data: *const i8,
}

const _: () = assert!(std::mem::size_of::<Samples>() == 24);
const _: () = assert!(std::mem::offset_of!(Samples, index) == 0);
const _: () = assert!(std::mem::offset_of!(Samples, length) == 4);
const _: () = assert!(std::mem::offset_of!(Samples, rate) == 8);
const _: () = assert!(std::mem::offset_of!(Samples, data) == 16);

/// Load `click` at `share` of its full strength as a custom periodic effect
/// in a slot of its own. The driver drives the motor at the click's samples,
/// scaled by the magnitude.
fn upload(dev: &File, click: &Click, share: f32) -> Result<Loaded> {
    // The driver tells the two kinds apart by the length in bytes of what
    // the effect points to, and loads nothing for any other length, though
    // the upload still succeeds. For a waveform it keeps it reads the
    // waveform's number from the first of three values and writes how long
    // it plays into the other two, in whole seconds and then milliseconds.
    let mut kept: [i16; 3] = [0; 3];
    let mut samples = Samples { index: 0, length: 0, rate: shapes::RATE, data: std::ptr::null() };
    let (len, data) = match click {
        Click::Made => anyhow::bail!("a made click must have samples before upload"),
        Click::Kept(id) => {
            kept[0] = *id;
            (std::mem::size_of_val(&kept), kept.as_mut_ptr())
        }
        Click::Samples { data, rate } => {
            samples.length = u32::try_from(data.len())?;
            samples.rate = *rate;
            samples.data = data.as_ptr();
            (std::mem::size_of::<Samples>(), (&mut samples as *mut Samples).cast::<i16>())
        }
    };
    let periodic = libc::ff_periodic_effect {
        waveform: FF_CUSTOM,
        period: 0,
        magnitude: (share.clamp(0.0, 1.0) * f32::from(i16::MAX)).round() as i16,
        offset: 0,
        phase: 0,
        envelope: libc::ff_envelope { attack_length: 0, attack_level: 0, fade_length: 0, fade_level: 0 },
        custom_len: len as u32,
        custom_data: data,
    };
    let mut effect = libc::ff_effect {
        type_: FF_PERIODIC,
        id: -1,
        direction: 0,
        trigger: libc::ff_trigger { button: 0, interval: 0 },
        replay: libc::ff_replay { length: 0, delay: 0 },
        u: [0; 4],
    };
    // The union holds the periodic effect; it is exactly as large.
    const _: () = assert!(std::mem::size_of::<libc::ff_periodic_effect>() == std::mem::size_of::<[u64; 4]>());
    effect.u = unsafe { std::mem::transmute::<libc::ff_periodic_effect, [u64; 4]>(periodic) };
    if unsafe { libc::ioctl(dev.as_raw_fd(), EVIOCSFF as _, &mut effect) } < 0 {
        return Err(io::Error::last_os_error().into());
    }
    let length = match click {
        Click::Made => anyhow::bail!("a made click must have samples before upload"),
        Click::Kept(id) => {
            let [_, seconds, millis] = kept;
            let length = Duration::from_secs(u64::try_from(seconds)?) + Duration::from_millis(u64::try_from(millis)?);
            ensure!(!length.is_zero(), "the driver loaded waveform {id} without saying how long it plays");
            length
        }
        // The queue is full before the samples start playing.
        Click::Samples { data, rate } => Duration::from_secs_f64(data.len() as f64 / f64::from(*rate)),
    };
    Ok(Loaded { slot: effect.id, length })
}

/// Start the loaded effect in slot `id`.
fn play(dev: &mut File, id: i16) -> io::Result<()> {
    let event = libc::input_event { time: libc::timeval { tv_sec: 0, tv_usec: 0 }, type_: EV_FF, code: id as u16, value: 1 };
    let bytes = unsafe { std::slice::from_raw_parts((&event as *const libc::input_event).cast::<u8>(), std::mem::size_of::<libc::input_event>()) };
    dev.write_all(bytes)
}

/// Take the effect in slot `id` away, which stops it for good.
fn erase(dev: &File, id: i16) -> io::Result<()> {
    if unsafe { libc::ioctl(dev.as_raw_fd(), EVIOCRMFF as _, libc::c_int::from(id)) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
