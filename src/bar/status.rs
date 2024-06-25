// Status of the status bar :)

use std::{collections::HashSet, path::PathBuf, time::Duration};

use crate::ps;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Feed {
    pub position: usize,
    pub name: String,
    pub dir: PathBuf,
    pub age_of_output: Option<Duration>,
    pub age_of_log: Option<Duration>,
    pub log_size_bytes: u64,
    pub log_lines: usize,
    pub pid: u32,
    pub state: Option<ps::State>,
    pub pdescendants: HashSet<ps::Proc>,
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
                    "LAST_OUTPUTTED",
                    "LAST_LOGGED",
                    "LOG_SIZE",
                    "LOG_LINES",
                    "PID",
                    "PROC_STATE",
                    "PROC_DESCENDANTS",
                ]);
                for Feed {
                    position,
                    name,
                    dir,
                    age_of_output,
                    age_of_log,
                    log_size_bytes,
                    log_lines,
                    pid,
                    state,
                    pdescendants,
                } in feeds.iter()
                {
                    let pdescendants = if pdescendants.is_empty() {
                        "-".to_string()
                    } else {
                        let mut pdescendants: Vec<&ps::Proc> =
                            pdescendants.iter().collect();
                        pdescendants.sort_by_key(|p| p.pid);
                        pdescendants
                            .iter()
                            .map(|p| {
                                format!("{}:{}", p.pid, p.state.to_str())
                            })
                            .collect::<Vec<String>>()
                            .join(",")
                    };
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
                        &duration_fmt(*age_of_output, audience),
                        &duration_fmt(*age_of_log, audience),
                        &log_size,
                        &log_lines.to_string(),
                        &pid.to_string(),
                        &state
                            .map(|s| s.to_str().to_string())
                            .unwrap_or("-".to_string()),
                        &pdescendants,
                    ]);
                }
                format!("{}", table)
            }
        }
    }
}

fn duration_fmt(duration: Option<Duration>, audience: Audience) -> String {
    match (duration, audience) {
        (None, Audience::Human) => "never".to_string(),
        (None, Audience::Machine) => "-1.00".to_string(),
        (Some(duration), Audience::Human) => {
            // Units smaller than 1 second aren't too human-readable,
            // so dropping them.
            let d = Duration::from_secs(duration.as_secs());
            humantime::format_duration(d).to_string()
        }
        (Some(duration), Audience::Machine) => format!(
            "{seconds:.precision$}",
            precision = 2,
            seconds = duration.as_secs_f64()
        ),
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
