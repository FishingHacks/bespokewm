use tracing::info;
use wm::Wm;

macro_rules! trace_result {
    ($value: expr) => {
        match $value {
            Ok(_) => {},
            Err(e) => {
                error!("{e:?}");
            }
        }
    };
    ($value: expr; $context: expr) => {
        match $value.context($context) {
            Ok(_) => {},
            Err(e) => {
                error!("{e:?}");
            }
        }
    };
}

macro_rules! request_sync {
    ($conn: expr => $request: expr) => {
        $conn.wait_for_reply($conn.send_request(&$request))?
    };
    ($conn: expr => $request: expr; $context: expr) => {
        $conn
            .wait_for_reply($conn.send_request(&$request))
            .context($context)?
    };
}

pub mod drawing;
pub mod actions;
pub mod ewmh;
pub mod atoms;
mod config;
pub mod events;
pub mod keyboard;
pub mod layout;
pub mod screen;
mod wm;

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

    info!("acd");

    let mut wm = Wm::new()?;

    wm.run(actions::ACTIONS)
}
