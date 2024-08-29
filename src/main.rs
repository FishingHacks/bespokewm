use wm::Wm;

// #[macro_export]
macro_rules! request_sync {
    ($conn: expr => $request: expr) => {
        $conn.wait_for_reply($conn.send_request(&$request))?
    };
    ($conn: expr => $request: expr; $context: expr) => {
        $conn.wait_for_reply($conn.send_request(&$request)).context($context)?
    };
}

pub mod keyboard;
pub mod layout;
pub mod atoms;
mod wm;
mod config;

fn main() -> anyhow::Result<()> {
    let (dir, log_file) = config::get_log_file()?;
    let writer = tracing_appender::rolling::daily(dir, log_file);
    let (non_blocking, _guard) = tracing_appender::non_blocking(writer);
    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .with_writer(non_blocking)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("Setting the subscriber failed");

    let mut wm = Wm::new()?;

    wm.run()
}
