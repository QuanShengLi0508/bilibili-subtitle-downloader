#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod bili;
#[allow(dead_code)]
mod cli;
mod export;
mod external;
mod gui;
mod transcribe;
mod zhihu;

fn main() -> eframe::Result<()> {
    bili::configure_tools_path();
    gui::run()
}
