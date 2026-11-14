//! The functions the USB port offers, which init sets up as
//! `sys.usb.config` asks: adb while the recovery waits, with files over MTP
//! beside it while file transfer is on.

use crate::{prop, set_prop};
use anyhow::{bail, Context, Result};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

pub mod fastboot;

static MTP: AtomicBool = AtomicBool::new(false);

/// What init reads to set the port up, and what it writes once it has: a
/// request is in place when the second says what the first asked for.
const CONFIG: &str = "sys.usb.config";
const STATE: &str = "sys.usb.state";

/// OnePlus's switch that lets init hand the port over at all.
const DISABLE: &str = "sys.usb.config_disable";

/// How long between two readings of a property whose change is waited for.
const POLL: Duration = Duration::from_millis(20);

/// The programs init stops when the port goes to "none". Each holds a
/// function of the port until it has ended, and init's `stop` returns before
/// that: `init.svc.<name>` reads "stopped" only once init has reaped it.
/// fastbootd is the one that matters most here: it serves the fastboot
/// function, and may still be writing the partitions the recovery mounts
/// once the port is back at rest.
const NONE_STOPS: [&str; 2] = ["adbd", "fastbootd"];

/// What the port says of itself, read and written through this, so the
/// waiting for init can be tried without a phone.
pub trait Props {
    fn get(&self, name: &str) -> Option<String>;
    fn set(&self, name: &str, value: &str) -> Result<()>;
}

/// The phone's own property area.
pub struct System;

impl Props for System {
    fn get(&self, name: &str) -> Option<String> {
        prop(name)
    }

    fn set(&self, name: &str, value: &str) -> Result<()> {
        set_prop(name, value)
    }
}

/// What the port offers while nothing else has it.
pub fn resting() -> &'static str {
    if MTP.load(Ordering::Relaxed) {
        "mtp,adb"
    } else {
        "adb"
    }
}

/// Whether MTP goes beside adb while the port rests: from when its function
/// has its descriptors until it goes.
pub fn set_mtp(on: bool) {
    MTP.store(on, Ordering::Relaxed);
}

/// Ask init for `config` and wait until it is in place. Going to "none"
/// also waits for the programs that held the port to be gone, so their
/// functions are free for what comes next.
pub fn set(config: &str, timeout: Duration) -> Result<()> {
    set_over(&System, config, timeout)
}

/// Hand the port to `config` through "none", which stops the programs
/// serving it and takes every function off; init then sets the port up
/// once the programs behind the new configuration are ready.
pub fn switch(config: &str, timeout: Duration) -> Result<()> {
    switch_over(&System, config, timeout)
}

fn switch_over(props: &dyn Props, config: &str, timeout: Duration) -> Result<()> {
    set_over(props, "none", timeout)?;
    set_over(props, config, timeout)
}

fn set_over(props: &dyn Props, config: &str, timeout: Duration) -> Result<()> {
    let asked = Instant::now();
    let (said, in_place) = (props.get(CONFIG), props.get(STATE));
    // A request the port already answers asks nothing of init: the property
    // it would write is the one already there, and init acts on a change
    // only. fastbootd can have ended under a port that still says fastboot,
    // which the serving check tells of.
    if said.as_deref() == Some(config) && in_place.as_deref() == Some(config) {
        if config == "none" {
            wait_free(props, left(asked, timeout))?;
        }
        if config == "fastboot" {
            fastboot::serving(props)?;
        }
        return Ok(());
    }
    // OnePlus's init hands the port over only once this switch lets it, and
    // nothing sets it in a recovery: a switch left over from the system
    // keeps every later request off the port. Only the file transfer rules
    // are gated on it, so only there is a refusal the request's own failure.
    if let Err(e) = props.set(DISABLE, "0")
        && config.contains("mtp")
    {
        return Err(e).with_context(|| format!("init refused {DISABLE}=0, which the rules for {config} are gated on"));
    }
    // init acts on a change of the value only: a request naming what the
    // port already has starts nothing, and would wait out its timeout with
    // the port wherever the last request left it. Take the port down first,
    // which is a change, and ask again.
    if config != "none" && said.as_deref() == Some(config) {
        set_over(props, "none", left(asked, timeout))?;
    }
    props.set(CONFIG, config).with_context(|| format!("init refused {CONFIG}={config}"))?;
    if let Err(e) = wait(props, STATE, config, left(asked, timeout)) {
        return Err(match config {
            "fastboot" => e.context(fastboot::because(props)),
            _ => e,
        });
    }
    if config == "none" {
        wait_free(props, left(asked, timeout))?;
    }
    if config == "fastboot" {
        fastboot::serving(props)?;
    }
    Ok(())
}

/// What is left of `timeout`, counted from `asked`, for the steps that
/// follow one another in one request.
fn left(asked: Instant, timeout: Duration) -> Duration {
    timeout.saturating_sub(asked.elapsed())
}

/// Wait until property `name` reads `value`, for at most `timeout`.
fn wait(props: &dyn Props, name: &str, value: &str, timeout: Duration) -> Result<()> {
    let until = Instant::now() + timeout;
    loop {
        let now = props.get(name);
        if now.as_deref() == Some(value) {
            return Ok(());
        }
        if Instant::now() >= until {
            bail!("{name} is {now:?} after {} ms, not {value:?}", timeout.as_millis());
        }
        thread::sleep(POLL);
    }
}

/// Wait until every program that held a function of the port has ended, so
/// the function it held is free and nothing it was writing is under way.
fn wait_free(props: &dyn Props, timeout: Duration) -> Result<()> {
    let until = Instant::now() + timeout;
    for service in NONE_STOPS {
        let name = format!("init.svc.{service}");
        loop {
            let now = props.get(&name);
            if matches!(now.as_deref(), None | Some("stopped")) {
                break;
            }
            if Instant::now() >= until {
                bail!("{service} still holds the port {} ms after it went to none: {name} is {now:?}", timeout.as_millis());
            }
            thread::sleep(POLL);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap;

    const T: Duration = Duration::from_secs(2);

    /// Stock recovery's init, as far as a request on the port can tell:
    /// `sys.usb.config` acts on a change of the value only, "none" stops the
    /// programs that hold a function, and a program init has not reaped yet
    /// cannot be started again.
    struct Init {
        props: RefCell<HashMap<String, String>>,
        /// Readings left before init has reaped a program it stopped.
        reaping: RefCell<HashMap<String, u32>>,
        writes: RefCell<Vec<(String, String)>>,
        disable_refused: bool,
        /// fastbootd starts but never opens its function.
        fastbootd_breaks: bool,
    }

    impl Init {
        fn at(props: &[(&str, &str)]) -> Init {
            Init {
                props: RefCell::new(props.iter().map(|(k, v)| ((*k).to_string(), (*v).to_string())).collect()),
                reaping: RefCell::new(HashMap::new()),
                writes: RefCell::new(Vec::new()),
                disable_refused: false,
                fastbootd_breaks: false,
            }
        }

        fn in_adb() -> Init {
            Init::at(&[(CONFIG, "adb"), (STATE, "adb"), ("sys.usb.ffs.ready", "1"), ("init.svc.adbd", "running")])
        }

        fn in_fastboot() -> Init {
            Init::at(&[
                (CONFIG, "fastboot"),
                (STATE, "fastboot"),
                ("sys.usb.ffs.ready", "1"),
                (fastboot::SERVICE_PROP, "running"),
                (fastboot::CONTROLLER, "a600000.dwc3"),
            ])
        }

        fn said(&self, name: &str) -> Option<String> {
            self.props.borrow().get(name).cloned()
        }

        fn asked(&self, name: &str, value: &str) -> bool {
            self.writes.borrow().iter().any(|(n, v)| n == name && v == value)
        }

        /// What init's recovery rules do when `config` is written.
        fn act(&self, config: &str) {
            let mut props = self.props.borrow_mut();
            match config {
                "none" => {
                    props.insert(STATE.to_string(), "none".to_string());
                    props.insert("sys.usb.ffs.ready".to_string(), "0".to_string());
                    for service in NONE_STOPS {
                        let name = format!("init.svc.{service}");
                        let running = props.get(&name).map(String::as_str) == Some("running");
                        if running {
                            props.insert(name, "stopping".to_string());
                            self.reaping.borrow_mut().insert(service.to_string(), 2);
                        }
                    }
                }
                "adb" => {
                    if props.get("init.svc.adbd").map(String::as_str) == Some("stopping") {
                        return; // init cannot start what it has not reaped
                    }
                    props.insert("init.svc.adbd".to_string(), "running".to_string());
                    props.insert("sys.usb.ffs.ready".to_string(), "1".to_string());
                    props.insert(STATE.to_string(), "adb".to_string());
                }
                "fastboot" => {
                    if props.get(fastboot::SERVICE_PROP).map(String::as_str) == Some("stopping") {
                        return; // init cannot start what it has not reaped
                    }
                    props.insert(fastboot::SERVICE_PROP.to_string(), "running".to_string());
                    if self.fastbootd_breaks {
                        return; // it never opens ep0 or says it is ready
                    }
                    props.insert("sys.usb.ffs.ready".to_string(), "1".to_string());
                    props.insert(STATE.to_string(), "fastboot".to_string());
                }
                "mtp,adb" => {
                    if props.get(DISABLE).map(String::as_str) != Some("0") {
                        return;
                    }
                    props.insert(STATE.to_string(), "mtp,adb".to_string());
                }
                _ => {}
            }
        }

        /// A program init stopped is reaped after a few readings of its
        /// property, as init does here and there.
        fn reap(&self, name: &str) {
            let Some(service) = name.strip_prefix("init.svc.") else { return };
            let ended = {
                let mut reaping = self.reaping.borrow_mut();
                let mut ended = false;
                if let Some(left) = reaping.get_mut(service) {
                    *left = left.saturating_sub(1);
                    ended = *left == 0;
                }
                if ended {
                    reaping.remove(service);
                }
                ended
            };
            if ended {
                self.props.borrow_mut().insert(name.to_string(), "stopped".to_string());
            }
        }
    }

    impl Props for Init {
        fn get(&self, name: &str) -> Option<String> {
            self.reap(name);
            self.said(name)
        }

        fn set(&self, name: &str, value: &str) -> Result<()> {
            self.writes.borrow_mut().push((name.to_string(), value.to_string()));
            if name == DISABLE && self.disable_refused {
                bail!("init refused {name}={value}");
            }
            if name == CONFIG {
                if self.said(name).as_deref() == Some(value) {
                    return Ok(()); // no change, so no rule runs
                }
                self.props.borrow_mut().insert(name.to_string(), value.to_string());
                self.act(value);
                return Ok(());
            }
            self.props.borrow_mut().insert(name.to_string(), value.to_string());
            Ok(())
        }
    }

    #[test]
    fn the_port_is_not_free_while_fastbootd_still_ends() {
        let init = Init::in_fastboot();
        set_over(&init, "none", T).unwrap();
        assert_eq!(init.said(fastboot::SERVICE_PROP).as_deref(), Some("stopped"), "nothing mounts partitions while fastbootd may still write them");
        assert!(matches!(init.said("init.svc.adbd").as_deref(), None | Some("stopped")));
    }

    #[test]
    fn a_port_already_down_still_waits_for_the_previous_service_to_end() {
        let init = Init::in_fastboot();
        init.props.borrow_mut().insert(CONFIG.to_string(), "none".to_string());
        init.act("none");
        set_over(&init, "none", T).unwrap();
        assert_eq!(init.said(fastboot::SERVICE_PROP).as_deref(), Some("stopped"));
        assert!(!init.writes.borrow().iter().any(|(name, _)| name == CONFIG));
    }

    #[test]
    fn a_switch_back_to_fastboot_is_not_dropped_while_fastbootd_ends() {
        let init = Init::in_fastboot();
        switch_over(&init, "fastboot", T).unwrap();
        assert_eq!(init.said(STATE).as_deref(), Some("fastboot"));
        assert_eq!(init.said(fastboot::SERVICE_PROP).as_deref(), Some("running"));
    }

    #[test]
    fn a_request_the_port_already_answers_asks_init_for_nothing() {
        let init = Init::in_adb();
        set_over(&init, "adb", T).unwrap();
        assert!(!init.writes.borrow().iter().any(|(name, _)| name == CONFIG), "the value init has is not written again: {:?}", init.writes.borrow());
    }

    #[test]
    fn a_request_init_already_has_is_asked_again_through_none() {
        let init = Init::in_adb();
        // A switch that failed after the value was written leaves the port
        // saying adb while nothing serves it.
        init.props.borrow_mut().insert(STATE.to_string(), "none".to_string());
        set_over(&init, "adb", T).unwrap();
        assert_eq!(init.said(STATE).as_deref(), Some("adb"));
        assert!(init.asked(CONFIG, "none"), "the port went down first: {:?}", init.writes.borrow());
    }

    #[test]
    fn a_fastbootd_that_never_opens_its_function_is_named() {
        let mut init = Init::in_adb();
        init.fastbootd_breaks = true;
        let why = format!("{:#}", switch_over(&init, "fastboot", Duration::from_millis(200)).unwrap_err());
        assert!(why.contains("fastbootd"), "{why}");
        assert!(why.contains("sys.usb.ffs.ready"), "{why}");
    }

    #[test]
    fn file_transfer_says_why_init_left_the_port_alone() {
        let mut init = Init::in_adb();
        init.disable_refused = true;
        let why = format!("{:#}", set_over(&init, "mtp,adb", T).unwrap_err());
        assert!(why.contains(DISABLE), "{why}");
    }

    #[test]
    fn adb_is_asked_for_even_where_file_transfers_switch_is_refused() {
        let mut init = Init::in_adb();
        init.disable_refused = true;
        switch_over(&init, "adb", T).unwrap();
        assert_eq!(init.said(STATE).as_deref(), Some("adb"));
    }

    #[test]
    fn a_port_that_stays_down_says_what_it_says() {
        let init = Init::at(&[(CONFIG, "adb"), (STATE, "adb")]);
        let why = format!("{:#}", set_over(&init, "sideload", Duration::from_millis(100)).unwrap_err());
        assert!(why.contains("sys.usb.state") && why.contains("sideload"), "{why}");
    }
}
