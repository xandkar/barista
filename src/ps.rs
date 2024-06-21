use std::{
    collections::{HashMap, HashSet},
    num::ParseIntError,
};

use anyhow::{anyhow, bail, Context};

#[derive(Debug)]
pub struct Info {
    pub pid: u32,
    pub ppid: u32,
    pub pgrp: u32,
}

pub async fn list() -> anyhow::Result<Vec<Info>> {
    let out = exec("ps", &["-eo", "pid,ppid,pgrp"]).await?;
    let mut list = Vec::new();
    // Skip headers.
    for line in out.lines().skip(1) {
        match line
            .split_whitespace()
            .map(|num| num.parse())
            .collect::<Vec<Result<u32, ParseIntError>>>()[..]
        {
            [Ok(pid), Ok(ppid), Ok(pgrp)] => {
                let info = Info { pid, ppid, pgrp };
                list.push(info);
            }
            _ => {
                bail!("Invalid ps output line: {:?}", line);
            }
        }
    }
    Ok(list)
}

pub fn groups(procs: &[Info]) -> HashMap<u32, HashSet<u32>> {
    let mut groups: HashMap<u32, HashSet<u32>> = HashMap::new();
    for proc in procs {
        groups
            .entry(proc.pgrp)
            .and_modify(|group| {
                group.insert(proc.pid);
            })
            .or_insert(HashSet::from([proc.pid]));
    }
    groups
}

async fn exec(cmd: &str, args: &[&str]) -> anyhow::Result<String> {
    use std::process::Output;

    let Output {
        status,
        stdout,
        stderr,
    } = tokio::process::Command::new(cmd)
        .args(args)
        .output()
        .await
        .context(format!("Failed to spawn cmd={:?}, args={:?}", cmd, args))?;
    status
        .success()
        .then_some(String::from_utf8(stdout)?)
        .ok_or_else(|| {
            anyhow!(
                "Failed to run cmd={:?}, args={:?}. Code: {}. Stderr: {:?}",
                cmd,
                args,
                status.code().map_or("none".to_string(), |n| n.to_string()),
                String::from_utf8_lossy(stderr.as_slice())
            )
        })
}
