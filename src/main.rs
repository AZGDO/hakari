#![allow(dead_code, unused_imports)]

mod app;
mod auth;
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

    if args.debug {
        tracing_subscriber::fmt()
            .with_env_filter("hakari=debug")
            .with_writer(std::io::stderr)
            .init();
    }

    let project_dir = args
        .project_dir
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

    let config = HakariConfig::load(args.config.as_ref())?;

    let mut terminal = tui::terminal::init()?;

    let mut app = App::new(project_dir, config);

    while app.running {
        terminal.draw(|frame| {
            app.render(frame);
        })?;

        let timeout = if app.agent_running || app.connect_polling {
            Duration::from_millis(50)
        } else {
            Duration::from_millis(80)
        };

        if let Some(evt) = event::poll_event(timeout)? {
            app.handle_event(evt);
        }
    }

    tui::terminal::restore(&mut terminal)?;

    if app.reinstall_pending {
        // Determine source directory: walk up from current executable or use cwd
        let src_dir = locate_hakari_src();
        println!("Reinstalling hakari from {}...", src_dir.display());
        let status = std::process::Command::new("cargo")
            .arg("install")
            .arg("--path")
            .arg(&src_dir)
            .arg("--force")
            .status();
        match status {
            Ok(s) if s.success() => println!("hakari reinstalled successfully."),
            Ok(s) => eprintln!("cargo install exited with: {}", s),
            Err(e) => eprintln!("Failed to run cargo install: {}", e),
        }
    }

    Ok(())
}

fn locate_hakari_src() -> std::path::PathBuf {
    // Check if current exe is inside a cargo target dir — walk up to find Cargo.toml
    if let Ok(exe) = std::env::current_exe() {
        let mut dir = exe.as_path();
        while let Some(parent) = dir.parent() {
            if parent.join("Cargo.toml").exists() {
                return parent.to_path_buf();
            }
            dir = parent;
        }
    }
    // Fall back to cwd
    std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
}
