// Status of the status bar :)

use std::{collections::HashSet, path::PathBuf, time::Duration};

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Feed {
    pub position: usize,
    pub name: String,
    pub dir: PathBuf,
    // pub is_running: bool,
    pub age_of_output: Option<Duration>,
    pub age_of_log: Option<Duration>,
    pub log_size_bytes: u64,
    pub log: Vec<String>,
    pub pgroup: HashSet<u32>,
}

#[derive(Debug, Clone, Copy)]
pub enum Audience {
    Human,
    Machine,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub enum Status {
    Down,
    UpOff,
    UpOn { feeds: Vec<Feed> },
}

impl Default for Status {
    fn default() -> Self {
        Self::Down
    }
}

impl Status {
    pub fn to_str(&self, audience: Audience) -> String {
        match self {
            Self::Down => "down".to_string(),
            Self::UpOff => "up off".to_string(),
            Self::UpOn { feeds } => {
                let mut table = comfy_table::Table::new();
                table.load_preset(comfy_table::presets::NOTHING); // No borders or dividers.
                table.set_header([
                    "POSITION",
                    "NAME",
                    "DIR",
                    // "RUNNING?",
                    "LAST_OUTPUTTED",
                    "LAST_LOGGED",
                    "LOG_SIZE",
                    "LOG_LINES",
                    "PROC_GROUP_SIZE",
                ]);
                for Feed {
                    position,
                    name,
                    dir,
                    // is_running,
                    age_of_output,
                    age_of_log,
                    log_size_bytes,
                    log,
                    pgroup: pgrp,
                } in feeds.iter()
                {
                    let log_size = match audience {
                        Audience::Human => {
                            bytesize::ByteSize(*log_size_bytes).to_string()
                        }
                        Audience::Machine => log_size_bytes.to_string(),
                    };
                    table.add_row(vec![
                        &position.to_string(),
                        name,
                        dir.to_string_lossy().as_ref(),
                        // is_running.then_some("YES").unwrap_or("NO"),
                        &duration_fmt(*age_of_output, audience),
                        &duration_fmt(*age_of_log, audience),
                        &log_size,
                        &log.len().to_string(),
                        &pgrp.len().to_string(),
                    ]);
                }
                format!("{}", table)
            }
        }
    }
}

fn duration_fmt(duration: Option<Duration>, audience: Audience) -> String {
    // Units smaller than 1 second aren't too human-readable,
    // so dropping them.
    let seconds = duration.map(|d| d.as_secs());
    match (seconds, audience) {
        (None, Audience::Human) => "never".to_string(),
        (None, Audience::Machine) => "-1".to_string(),
        (Some(s), Audience::Human) => {
            humantime::format_duration(Duration::from_secs(s)).to_string()
        }
        (Some(s), Audience::Machine) => s.to_string(),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test() {
        assert_eq!(
            "down",
            super::Status::Down.to_str(super::Audience::Machine)
        );
        assert_eq!(
            "up off",
            super::Status::UpOff.to_str(super::Audience::Machine)
        );
    }
}
