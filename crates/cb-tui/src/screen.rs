//! 终端屏幕：alternate screen + raw mode + 恢复。
//!
//! 「fresh and clean」在这里落地：进入后只画这一屏，退出（含 panic）恢复原样。

use crossterm::cursor::{Hide, Show};
use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io::{stdout, Stdout};

pub struct Screen {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    mouse: bool,
}

impl Screen {
    pub fn enter(mouse: bool) -> anyhow::Result<Screen> {
        enable_raw_mode()?;
        let mut out = stdout();
        execute!(out, EnterAlternateScreen, Hide)?;
        if mouse {
            execute!(out, EnableMouseCapture)?;
        }
        install_panic_hook();
        let terminal = Terminal::new(CrosstermBackend::new(out))?;
        Ok(Screen { terminal, mouse })
    }

    pub fn draw(&mut self, app: &crate::App) -> anyhow::Result<()> {
        self.terminal.draw(|frame| crate::views::draw(frame, app))?;
        Ok(())
    }

    pub fn size(&self) -> (u16, u16) {
        self.terminal
            .size()
            .map(|r| (r.width, r.height))
            .unwrap_or((99, 30))
    }
}

impl Drop for Screen {
    fn drop(&mut self) {
        restore(self.mouse);
    }
}

fn restore(mouse: bool) {
    let _ = disable_raw_mode();
    let mut out = stdout();
    let _ = execute!(out, Show);
    if mouse {
        let _ = execute!(out, DisableMouseCapture);
    }
    let _ = execute!(out, LeaveAlternateScreen);
}

fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore(true);
        previous(info);
    }));
}
