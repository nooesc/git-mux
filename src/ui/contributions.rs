use chrono::Datelike;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use crate::app::AppState;
use crate::github::contributions::ContributionDay;

pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
    if state.contributions.days.is_empty() {
        let loading = if state.loading.contains(&crate::app::View::Graph) {
            "Loading contribution graph..."
        } else {
            "No contribution data"
        };
        let p = Paragraph::new(loading)
            .alignment(ratatui::layout::Alignment::Center)
            .block(Block::default().borders(Borders::ALL).title(" Contributions "));
        frame.render_widget(p, area);
        return;
    }

    let [graph_area, stats_area] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(3),
    ])
    .areas(area);

    render_heatmap(frame, graph_area, &state.contributions.days);
    render_stats(frame, stats_area, &state.contributions.days, state.contributions.total);
}

fn render_heatmap(frame: &mut Frame, area: Rect, days: &[ContributionDay]) {
    // Organize days into a grid: 7 rows (weekdays) x N columns (weeks)
    // GitHub contribution calendar starts on Sunday (weekday 0 in chrono = Monday)
    // We'll arrange: rows 0-6 = Mon, Tue, Wed, Thu, Fri, Sat, Sun

    if days.is_empty() {
        return;
    }

    // Group days by week (every 7 days starting from the first Sunday)
    let mut weeks: Vec<[Option<&ContributionDay>; 7]> = Vec::new();
    let mut current_week: [Option<&ContributionDay>; 7] = [None; 7];

    for day in days {
        // chrono: Mon=0, Tue=1, Wed=2, Thu=3, Fri=4, Sat=5, Sun=6
        let weekday = day.date.weekday().num_days_from_monday() as usize;

        // If we're at Sunday (6) and current week has data before it, or if
        // weekday is 0 (Monday) and the previous week had entries, push and start new
        if weekday == 0 && weeks.is_empty() && current_week.iter().any(|d| d.is_some()) {
            weeks.push(current_week);
            current_week = [None; 7];
        } else if weekday == 0 && !weeks.is_empty() {
            weeks.push(current_week);
            current_week = [None; 7];
        }

        current_week[weekday] = Some(day);
    }
    // Push the last partial week
    if current_week.iter().any(|d| d.is_some()) {
        weeks.push(current_week);
    }

    // Determine the year for the title
    let year = days
        .last()
        .map(|d| d.date.format("%Y").to_string())
        .unwrap_or_else(|| "2026".to_string());

    let day_labels = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

    // Build month labels row
    let mut month_spans: Vec<Span> = vec![Span::raw("      ")]; // offset for day labels
    let mut last_month = None;
    for week in &weeks {
        // Use the first available day in the week to determine the month
        let month = week
            .iter()
            .flatten()
            .next()
            .map(|d| d.date.format("%b").to_string());

        if month != last_month {
            if let Some(ref m) = month {
                month_spans.push(Span::styled(
                    m.clone(),
                    Style::default().fg(Color::DarkGray),
                ));
                last_month = month;
            } else {
                month_spans.push(Span::raw(" "));
            }
        } else {
            month_spans.push(Span::raw(" "));
        }
    }

    let mut lines: Vec<Line> = Vec::new();

    // Title
    lines.push(Line::from(Span::styled(
        format!("  Contribution Graph - {}", year),
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));

    // Month labels
    lines.push(Line::from(month_spans));

    // Each row = one weekday
    for (row_idx, day_label) in day_labels.iter().enumerate() {
        let mut spans: Vec<Span> = vec![Span::styled(
            format!("  {} ", day_label),
            Style::default().fg(Color::DarkGray),
        )];

        for week in &weeks {
            let cell = week[row_idx];
            let (ch, style) = match cell {
                Some(day) => level_to_cell(day.level),
                None => (' ', Style::default()),
            };
            spans.push(Span::styled(String::from(ch), style));
        }

        lines.push(Line::from(spans));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Contributions ");

    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, area);
}

fn level_to_cell(level: u8) -> (char, Style) {
    match level {
        0 => ('\u{2591}', Style::default().fg(Color::DarkGray)),           // ░
        1 => ('\u{2592}', Style::default().fg(Color::Green)),              // ▒
        2 => ('\u{2593}', Style::default().fg(Color::Green)),              // ▓
        3 => ('\u{2588}', Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)), // █
        4 => ('\u{2588}', Style::default().fg(Color::LightGreen).add_modifier(Modifier::BOLD)), // █
        _ => ('\u{2591}', Style::default().fg(Color::DarkGray)),
    }
}

fn render_stats(frame: &mut Frame, area: Rect, days: &[ContributionDay], total: u32) {
    let (current_streak, longest_streak) = calculate_streaks(days);

    let stats = Line::from(vec![
        Span::raw("  Total: "),
        Span::styled(
            format_with_commas(total),
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" contributions    "),
        Span::raw("Current streak: "),
        Span::styled(
            format!("{} days", current_streak),
            Style::default().fg(Color::Green),
        ),
        Span::raw("    Longest streak: "),
        Span::styled(
            format!("{} days", longest_streak),
            Style::default().fg(Color::Yellow),
        ),
    ]);

    let paragraph = Paragraph::new(stats)
        .block(Block::default().borders(Borders::TOP));
    frame.render_widget(paragraph, area);
}

fn calculate_streaks(days: &[ContributionDay]) -> (u32, u32) {
    let today = chrono::Utc::now().date_naive();

    // Current streak: count backwards from today (or yesterday if today has 0)
    let mut current = 0u32;
    for day in days.iter().rev() {
        if day.date > today {
            continue;
        }
        if day.count > 0 {
            current += 1;
        } else if day.date == today {
            continue; // today might not be over yet
        } else {
            break;
        }
    }

    // Longest streak
    let mut longest = 0u32;
    let mut run = 0u32;
    for day in days {
        if day.count > 0 {
            run += 1;
            longest = longest.max(run);
        } else {
            run = 0;
        }
    }

    (current, longest)
}

fn format_with_commas(n: u32) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, ch) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(ch);
    }
    result.chars().rev().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn make_day(year: i32, month: u32, day: u32, count: u32, level: u8) -> ContributionDay {
        ContributionDay {
            date: NaiveDate::from_ymd_opt(year, month, day).unwrap(),
            count,
            level,
        }
    }

    #[test]
    fn test_format_with_commas() {
        assert_eq!(format_with_commas(0), "0");
        assert_eq!(format_with_commas(999), "999");
        assert_eq!(format_with_commas(1000), "1,000");
        assert_eq!(format_with_commas(1247), "1,247");
        assert_eq!(format_with_commas(1000000), "1,000,000");
    }

    #[test]
    fn test_calculate_streaks_empty() {
        let (current, longest) = calculate_streaks(&[]);
        assert_eq!(current, 0);
        assert_eq!(longest, 0);
    }

    #[test]
    fn test_calculate_streaks_basic() {
        let days = vec![
            make_day(2025, 1, 1, 0, 0),
            make_day(2025, 1, 2, 3, 2),
            make_day(2025, 1, 3, 5, 3),
            make_day(2025, 1, 4, 1, 1),
            make_day(2025, 1, 5, 0, 0),
            make_day(2025, 1, 6, 2, 1),
            make_day(2025, 1, 7, 4, 2),
        ];

        let (_current, longest) = calculate_streaks(&days);
        assert_eq!(longest, 3); // Jan 2-4
    }

    #[test]
    fn test_level_to_cell() {
        let (ch, _) = level_to_cell(0);
        assert_eq!(ch, '\u{2591}');

        let (ch, _) = level_to_cell(1);
        assert_eq!(ch, '\u{2592}');

        let (ch, _) = level_to_cell(2);
        assert_eq!(ch, '\u{2593}');

        let (ch, _) = level_to_cell(3);
        assert_eq!(ch, '\u{2588}');

        let (ch, _) = level_to_cell(4);
        assert_eq!(ch, '\u{2588}');
    }
}
