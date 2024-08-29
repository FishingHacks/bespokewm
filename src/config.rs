use std::path::PathBuf;

static APP_NAME: &str = "wm";

static XDG_HOME: &str = "HOME";
static XDG_CONFIG_HOME: &str = "XDG_CONFIG_HOME";
static XDG_DATA_DIR: &str = "XDG_DATA_HOME";

fn get_data_dir() -> anyhow::Result<PathBuf> {
    match std::env::var(XDG_DATA_DIR).map(PathBuf::from) {
        Ok(mut path) => {
            path.push(APP_NAME);
            if !path.exists() || !path.is_dir() {
                std::fs::create_dir(&path)?;
            }

            return Ok(path);
        }
        Err(_) => (),
    }

    if let Ok(mut path) = std::env::var(XDG_HOME).map(PathBuf::from) {
        path.push(".local");
        path.push("share");
        path.push(APP_NAME);
        if !path.exists() || !path.is_dir() {
            std::fs::create_dir(&path)?;
        }

        return Ok(path);
    }
    
    anyhow::bail!("failed to get the $HOME variable");
}

pub fn get_log_file() -> anyhow::Result<(PathBuf, String)> {
    Ok((get_data_dir()?, format!("{}.log", APP_NAME)))
}

#[derive(Debug, Clone, Copy)]
pub enum Action {
    Quit,
}



pub const GAP_SIZE: u16 = 2;