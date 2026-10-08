use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Row, Table, Wrap};

use fika_station::time::{age, hms};

use crate::app::App;

pub fn draw(f: &mut Frame, app: &App) {
    let area = f.area();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // header
            Constraint::Min(8),    // chat + heard
            Constraint::Length(8), // waterfall
            Constraint::Length(5), // log
            Constraint::Length(3), // input
        ])
        .split(area);

    draw_header(f, app, rows[0]);
    let mid = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(68), Constraint::Percentage(32)])
        .split(rows[1]);
    draw_chat(f, app, mid[0]);
    draw_heard(f, app, mid[1]);
    draw_waterfall(f, app, rows[2]);
    draw_log(f, app, rows[3]);
    draw_input(f, app, rows[4]);
    if app.show_help {
        draw_help(f, area);
    }
}

fn draw_header(f: &mut Frame, app: &App, area: Rect) {
    let st = &app.station;
    let freq = match st.rig.freq_hz {
        Some(hz) => format!("{:.3} kHz", hz as f64 / 1000.0),
        None => "---".into(),
    };
    let ptt = if st.rig.ptt {
        Span::styled(
            " TX ",
            Style::default()
                .bg(Color::Red)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(" RX ", Style::default().bg(Color::Green).fg(Color::Black))
    };
    let rig = if st.cfg.rig.kind != fika_station::config::RigKind::None && !st.rig.connected {
        format!("{} (disconnected)", st.rig.name)
    } else {
        st.rig.name.clone()
    };
    let line = Line::from(vec![
        Span::styled(
            format!(" fika  {} ", st.cfg.station.call),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw(st.cfg.station.grid.clone().unwrap_or_default()),
        Span::raw("  "),
        ptt,
        Span::raw(format!(
            "  {freq}  {rig}  {}  to {}",
            st.profile, st.dest_label
        )),
        Span::raw(if st.cfg.audio.loopback {
            "  [loopback]"
        } else {
            ""
        }),
    ]);
    f.render_widget(
        Paragraph::new(line).style(Style::default().bg(Color::DarkGray)),
        area,
    );
}

fn draw_chat(f: &mut Frame, app: &App, area: Rect) {
    let inner_h = area.height.saturating_sub(2) as usize;
    let st = &app.station;
    let mut lines: Vec<Line> = Vec::new();
    for c in &st.chat {
        let who = if c.mine {
            Span::styled(
                format!("{} → {}", c.from, c.to),
                Style::default().fg(Color::Cyan),
            )
        } else {
            Span::styled(
                format!("{} → {}", c.from, c.to),
                Style::default().fg(Color::Yellow),
            )
        };
        let mut meta = Vec::new();
        if let Some(s) = c.snr_db {
            meta.push(format!("{s:+.0} dB"));
        }
        if let Some(s) = &c.status {
            meta.push(s.clone());
        }
        let meta = if meta.is_empty() {
            String::new()
        } else {
            format!("  [{}]", meta.join(", "))
        };
        lines.push(Line::from(vec![
            Span::styled(
                format!("{} ", hms(c.epoch)),
                Style::default().fg(Color::DarkGray),
            ),
            who,
            Span::styled(meta, Style::default().fg(Color::DarkGray)),
        ]));
        lines.push(Line::from(format!("    {}", c.text)));
    }
    let skip = lines.len().saturating_sub(inner_h);
    let shown: Vec<Line> = lines.into_iter().skip(skip).collect();
    f.render_widget(
        Paragraph::new(shown)
            .wrap(Wrap { trim: false })
            .block(Block::default().borders(Borders::ALL).title(" messages ")),
        area,
    );
}

fn draw_heard(f: &mut Frame, app: &App, area: Rect) {
    let rows: Vec<Row> = app
        .station
        .heard
        .sorted()
        .into_iter()
        .map(|e| {
            Row::new(vec![
                e.call.clone(),
                format!(
                    "{:+.0}{}",
                    e.snr_db,
                    if e.profile == fika_modem::Profile::Slow {
                        "s"
                    } else {
                        ""
                    }
                ),
                e.grid.clone().unwrap_or_default(),
                age(e.last_epoch),
            ])
        })
        .collect();
    let table = Table::new(
        rows,
        [
            Constraint::Length(8),
            Constraint::Length(5),
            Constraint::Length(5),
            Constraint::Length(4),
        ],
    )
    .header(
        Row::new(vec!["call", "dB", "grid", "age"])
            .style(Style::default().add_modifier(Modifier::BOLD)),
    )
    .block(Block::default().borders(Borders::ALL).title(" heard "));
    f.render_widget(table, area);
}

fn draw_waterfall(f: &mut Frame, app: &App, area: Rect) {
    let st = &app.station;
    let level = format!(
        " waterfall 300–2700 Hz   in {:+.0} dBFS{} ",
        st.level_db,
        if st.peak > 0.95 { " CLIP" } else { "" }
    );
    let block = Block::default().borders(Borders::ALL).title(level);
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.width < 10 || inner.height < 2 {
        return;
    }
    let width = inner.width as usize;
    // Frequency ruler: a tick every 500 Hz, labelled in hundreds of hertz.
    let mut ruler = vec![' '; width];
    for hz in (500..=2500).step_by(500) {
        let x = ((hz as f64 - 300.0) / 2400.0 * width as f64) as usize;
        for (i, ch) in format!("{}", hz / 100).chars().enumerate() {
            if x + i < width {
                ruler[x + i] = ch;
            }
        }
    }
    let ruler: String = ruler.into_iter().collect();
    let mut lines = vec![Line::from(Span::styled(
        ruler,
        Style::default().fg(Color::DarkGray),
    ))];
    let shades = [' ', '░', '▒', '▓', '█'];
    let n_rows = inner.height as usize - 1;
    let rows: Vec<&Vec<f32>> = st.spectrum.iter().rev().take(n_rows).collect();
    // Reference: median of the most recent row, so the floor is blank.
    for row in rows {
        let mut sorted = row.clone();
        sorted.sort_by(|a, b| a.total_cmp(b));
        let floor = sorted.get(sorted.len() / 2).copied().unwrap_or(0.0);
        let per_col = row.len().max(1) as f64 / width as f64;
        let s: String = (0..width)
            .map(|x| {
                let a = (x as f64 * per_col) as usize;
                let b = (((x + 1) as f64 * per_col) as usize)
                    .max(a + 1)
                    .min(row.len());
                let m = row[a.min(row.len() - 1)..b]
                    .iter()
                    .fold(f32::MIN, |m, &v| m.max(v));
                let db = m - floor;
                let idx = ((db / 6.0).clamp(0.0, 4.0)) as usize;
                shades[idx]
            })
            .collect();
        lines.push(Line::from(Span::styled(
            s,
            Style::default().fg(Color::Blue),
        )));
    }
    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_log(f: &mut Frame, app: &App, area: Rect) {
    let n = area.height.saturating_sub(2) as usize;
    let lines: Vec<Line> = app
        .station
        .log
        .iter()
        .rev()
        .take(n)
        .rev()
        .map(|s| Line::from(s.as_str()))
        .collect();
    f.render_widget(
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(" log ")),
        area,
    );
}

fn draw_input(f: &mut Frame, app: &App, area: Rect) {
    let st = &app.station;
    let input = app.editor.text();
    let preview = if input.starts_with('/') || input.is_empty() {
        String::new()
    } else {
        match st.preview(&input) {
            Some((bits, blocks, air)) => format!(
                " {} chars, {bits} bits, {blocks} block{}, {air:.1} s ",
                input.chars().count(),
                if blocks == 1 { "" } else { "s" }
            ),
            None => " too long ".into(),
        }
    };
    let busy = st.tx_busy.as_ref().map(|(l, started, air)| {
        format!(
            " transmitting {l} {:.0}/{air:.0} s ",
            started.elapsed().as_secs_f64().min(*air)
        )
    });
    let title = format!(
        " to {} {}{}",
        st.dest_label,
        preview,
        busy.unwrap_or_default()
    );
    let status = Span::styled(
        format!("  {}", app.status),
        Style::default().fg(Color::DarkGray),
    );
    let text = Line::from(vec![Span::raw(input.clone()), status]);
    let block = Block::default().borders(Borders::ALL).title(title);
    f.render_widget(Paragraph::new(text).block(block), area);
    let x = area.x
        + 1
        + input
            .chars()
            .take(app.editor.cursor())
            .map(|c| if c.is_ascii() { 1 } else { 2 })
            .sum::<usize>() as u16;
    f.set_cursor_position((x.min(area.x + area.width - 2), area.y + 1));
}

fn draw_help(f: &mut Frame, area: Rect) {
    let w = 60.min(area.width.saturating_sub(4));
    let h = 18.min(area.height.saturating_sub(2));
    let rect = Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    );
    f.render_widget(Clear, rect);
    let text = vec![
        Line::from("Type a message and press Enter to send it."),
        Line::from(""),
        Line::from("/to @group | CALL | all   change destination"),
        Line::from("/profile fast|slow        speed profile"),
        Line::from("/beacon                   send a beacon"),
        Line::from("/clear                    clear the chat"),
        Line::from("/quit  (or Ctrl-C)        exit"),
        Line::from("Esc or C-g                ABORT transmission, PTT off"),
        Line::from(""),
        Line::from("Editing: Emacs keys. C-a C-e C-b C-f M-b M-f move,"),
        Line::from("C-h C-d delete, C-k C-u C-w M-d kill, C-y yank,"),
        Line::from("C-p C-n history, C-t transpose, C-l clears the chat."),
        Line::from("Direct messages to a callsign request an ACK."),
        Line::from("F1 or Esc closes this help."),
    ];
    f.render_widget(
        Paragraph::new(text).block(Block::default().borders(Borders::ALL).title(" help ")),
        rect,
    );
}
