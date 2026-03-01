use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::layout::Alignment;
use ratatui::style::{Color, Style};
use ratatui::widgets::Paragraph;
use ratatui::{DefaultTerminal, Frame};
use std::time::Duration;

fn main() -> Result<()> {
    let mut terminal = ratatui::init();
    let result = run(&mut terminal);
    ratatui::restore();
    result
}

fn run(terminal: &mut DefaultTerminal) -> Result<()> {
    loop {
        terminal.draw(render)?;
        if event::poll(Duration::from_millis(250))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                if key.code == KeyCode::Char('q') {
                    break;
                }
            }
        }
    }
    Ok(())
}

fn render(frame: &mut Frame) {
    let area = frame.area();
    let greeting = Paragraph::new("ghd — GitHub Dashboard\n\nPress 'q' to quit")
        .style(Style::default().fg(Color::Cyan))
        .alignment(Alignment::Center);
    frame.render_widget(greeting, area);
}
