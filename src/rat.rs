//! The ratatui ecosystem inside a Halftone frame (feature `ratatui`).
//! Any ratatui widget or tachyonfx effect draws through `Frame::ratatui`, in the terminal and in the browser.

pub use ratatui_core as core;
pub use ratatui_widgets as widgets;
pub use tachyonfx as fx;

use crate::{attr, Color, Frame, Rect};
use ratatui_core::{buffer::Buffer as RBuf, layout::Rect as RRect, style::{Color as RColor, Modifier}};

impl From<Color> for RColor {
    fn from(c: Color) -> RColor {
        match c {
            Color::Reset => RColor::Reset,
            Color::Rgb(r, g, b) => RColor::Rgb(r, g, b),
        }
    }
}

/// xterm's default 16-colour palette, so named and indexed ratatui colours land as real RGB.
const ANSI: [u32; 16] = [
    0x000000, 0xcd0000, 0x00cd00, 0xcdcd00, 0x0000ee, 0xcd00cd, 0x00cdcd, 0xe5e5e5,
    0x7f7f7f, 0xff0000, 0x00ff00, 0xffff00, 0x5c5cff, 0xff00ff, 0x00ffff, 0xffffff,
];

fn from_r(c: RColor) -> Color {
    use RColor::*;
    let i = match c {
        Reset => return Color::Reset,
        Rgb(r, g, b) => return Color::Rgb(r, g, b),
        Indexed(i) => i,
        Black => 0, Red => 1, Green => 2, Yellow => 3, Blue => 4, Magenta => 5, Cyan => 6, Gray => 7,
        DarkGray => 8, LightRed => 9, LightGreen => 10, LightYellow => 11, LightBlue => 12, LightMagenta => 13, LightCyan => 14, White => 15,
    };
    match i {
        0..=15 => Color::rgb(ANSI[i as usize]),
        16..=231 => {
            let lvl = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
            let i = i - 16;
            Color::Rgb(lvl(i / 36), lvl(i / 6 % 6), lvl(i % 6))
        }
        _ => {
            let g = 8 + (i - 232) * 10;
            Color::Rgb(g, g, g)
        }
    }
}

const MODS: [(Modifier, u8); 4] = [(Modifier::BOLD, attr::BOLD), (Modifier::DIM, attr::DIM), (Modifier::ITALIC, attr::ITALIC), (Modifier::UNDERLINED, attr::UNDERLINE)];

impl Frame<'_> {
    /// Runs `draw` on a ratatui buffer over `area`, seeded with what's already there, then copies it back.
    pub fn ratatui(&mut self, area: Rect, draw: impl FnOnce(RRect, &mut RBuf)) {
        let area = Rect::new(area.x, area.y, area.w.min(self.buf.w.saturating_sub(area.x)), area.h.min(self.buf.h.saturating_sub(area.y)));
        let r = RRect::new(area.x, area.y, area.w, area.h);
        let mut rb = RBuf::empty(r);
        for y in area.y..area.bottom() {
            for x in area.x..area.right() {
                let c = self.buf.get(x, y);
                let m = MODS.iter().filter(|(_, a)| c.attrs & a != 0).fold(Modifier::empty(), |m, (rm, _)| m | *rm);
                rb[(x, y)].set_char(c.ch).set_fg(c.fg.into()).set_bg(c.bg.into()).modifier = m;
            }
        }
        draw(r, &mut rb);
        for y in area.y..area.bottom() {
            for x in area.x..area.right() {
                let rc = &rb[(x, y)];
                if let Some(c) = self.buf.cell(x, y) {
                    // Wide glyphs leave an empty continuation cell; Halftone draws one char per cell.
                    c.ch = rc.symbol().chars().next().unwrap_or(' ');
                    c.fg = from_r(rc.fg);
                    c.bg = from_r(rc.bg);
                    c.attrs = MODS.iter().filter(|(m, _)| rc.modifier.contains(*m)).fold(0, |a, (_, b)| a | b);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Buffer;

    #[test]
    fn round_trips_widgets_and_colours() {
        let mut buf = Buffer::new(10, 2);
        let (mut hits, area) = (vec![], Rect::new(0, 0, 10, 2));
        let mut f = Frame { buf: &mut buf, area, hits: &mut hits, mouse: None, time: 0.0 };
        f.ratatui(area, |r, b| {
            use ratatui_core::{style::Style, widgets::Widget};
            ratatui_widgets::paragraph::Paragraph::new("hi").style(Style::new().fg(RColor::Red).bold()).render(r, b);
        });
        let c = buf.get(1, 0);
        assert_eq!((c.ch, c.fg, c.attrs), ('i', Color::rgb(0xcd0000), attr::BOLD));
        assert_eq!(from_r(RColor::Indexed(196)), Color::Rgb(255, 0, 0));
        assert_eq!(from_r(RColor::Indexed(244)), Color::Rgb(128, 128, 128));
    }
}
