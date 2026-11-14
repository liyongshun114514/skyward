//! The first screen: the frame the panel shows before the pages' own first
//! frame, while the slot's partitions are still being mounted.
//!
//! Ported as it is from `TvTzzz/skyward-oneplus-turbo-6v` at `d668e18`,
//! `crates/skyward/src/ui.rs::splash`: the reference has no frame-by-frame
//! animation there, only this one still frame. `Skyward` is set in the
//! title face across the panel, and「正在启动…」under it in the body face,
//! both from the left margin the layout uses, on the page's own background
//! colour so the panel never shows black before the pages do.
//!
//! The frame is drawn from the panel's size alone, in the reference's
//! proportions: the title's em is 13% of the width, its pen starts 8% in,
//! its baseline sits 46% down, and the line under it is 3.6% wide, one and
//! a fifth title lines below that baseline. Nothing here reads a settings
//! file, opens a display or waits: the runtime draws it where it likes, and
//! `skyward/src/ui.rs` hands the frame to the surface it keeps for the
//! pages.

use crate::canvas::Canvas;
use crate::color::Rgba;
use crate::font::Fonts;

/// The wordmark, as the reference spells it, in every language.
pub const TITLE: &str = "Skyward";

/// The line under the wordmark, as the reference writes it.
pub const STARTING: &str = "正在启动…";

/// The title's em, of the panel's width.
const TITLE_OF_WIDTH: f32 = 0.13;
/// Where the title's pen starts, of the panel's width.
const LEFT_OF_WIDTH: f32 = 0.08;
/// Where the title's baseline sits, of the panel's height.
const BASELINE_OF_HEIGHT: f32 = 0.46;
/// The line's em, of the panel's width.
const HINT_OF_WIDTH: f32 = 0.036;
/// How far under the title's baseline the line's baseline sits, in title
/// ems.
const HINT_GAP: f32 = 1.15;

/// The three colours the first screen is drawn in, as the reference has
/// them: the page's field, the wordmark's ink, and the softer ink of the
/// line under it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Colors {
    pub bg: Rgba,
    pub ink: Rgba,
    pub hint: Rgba,
}

/// The colours the first screen is drawn in, dark or light as the pages
/// were kept.
pub fn colors(dark: bool) -> Colors {
    match dark {
        true => Colors {
            bg: Rgba::rgb(0x0e, 0x10, 0x14),
            ink: Rgba::rgb(0xf2, 0xf3, 0xf5),
            hint: Rgba::rgb(0x8a, 0x90, 0x98),
        },
        false => Colors {
            bg: Rgba::rgb(0xf7, 0xf7, 0xfa),
            ink: Rgba::rgb(0x14, 0x14, 0x18),
            hint: Rgba::rgb(0x7a, 0x7a, 0x84),
        },
    }
}

/// Where the first screen's two lines lie on a panel `w` × `h`: the pen and
/// em of the title, its baseline, the line under it, and the width both are
/// kept within, the margins on either side.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layout {
    pub title_x: f32,
    pub title_px: f32,
    pub baseline: f32,
    pub hint_px: f32,
    pub hint_baseline: f32,
    pub max_w: f32,
}

/// The reference's layout on a panel `w` × `h`, rounded to whole pixels as
/// the reference rounds it.
pub fn layout(w: u32, h: u32) -> Layout {
    let px = (w as f32 * TITLE_OF_WIDTH).round();
    let x = (w as f32 * LEFT_OF_WIDTH).round();
    Layout {
        title_x: x,
        title_px: px,
        baseline: (h as f32 * BASELINE_OF_HEIGHT).round(),
        hint_px: (w as f32 * HINT_OF_WIDTH).round(),
        hint_baseline: (h as f32 * BASELINE_OF_HEIGHT).round() + px * HINT_GAP,
        max_w: w as f32 - x * 2.0,
    }
}

/// Draw the first screen over `out`, in the theme `dark` says: the
/// reference's frame, whole.
///
/// `out` is a canvas of the panel's size that holds all of itself, already
/// filled with the field's colour ([`colors`]`(dark).bg`): this draws only
/// the two lines on it, so a runtime may keep the field from one frame to
/// the next.
pub fn draw(out: &mut Canvas, fonts: &Fonts, dark: bool) {
    let l = layout(out.w, out.h);
    let Colors { ink, hint, .. } = colors(dark);
    out.text(&fonts.title, TITLE, l.title_x, l.baseline, l.title_px, ink, Some(l.max_w));
    out.text(&fonts.body, STARTING, l.title_x, l.hint_baseline, l.hint_px, hint, Some(l.max_w));
}

/// The first screen on a panel `w` × `h`, in the theme `dark` says: a canvas
/// of the panel's size, holding all of itself, filled with the field's
/// colour and inked with the two lines.
pub fn frame(fonts: &Fonts, w: u32, h: u32, dark: bool) -> Canvas {
    let mut out = Canvas::filled(w, h, colors(dark).bg);
    draw(&mut out, fonts, dark);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The typeface the pages are drawn in, out of the repository's own
    /// resources, as the rest of the crate's tests load it.
    fn fonts() -> Fonts {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../resources/fonts");
        Fonts::load(format!("{dir}/MiSansVF.ttf"), 0, format!("{dir}/KirkMono-Regular.ttf")).unwrap()
    }

    /// The pixel at (`x`, `y`) of a whole canvas.
    fn at(c: &Canvas, x: u32, y: u32) -> [u8; 4] {
        assert!(c.whole(), "a whole canvas");
        let i = ((y * c.w + x) * 4) as usize;
        [c.data[i], c.data[i + 1], c.data[i + 2], c.data[i + 3]]
    }

    /// The bounding box of the pixels that stand off the field, in
    /// `rows` and the columns of the canvas: left, top, right, bottom, all
    /// beyond the field by more than `over` in any channel.
    fn ink(c: &Canvas, field: Rgba, over: i32, rows: std::ops::Range<u32>) -> Option<(u32, u32, u32, u32)> {
        let mut found: Option<(u32, u32, u32, u32)> = None;
        for y in rows {
            for x in 0..c.w {
                let p = at(c, x, y);
                let far = |a: u8, b: u8| (a as i32 - b as i32).abs() > over;
                if far(p[0], field.r) || far(p[1], field.g) || far(p[2], field.b) {
                    found = Some(match found {
                        Some((l, t, r, b)) => (l.min(x), t.min(y), r.max(x), b.max(y)),
                        None => (x, y, x, y),
                    });
                }
            }
        }
        found
    }

    #[test]
    fn the_layout_is_the_references_on_the_plk110_panel() {
        // 1272 × 2772 at 560 dpi, the panel this port runs on.
        let l = layout(1272, 2772);
        assert_eq!((l.title_x, l.title_px, l.baseline), (102.0, 165.0, 1275.0));
        assert_eq!(l.hint_px, 46.0);
        assert_eq!(l.hint_baseline, 1275.0 + 165.0 * 1.15);
        assert_eq!(l.max_w, 1272.0 - 204.0);
    }

    #[test]
    fn the_layout_is_the_references_on_the_panel_it_was_written_for() {
        // 1440 × 3200 at 560 dpi, the Xiaomi panel the reference draws on.
        let l = layout(1440, 3200);
        assert_eq!((l.title_x, l.title_px, l.baseline), (115.0, 187.0, 1472.0));
        assert_eq!(l.hint_px, 52.0);
        assert_eq!(l.hint_baseline, 1472.0 + 187.0 * 1.15);
    }

    #[test]
    fn the_field_is_the_references_in_both_themes() {
        let dark = colors(true);
        assert_eq!((dark.bg.r, dark.bg.g, dark.bg.b), (0x0e, 0x10, 0x14));
        assert_eq!((dark.ink.r, dark.ink.g, dark.ink.b), (0xf2, 0xf3, 0xf5));
        assert_eq!((dark.hint.r, dark.hint.g, dark.hint.b), (0x8a, 0x90, 0x98));
        let light = colors(false);
        assert_eq!((light.bg.r, light.bg.g, light.bg.b), (0xf7, 0xf7, 0xfa));
        assert_eq!((light.ink.r, light.ink.g, light.ink.b), (0x14, 0x14, 0x18));
        assert_eq!((light.hint.r, light.hint.g, light.hint.b), (0x7a, 0x7a, 0x84));
    }

    #[test]
    fn the_frame_covers_the_panel_in_the_fields_colour() {
        let f = fonts();
        for dark in [true, false] {
            let c = frame(&f, 1272, 2772, dark);
            assert_eq!((c.w, c.h), (1272, 2772));
            assert!(c.whole(), "the first frame holds all of itself");
            let field = colors(dark).bg;
            for (x, y) in [(0, 0), (1271, 0), (0, 2771), (1271, 2771), (1271, 1386)] {
                let p = at(&c, x, y);
                assert_eq!((p[0], p[1], p[2]), (field.r, field.g, field.b), "{x},{y} in the {dark:?} theme");
            }
        }
    }

    #[test]
    fn the_wordmark_is_inked_where_the_reference_puts_it() {
        let f = fonts();
        let c = frame(&f, 1272, 2772, true);
        let l = layout(1272, 2772);
        let (left, top, right, bottom) = ink(&c, colors(true).bg, 40, 900..1400).expect("the wordmark is inked");
        // Its pen starts at 8% of the width; the ink of `S` starts a little
        // after the pen, never before it.
        assert!(left >= l.title_x as u32, "{left} is left of the pen at {}", l.title_x);
        assert!(left <= l.title_x as u32 + 20, "{left} is far right of the pen at {}", l.title_x);
        // The cap line stands above the baseline and `y`'s tail below it.
        assert!(top < l.baseline as u32, "{top} is under the baseline {}", l.baseline);
        assert!(bottom > l.baseline as u32, "{bottom} is above the baseline {}", l.baseline);
        // The 13% em: a capital about 0.7 em tall over the baseline.
        let cap = l.baseline as u32 - top;
        assert!((cap as f32 - l.title_px * 0.7).abs() < l.title_px * 0.12, "the cap is {cap} px at {} px em", l.title_px);
        // Kept within the margins the reference keeps it in.
        assert!((right as f32) < l.title_x + l.max_w, "{right} runs past the right margin");
    }

    #[test]
    fn the_starting_line_sits_one_and_a_fifth_title_lines_below_the_wordmark() {
        let f = fonts();
        let c = frame(&f, 1272, 2772, true);
        let l = layout(1272, 2772);
        // The line's own band, well clear of the wordmark's tail.
        let (left, top, right, bottom) = ink(&c, colors(true).bg, 25, 1400..1520).expect("the starting line is inked");
        assert!((left as f32 - l.title_x).abs() <= 20.0, "{left} is off the title's pen at {}", l.title_x);
        assert!(top < l.hint_baseline as u32 && bottom > l.hint_baseline as u32, "{top}..{bottom} does not cross {}", l.hint_baseline);
        // The line is 3.6% of the width, its ink a little shorter than its
        // em; and it is much shorter than the wordmark above it.
        assert!((right - left) as f32 > l.hint_px * 3.0, "the line is only {} px wide", right - left);
        // Nothing is inked between the wordmark's tail and the line.
        assert_eq!(ink(&c, colors(true).bg, 25, 1330..1400), None, "the space between the two lines is empty");
    }

    #[test]
    fn the_theme_choice_changes_the_inks_but_not_the_places() {
        let f = fonts();
        let dark = frame(&f, 1272, 2772, true);
        let light = frame(&f, 1272, 2772, false);
        let (d, l) = (colors(true), colors(false));
        let dbox = ink(&dark, d.bg, 40, 900..1400).expect("the dark wordmark");
        let lbox = ink(&light, l.bg, 40, 900..1400).expect("the light wordmark");
        assert_eq!(dbox, lbox, "the wordmark does not move with the theme");
        let extremes = |c: &Canvas, b: (u32, u32, u32, u32)| {
            let px = || (b.1..=b.3).flat_map(|y| (b.0..=b.2).map(move |x| (x, y))).map(|(x, y)| at(c, x, y)[0]);
            (px().min().unwrap(), px().max().unwrap())
        };
        let (_, dark_lightest) = extremes(&dark, dbox);
        assert!(dark_lightest > d.bg.r + 180, "the dark theme's ink is light: {dark_lightest}");
        let (light_darkest, _) = extremes(&light, lbox);
        assert!(light_darkest < l.bg.r - 180, "the light theme's ink is dark: {light_darkest}");
    }

    #[test]
    fn the_same_panel_draws_the_same_frame_every_time() {
        let f = fonts();
        let a = frame(&f, 1272, 2772, true);
        let b = frame(&f, 1272, 2772, true);
        assert_eq!(a.data, b.data);
    }

    #[test]
    fn a_frame_drawn_on_a_field_is_the_whole_frame() {
        let f = fonts();
        let mut c = Canvas::filled(1272, 2772, colors(true).bg);
        draw(&mut c, &f, true);
        let whole = frame(&f, 1272, 2772, true);
        assert_eq!(c.data, whole.data);
    }
}
