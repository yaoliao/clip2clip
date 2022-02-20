use anyhow::Result;
use clipboard_master::Master;
use log::{error, info};

use crate::clipboard::ClipboardListener;
use crate::transport::{ClipHandler, Msg};

mod clipboard;
mod transport;

fn main() -> Result<()> {
    log_setting();

    info!("starting clip2clip !!!!!!!!!!!!!!!");

    let (tx, rx) = tokio::sync::mpsc::channel::<Msg>(100);
    let mut handler = ClipHandler::new(rx);

    std::thread::spawn(move || {
        if let Err(e) = handler.start_gossip() {
            error!("run clip error: {:?} ", e);
        }
    });

    info!("starting ClipboardListener !!!!!!!!!!!!!!!");
    let listener = ClipboardListener::new(tx);
    Master::new(listener).run()?;

    Ok(())
}

pub fn log_setting() {
    use chrono::Local;
    use log::LevelFilter;
    use std::io::Write;

    // log
    let env = env_logger::Env::default()
        .filter_or(env_logger::DEFAULT_FILTER_ENV, LevelFilter::Info.as_str());
    env_logger::Builder::from_env(env)
        .format(|buf, record| {
            writeln!(
                buf,
                "{} {} [{}:{}] {}",
                Local::now().format("%Y-%m-%d %H:%M:%S"),
                record.level(),
                record.module_path().unwrap_or("<unnamed>"),
                record.line().unwrap_or(0),
                &record.args()
            )
        })
        .init();
}
