//! Stock ratatui widgets and a tachyonfx intro, styled by Halftone themes.
//! `cargo run --release --features ratatui --example widgets`. `t` changes theme, `q` quits.

use halftone::prelude::*;
use halftone::rat::core::{layout::{Constraint as C, Layout, Margin}, style::Style as RStyle, symbols::Marker, text::Line, widgets::Widget};
use halftone::rat::fx::{self, fx as e, Interpolation, Motion};
use halftone::rat::widgets::{
    barchart::{Bar, BarChart},
    block::Block,
    borders::BorderType,
    chart::{Axis, Chart, Dataset, GraphType},
    gauge::LineGauge,
    table::{Row, Table},
    tabs::Tabs,
};
use halftone::theme::{Theme, PRESETS};

const SERVICES: [(&str, &str, f32); 5] = [("api", "v4.12.0", 38.0), ("auth", "v2.3.1", 12.0), ("search", "v1.9.4", 64.0), ("billing", "v3.0.2", 21.0), ("edge", "v7.1.0", 7.0)];
const REGIONS: [&str; 5] = ["iad", "sfo", "fra", "nrt", "syd"];

struct Dash {
    idx: usize,
    from: Theme,
    blend: f32,
    idle: f32,
    dt: f64,
    intro: fx::Effect,
}

impl Dash {
    fn theme(&mut self, i: usize) {
        let t = self.blend.clamp(0.0, 1.0);
        self.from = self.from.mix(&PRESETS[self.idx], t * t * (3.0 - 2.0 * t));
        (self.idx, self.blend, self.idle) = (i % PRESETS.len(), 0.0, 0.0);
    }
}

fn wave(t: f32, i: f32) -> f32 {
    0.5 + 0.28 * (t * 0.9 + i * 0.21).sin() + 0.14 * (t * 2.3 + i * 0.67).sin()
}

impl App for Dash {
    fn update(&mut self, ev: &Event, ctx: &mut Ctx) {
        match *ev {
            Event::Tick(dt) => {
                self.dt = dt;
                self.blend += dt as f32 / 0.6;
                self.idle += dt as f32;
                if self.idle > 4.0 {
                    self.theme(self.idx + 1);
                }
                let t = self.blend.clamp(0.0, 1.0);
                theme::set(self.from.mix(&PRESETS[self.idx], t * t * (3.0 - 2.0 * t)));
            }
            _ => match ev.ch() {
                Some('t') => self.theme(self.idx + 1),
                Some('q') => ctx.quit(),
                _ => {}
            },
        }
    }

    fn view(&mut self, f: &mut Frame) {
        let (t, area) = (f.time as f32, f.area);
        let (text, muted, faint, accent, line) = (theme::text().into(), theme::muted().into(), theme::faint().into(), theme::accent().into(), theme::line().into());
        let bg: halftone::rat::core::style::Color = theme::bg().into();
        let card = |title: &'static str| {
            Block::bordered().style(RStyle::new().bg(bg)).border_type(BorderType::Rounded).border_style(RStyle::new().fg(line)).title(Line::from(format!(" {title} ")).style(RStyle::new().fg(muted)))
        };
        let (idx, dt, intro) = (self.idx, self.dt, &mut self.intro);

        f.ratatui(area, |r, buf| {
            let [head, top, bottom] = Layout::vertical([C::Length(1), C::Fill(1), C::Length(SERVICES.len() as u16 + 4)]).spacing(1).areas(r.inner(Margin::new(2, 1)));
            let [chart, bars] = Layout::horizontal([C::Fill(2), C::Fill(1)]).spacing(2).areas(top);
            let [table, gauges] = Layout::horizontal([C::Fill(2), C::Fill(1)]).spacing(2).areas(bottom);

            Tabs::new(PRESETS.iter().map(|p| p.name))
                .select(idx)
                .style(RStyle::new().fg(faint))
                .highlight_style(RStyle::new().fg(text).bold())
                .divider(" ")
                .render(head, buf);

            // Two latency series drifting left, a sample every half second.
            let n = 80;
            let series = |k: f32| -> Vec<(f64, f64)> { (0..n).map(|i| (i as f64, (k * wave(t, i as f32 + t * 2.0)) as f64)).collect() };
            let (p50, p99) = (series(60.0), series(100.0));
            Chart::new(vec![
                Dataset::default().marker(Marker::Braille).graph_type(GraphType::Line).style(RStyle::new().fg(faint)).data(&p99),
                Dataset::default().marker(Marker::Braille).graph_type(GraphType::Line).style(RStyle::new().fg(accent)).data(&p50),
            ])
            .block(card("latency · ms"))
            .style(RStyle::new().bg(bg))
            .x_axis(Axis::default().bounds([0.0, n as f64]))
            .y_axis(Axis::default().bounds([0.0, 100.0]).labels(["0", "50", "100"].map(|s| Line::from(s).style(RStyle::new().fg(muted)))).style(RStyle::new().fg(line)))
            .render(chart, buf);

            let regions: Vec<Bar> = REGIONS
                .iter()
                .enumerate()
                .map(|(i, name)| Bar::default().value((100.0 * wave(t * 0.6, i as f32 * 3.0)) as u64).label(Line::from(*name)).text_value(""))
                .collect();
            BarChart::default()
                .block(card("traffic"))
                .data(halftone::rat::widgets::barchart::BarGroup::default().bars(&regions))
                .bar_width((bars.width.saturating_sub(4) / REGIONS.len() as u16).saturating_sub(2).max(1))
                .bar_gap(2)
                .max(100)
                .bar_style(RStyle::new().fg(accent))
                .label_style(RStyle::new().fg(muted))
                .render(bars, buf);

            let rows = SERVICES.iter().enumerate().map(|(i, (name, rev, ms))| {
                let ms = ms * (0.8 + 0.4 * wave(t, i as f32 * 5.0));
                let (dot, c) = if ms > 70.0 { ("●", theme::warn().into()) } else { ("●", theme::good().into()) };
                Row::new([Line::from(dot).style(RStyle::new().fg(c)), Line::from(*name).style(RStyle::new().fg(text)), Line::from(format!("{ms:.0} ms")).style(RStyle::new().fg(muted)), Line::from(*rev).style(RStyle::new().fg(faint))])
            });
            Table::new(rows, [C::Length(2), C::Fill(1), C::Length(8), C::Length(8)])
                .header(Row::new(["", "service", "p50", "rev"]).style(RStyle::new().fg(faint)).bottom_margin(1))
                .block(card("services"))
                .column_spacing(2)
                .render(table, buf);

            let inner = card("hosts").inner(gauges);
            card("hosts").render(gauges, buf);
            let rows = Layout::vertical([C::Length(1); 4]).spacing(1).split(inner.inner(Margin::new(1, 0)));
            for (i, name) in ["cpu", "mem", "disk", "net"].iter().enumerate() {
                LineGauge::default()
                    .ratio(wave(t * 0.7, i as f32 * 4.0).clamp(0.0, 1.0) as f64)
                    .label(Line::from(format!("{name:<5}")).style(RStyle::new().fg(muted)))
                    .filled_style(RStyle::new().fg(accent))
                    .unfilled_style(RStyle::new().fg(line))
                    .render(rows[i], buf);
            }

            intro.process(fx::Duration::from_millis((dt * 1000.0) as u32), buf, r);
        });
    }
}

halftone::main!(Dash {
    idx: 0,
    from: PRESETS[0],
    blend: 1.0,
    idle: 0.0,
    dt: 0.0,
    intro: e::sweep_in(Motion::LeftToRight, 24, 0, Color::rgb(PRESETS[0].bg), (900, Interpolation::QuadOut)),
});
