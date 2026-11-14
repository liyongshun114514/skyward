//! Panel brightness, with the panel's normal range below its HBM range.
use anyhow::{Context, Result, anyhow, ensure};
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) struct Backlight {
    path: PathBuf,
    top: u32,
    pub(crate) level: u32,
}

impl Backlight {
    #[cfg(target_os = "android")]
    pub(crate) fn open() -> Result<Self> {
        Self::open_at(Path::new("/sys/class/backlight"), Path::new("/sys/module/msm_drm/parameters/dsi_display0"), Path::new("/proc/device-tree/soc"))
    }

    fn open_at(devices: &Path, param: &Path, soc: &Path) -> Result<Self> {
        let mut dirs = fs::read_dir(devices)
            .with_context(|| format!("read {}", devices.display()))?
            .map(|e| e.map(|e| e.path()))
            .collect::<std::io::Result<Vec<_>>>()?;
        dirs.sort();
        let dir = dirs.iter().find(|d| d.file_name().is_some_and(|n| n == "panel0-backlight"))
            .or(dirs.first()).ok_or_else(|| anyhow!("no backlight device"))?;
        let read = |name: &str| -> Result<u32> {
            fs::read_to_string(dir.join(name)).with_context(|| format!("read {}/{name}", dir.display()))?
                .trim().parse().with_context(|| format!("parse {name}"))
        };
        let top = normal_max(read("max_brightness")?, param, soc)?;
        let level = read("brightness")?;
        Ok(Self { path: dir.join("brightness"), top, level })
    }

    pub(crate) fn unavailable() -> Self {
        Self { path: PathBuf::new(), top: 0, level: 1 }
    }

    /// No writes for an unavailable controller or an unchanged raw level.
    pub(crate) fn show(&mut self, level: Option<f32>) -> Result<()> {
        if self.top == 0 {
            return Ok(());
        }
        let raw = level.map_or(0, |v| ((v.clamp(0.0, 1.0) * self.top as f32).round() as u32).max(1));
        if raw != self.level {
            fs::write(&self.path, raw.to_string()).with_context(|| format!("write {}", self.path.display()))?;
            eprintln!("backlight {raw}");
            self.level = raw;
        }
        Ok(())
    }
}

const NORMAL_MAX: [(&str, &str); 4] = [
    ("mi,normal-max-brightness-clone", "mi,max-brightness-clone"),
    ("mi,max-brightness-normal", "qcom,mdss-dsi-bl-max-level"),
    ("mi,mdss-dsi-fac-bl-max-level", "qcom,mdss-dsi-bl-max-level"),
    // OnePlus panels provide their normal maximum separately from HBM.
    ("oplus,dsi-bl-normal-max-level", "qcom,mdss-dsi-bl-max-level"),
];

fn normal_max(max: u32, param: &Path, soc: &Path) -> Result<u32> {
    let boot = fs::read_to_string(param).with_context(|| format!("read {}", param.display()))?;
    let (name, _) = boot.trim().split_once(':').with_context(|| format!("{} names no panel: {boot:?}", param.display()))?;
    let node = fs::read_dir(soc).with_context(|| format!("read {}", soc.display()))?
        .map(|e| e.map(|e| e.path().join(name))).collect::<std::io::Result<Vec<_>>>()?
        .into_iter().find(|p| p.is_dir()).ok_or_else(|| anyhow!("panel {name} is not in the device tree"))?;
    let cell = |prop: &str| -> Result<u32> {
        let path = node.join(prop);
        let bytes = fs::read(&path).with_context(|| format!("read {}", path.display()))?;
        let cell: [u8; 4] = bytes.as_slice().try_into().map_err(|_| anyhow!("{} is not one cell", path.display()))?;
        Ok(u32::from_be_bytes(cell))
    };
    let (normal, full) = NORMAL_MAX.iter().find(|(normal, _)| node.join(normal).exists())
        .with_context(|| format!("panel {name} gives its normal maximum as none of {NORMAL_MAX:?}"))?;
    let (normal, full) = (cell(normal)?, cell(full)?);
    ensure!(normal > 0 && normal <= full, "panel {name}: normal maximum {normal} out of {full}");
    let top = (f64::from(max) * f64::from(normal) / f64::from(full)).round() as u32;
    ensure!(top > 0, "panel {name}: normal maximum rounds to zero");
    Ok(top)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Values read from the PLK110's running kernel and device tree.
    const PANEL: &str = "qcom,mdss_dsi_panel_AD296_P_3_A0020_dsc_cmd";

    fn fixture() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        let r = root.path();
        fs::create_dir_all(r.join("backlight/panel0-backlight")).unwrap();
        fs::create_dir_all(r.join("soc/display").join(PANEL)).unwrap();
        fs::write(r.join("panel"), format!("{PANEL}:PanelID-0x00670400:\n")).unwrap();
        fs::write(r.join("backlight/panel0-backlight/max_brightness"), "4094\n").unwrap();
        fs::write(r.join("backlight/panel0-backlight/brightness"), "1073\n").unwrap();
        let panel = r.join("soc/display").join(PANEL);
        fs::write(panel.join("oplus,dsi-bl-normal-max-level"), 3515u32.to_be_bytes()).unwrap();
        fs::write(panel.join("qcom,mdss-dsi-bl-max-level"), 4094u32.to_be_bytes()).unwrap();
        root
    }

    fn open(r: &Path) -> Result<Backlight> {
        Backlight::open_at(&r.join("backlight"), &r.join("panel"), &r.join("soc"))
    }

    #[test]
    fn plk110_start_slider_and_screen_off_use_the_normal_range() {
        let root = fixture();
        let r = root.path();
        let mut light = open(r).unwrap();
        assert_eq!(light.top, 3515);
        assert_eq!(light.level, 1073);
        let file = r.join("backlight/panel0-backlight/brightness");
        for (level, raw) in [(Some(0.7), 2461), (Some(1.0), 3515), (Some(0.02), 70), (None, 0), (Some(0.7), 2461)] {
            light.show(level).unwrap();
            assert_eq!(fs::read_to_string(&file).unwrap(), raw.to_string());
            assert_eq!(light.level, raw);
        }
    }

    #[test]
    fn unavailable_backlight_never_writes_an_empty_path() {
        let mut light = Backlight::unavailable();
        light.show(Some(0.7)).unwrap();
        light.show(None).unwrap();
        assert_eq!(light.level, 1);
    }

    #[test]
    fn xiaomi_finer_scale_keeps_its_existing_mapping() {
        let root = fixture();
        let panel = root.path().join("soc/display").join(PANEL);
        fs::remove_file(panel.join("oplus,dsi-bl-normal-max-level")).unwrap();
        fs::write(panel.join("mi,normal-max-brightness-clone"), 8192u32.to_be_bytes()).unwrap();
        fs::write(panel.join("mi,max-brightness-clone"), 16384u32.to_be_bytes()).unwrap();
        assert_eq!(open(root.path()).unwrap().top, 2047);
    }

    #[test]
    fn invalid_panel_range_is_rejected_without_writing_brightness() {
        let root = fixture();
        let r = root.path();
        let normal = r.join("soc/display").join(PANEL).join("oplus,dsi-bl-normal-max-level");
        for value in [0u32, 4095] {
            fs::write(&normal, value.to_be_bytes()).unwrap();
            assert!(open(r).is_err());
        }
        assert_eq!(fs::read_to_string(r.join("backlight/panel0-backlight/brightness")).unwrap(), "1073\n");
    }
}
