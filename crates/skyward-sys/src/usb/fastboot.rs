//! The phone's own fastbootd, which serves a computer's fastboot over the
//! port stock recovery's init makes for it: what has to be in place before
//! `sys.usb.config=fastboot` can come up, and which step is missing when it
//! does not.
//!
//! Stock recovery's `init.rc` takes these steps, in this order, and the
//! recovery reads each of them back:
//!
//! 1. `on init`, with `sys.usb.configfs` on, makes
//!    `/config/usb_gadget/g1/functions/ffs.fastboot`, and `on fs` mounts
//!    functionfs on `/dev/usb-ffs/fastboot`.
//! 2. `on property:sys.usb.config=fastboot` starts `fastbootd` and the
//!    fastboot HAL.
//! 3. fastbootd opens `/dev/usb-ffs/fastboot/ep0` and sets
//!    `sys.usb.ffs.ready` to 1. Nothing else sets it for this function:
//!    adbd sets it for the adb function, and fastbootd for this one.
//! 4. only then does init's rule for `sys.usb.config=fastboot`,
//!    `sys.usb.ffs.ready=1` and `sys.usb.configfs=1` give the gadget the
//!    fastboot function and the controller, and set
//!    `sys.usb.state=fastboot` last of all.
//!
//! A port that never reaches `sys.usb.state=fastboot` is missing one of
//! these, and [`missing`] says which.

use super::{Props, System, CONFIG, STATE};
use anyhow::{bail, Result};
use std::path::Path;

/// The service init starts for the phone's own fastboot, and the property
/// that says whether it runs.
pub const SERVICE: &str = "fastbootd";
pub const SERVICE_PROP: &str = "init.svc.fastbootd";

/// init's rule binds the function only once fastbootd sets this.
pub const READY: &str = "sys.usb.ffs.ready";

/// The controller init names from `ro.boot.usbcontroller`, whose name its
/// rule writes to the gadget's `UDC`.
pub const CONTROLLER: &str = "sys.usb.controller";

/// The function init makes in the gadget, the functionfs fastbootd opens,
/// and the endpoint it serves: the three the recovery can look for.
pub const FUNCTION: &str = "/config/usb_gadget/g1/functions/ffs.fastboot";
pub const MOUNT: &str = "/dev/usb-ffs/fastboot";
pub const EP0: &str = "/dev/usb-ffs/fastboot/ep0";

/// The step of init's rules that has not happened yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// init has not started fastbootd, or it has ended.
    Service,
    /// fastbootd has not opened its function and said so.
    Ready,
    /// init has not bound the function and set the port's state.
    State,
}

/// What init and fastbootd say of the port, as the recovery reads it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Status {
    pub service: Option<String>,
    pub ready: Option<String>,
    pub config: Option<String>,
    pub state: Option<String>,
    pub controller: Option<String>,
}

impl Status {
    pub fn read(props: &dyn Props) -> Status {
        Status {
            service: props.get(SERVICE_PROP),
            ready: props.get(READY),
            config: props.get(CONFIG),
            state: props.get(STATE),
            controller: props.get(CONTROLLER),
        }
    }

    /// Whether fastbootd runs, as init says. A port can say fastboot after
    /// fastbootd has ended, and then nothing answers the computer.
    pub fn running(&self) -> bool {
        self.service.as_deref() == Some("running")
    }
}

/// The first step of init's rules that has not happened, in their order.
pub fn missing(status: &Status) -> Option<Step> {
    if !status.running() {
        Some(Step::Service)
    } else if status.ready.as_deref() != Some("1") {
        Some(Step::Ready)
    } else if status.state.as_deref() != Some("fastboot") {
        Some(Step::State)
    } else {
        None
    }
}

/// Whether fastbootd serves the port the recovery has just asked for. init
/// sets `sys.usb.state` from its own rule, so a port that says fastboot is
/// one fastbootd said it was ready for; one that has ended since leaves the
/// port saying fastboot with nothing behind it.
pub fn serving(props: &dyn Props) -> Result<()> {
    let status = Status::read(props);
    if status.running() {
        return Ok(());
    }
    bail!("{SERVICE_PROP} is {:?}: nothing serves fastboot on the port any more", status.service)
}

/// Why the port did not come up, for the message a failed request leaves.
pub fn because(props: &dyn Props) -> String {
    let status = Status::read(props);
    match missing(&status) {
        Some(Step::Service) => format!(
            "{SERVICE_PROP} is {:?}: init starts {SERVICE} on {CONFIG}=fastboot, and only it opens {EP0} and sets {READY}=1, which init's rule waits for before it binds the function",
            status.service
        ),
        Some(Step::Ready) => format!(
            "{SERVICE} runs but has not opened {EP0}: {READY} is {:?}, and init binds the fastboot function only once it says 1",
            status.ready
        ),
        Some(Step::State) => format!(
            "init has not bound the fastboot function: {CONFIG} is {:?}, {STATE} is {:?}, {CONTROLLER} is {:?}, and the rule that binds it writes the last as the gadget's UDC",
            status.config, status.state, status.controller
        ),
        None => format!("{STATE} is {STATE}=fastboot, so the port is up"),
    }
}

/// Whether the function and its endpoint are where init makes them: read of
/// the files, as a recovery that has them says so.
pub fn gadget_ready_at(root: &Path) -> bool {
    let under = |path: &str| root.join(path.trim_start_matches('/'));
    under(FUNCTION).is_dir() && under(EP0).exists()
}

/// Whether the recovery's own gadget has fastbootd's function.
pub fn gadget_ready() -> bool {
    gadget_ready_at(Path::new("/"))
}

/// One line for the kernel log, of everything the port says of fastboot.
pub fn line() -> String {
    describe(&Status::read(&System), gadget_ready())
}

/// What the port says of fastboot, in one line.
pub fn describe(status: &Status, gadget: bool) -> String {
    let said = |value: &Option<String>| value.clone().unwrap_or_else(|| "unset".to_string());
    format!(
        "fastboot: {SERVICE_PROP}={} {READY}={} {CONFIG}={} {STATE}={} {CONTROLLER}={} gadget={}",
        said(&status.service),
        said(&status.ready),
        said(&status.config),
        said(&status.state),
        said(&status.controller),
        if gadget { "present" } else { "missing" }
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// A property area that says what a test tells it to.
    #[derive(Default)]
    struct Fixed(HashMap<&'static str, &'static str>);

    impl Fixed {
        fn of(pairs: &[(&'static str, &'static str)]) -> Fixed {
            Fixed(pairs.iter().copied().collect())
        }
    }

    impl Props for Fixed {
        fn get(&self, name: &str) -> Option<String> {
            self.0.get(name).map(|v| (*v).to_string())
        }

        fn set(&self, _name: &str, _value: &str) -> Result<()> {
            Ok(())
        }
    }

    fn status(service: Option<&'static str>, ready: Option<&'static str>, state: Option<&'static str>) -> Status {
        Status {
            service: service.map(str::to_string),
            ready: ready.map(str::to_string),
            config: Some("fastboot".to_string()),
            state: state.map(str::to_string),
            controller: Some("a600000.dwc3".to_string()),
        }
    }

    #[test]
    fn the_missing_step_is_the_first_one_init_has_not_taken() {
        assert_eq!(missing(&status(None, None, Some("none"))), Some(Step::Service));
        assert_eq!(missing(&status(Some("stopped"), Some("0"), Some("none"))), Some(Step::Service));
        assert_eq!(missing(&status(Some("running"), Some("0"), Some("none"))), Some(Step::Ready));
        assert_eq!(missing(&status(Some("running"), Some("1"), Some("none"))), Some(Step::State));
        assert_eq!(missing(&status(Some("running"), Some("1"), Some("fastboot"))), None);
    }

    #[test]
    fn a_port_still_on_fastboot_without_fastbootd_is_not_serving() {
        let props = Fixed::of(&[(SERVICE_PROP, "stopped"), (READY, "1"), (CONFIG, "fastboot"), (STATE, "fastboot")]);
        let why = format!("{:#}", serving(&props).unwrap_err());
        assert!(why.contains(SERVICE_PROP), "{why}");
    }

    #[test]
    fn a_port_fastbootd_serves_is_left_alone() {
        let props = Fixed::of(&[(SERVICE_PROP, "running"), (READY, "1"), (CONFIG, "fastboot"), (STATE, "fastboot")]);
        assert!(serving(&props).is_ok());
    }

    #[test]
    fn the_reason_names_the_step_the_port_stopped_at() {
        let not_started = Fixed::of(&[(SERVICE_PROP, "stopped"), (READY, "0"), (CONFIG, "fastboot"), (STATE, "none")]);
        assert!(because(&not_started).contains("fastbootd"));
        let not_ready = Fixed::of(&[(SERVICE_PROP, "running"), (READY, "0"), (CONFIG, "fastboot"), (STATE, "none")]);
        assert!(because(&not_ready).contains(EP0));
        let not_bound = Fixed::of(&[(SERVICE_PROP, "running"), (READY, "1"), (CONFIG, "fastboot"), (STATE, "none")]);
        assert!(because(&not_bound).contains(CONTROLLER));
    }

    #[test]
    fn the_line_carries_every_property_the_port_answers_with() {
        let said = describe(&status(Some("running"), Some("1"), Some("fastboot")), true);
        for part in [SERVICE_PROP, READY, CONFIG, STATE, CONTROLLER, "present"] {
            assert!(said.contains(part), "{said} names {part}");
        }
        assert!(describe(&status(Some("running"), Some("1"), Some("fastboot")), false).contains("missing"));
    }

    #[test]
    fn the_gadget_is_looked_for_where_init_makes_it() {
        let root = std::env::temp_dir().join(format!("skyward-fastboot-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        assert!(!gadget_ready_at(&root));
        std::fs::create_dir_all(root.join(FUNCTION.trim_start_matches('/'))).unwrap();
        std::fs::create_dir_all(root.join(MOUNT.trim_start_matches('/'))).unwrap();
        assert!(!gadget_ready_at(&root), "the function without its endpoint is not a fastboot function");
        std::fs::write(root.join(EP0.trim_start_matches('/')), b"").unwrap();
        assert!(gadget_ready_at(&root));
        std::fs::remove_dir_all(&root).unwrap();
    }
}
