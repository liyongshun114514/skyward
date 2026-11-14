//! Draw the first screen to PNG files, on the computer: the frame the
//! reference draws before its pages, at the panels this port cares about.
//!
//! Run with the directory holding MiSansVF.ttf and KirkMono-Regular.ttf,
//! then the directory the pictures go in:
//!
//! ```sh
//! cargo run -p skyward-ui --example splash_frames -- resources/fonts out
//! ```
//!
//! The names say the panel and the theme: `plk110-dark.png`,
//! `plk110-light.png` and `reference-1440x3200-dark.png`.

use skyward_ui::{splash, Fonts};
use std::fs::File;
use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let fonts = args.get(1).expect("the directory holding MiSansVF.ttf and KirkMono-Regular.ttf");
    let out = args.get(2).expect("the directory the pictures go in");
    let f = Fonts::load(format!("{fonts}/MiSansVF.ttf"), 0, format!("{fonts}/KirkMono-Regular.ttf")).expect("load the fonts");
    std::fs::create_dir_all(out).expect("make the pictures' directory");
    // The panels: this port's 1272 × 2772 at 560 dpi, and the 1440 × 3200
    // panel the reference was laid out on, in the dark theme.
    let shots = [
        ("plk110-dark", 1272, 2772, true),
        ("plk110-light", 1272, 2772, false),
        ("reference-1440x3200-dark", 1440, 3200, true),
    ];
    for (name, w, h, dark) in shots {
        let frame = splash::frame(&f, w, h, dark);
        let path = Path::new(out).join(format!("{name}.png"));
        let file = File::create(&path).expect("make the picture");
        frame.write_png(file).expect("write the picture");
        let l = splash::layout(w, h);
        println!(
            "{name}.png: {w}x{h}, title {} px at ({}, {}), line {} px at {}",
            l.title_px, l.title_x, l.baseline, l.hint_px, l.hint_baseline
        );
    }
}
