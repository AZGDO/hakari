#![allow(dead_code, unused_imports)]

mod app;
mod config;
mod llm;
mod memory;
mod nano;
mod project;
mod shizuka;
mod tools;
mod tui;

use app::App;
use clap::Parser;
use config::{CliArgs, HakariConfig};
use std::time::Duration;
use tui::event;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = CliArgs::parse();

    // Setup logging
    if args.debug {
        tracing_subscriber::fmt()
            .with_env_filter("hakari=debug")
            .with_writer(std::io::stderr)
            .init();
    }

    let project_dir = args.project_dir
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

    let config = HakariConfig::load(args.config.as_ref())?;

    let mut terminal = tui::terminal::init()?;

    let mut app = App::new(project_dir, config);

    // Main event loop
    while app.running {
        terminal.draw(|frame| {
            app.render(frame);
        })?;

        let timeout = if app.agent_running {
            Duration::from_millis(50) // Fast ticks during agent execution for smooth streaming
        } else {
            Duration::from_millis(100) // Normal tick rate
        };

        if let Some(event) = event::poll_event(timeout)? {
            app.handle_event(event);
        }
    }

    tui::terminal::restore(&mut terminal)?;

    Ok(())
}
