// Status of the status bar :)

use std::{path::PathBuf, time::Duration};

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Feed {
    pub position: usize,
    pub name: String,
    pub dir: PathBuf,
    // pub is_running: bool,
    pub age_of_output: Option<Duration>,
    pub age_of_log: Option<Duration>,
    pub log_size_bytes: u64,
    pub log_lines: usize,
    pub pid: u32,
    pub pgroup: usize,
    pub pchildren: usize,
    pub pdescendants: usize,
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
                    "PID",
                    "PROC_GROUP_SIZE",
                    "PROC_CHILDREN",
                    "PROC_DESCENDANTS",
                ]);
                for Feed {
                    position,
                    name,
                    dir,
                    // is_running,
                    age_of_output,
                    age_of_log,
                    log_size_bytes,
                    log_lines,
                    pid,
                    pgroup,
                    pchildren,
                    pdescendants,
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
                        &log_lines.to_string(),
                        &pid.to_string(),
                        &pgroup.to_string(),
                        &pchildren.to_string(),
                        &pdescendants.to_string(),
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
