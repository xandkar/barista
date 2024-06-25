use std::collections::{HashMap, HashSet};

use anyhow::{anyhow, bail, Context};

#[derive(Debug, PartialEq)]
pub struct Info {
    pub pid: u32,
    pub ppid: u32,
    pub pgrp: u32,
}

pub async fn list() -> anyhow::Result<Vec<Info>> {
    let out = ps_exec().await?;
    ps_parse(&out)
}

async fn ps_exec() -> anyhow::Result<String> {
    exec("ps", &["-eo", "pid,ppid,pgrp"]).await
}

fn ps_parse(out: &str) -> anyhow::Result<Vec<Info>> {
    let mut list = Vec::new();
    // Skip headers.
    for line in out.lines().skip(1) {
        match line
            .split_whitespace()
            .filter_map(|num| num.parse().ok())
            .collect::<Vec<u32>>()[..]
        {
            [pid, ppid, pgrp] => {
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
    let mut pgroup2pids: HashMap<u32, HashSet<u32>> = HashMap::new();
    for proc in procs {
        pgroup2pids
            .entry(proc.pgrp)
            .and_modify(|group| {
                group.insert(proc.pid);
            })
            .or_insert(HashSet::from([proc.pid]));
    }
    pgroup2pids
}

pub fn children(procs: &[Info]) -> HashMap<u32, HashSet<u32>> {
    let mut parent2children: HashMap<u32, HashSet<u32>> = HashMap::new();
    for proc in procs {
        let parent = proc.ppid;
        let child = proc.pid;
        parent2children
            .entry(parent)
            .and_modify(|children| {
                children.insert(child);
            })
            .or_insert(HashSet::from([child]));
    }
    parent2children
}

pub fn descendants(
    parent2children: &HashMap<u32, HashSet<u32>>,
) -> HashMap<u32, HashSet<u32>> {
    let mut parent2descendants = HashMap::new();
    for parent in parent2children.keys() {
        let mut parent_descendants = HashSet::new();
        collect_descendants(
            parent2children,
            *parent,
            &mut parent_descendants,
        );
        parent2descendants.insert(*parent, parent_descendants);
    }
    parent2descendants
}

pub fn collect_descendants(
    parent2children: &HashMap<u32, HashSet<u32>>,
    parent: u32,
    parent_descendants: &mut HashSet<u32>,
) {
    if let Some(children) = parent2children.get(&parent) {
        for child in children {
            parent_descendants.insert(*child);
            collect_descendants(parent2children, *child, parent_descendants);
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    const OUT_0: &str = "PID PPID PGRP";
    const OUT_1: &str = "  PID  PPID  PGRP
    1     0     1
    2     1     2
    3     1     3
    4     1     4
    5     4     4
";

    #[test]
    fn test_0_1_parse() {
        assert!(ps_parse(OUT_0).unwrap().is_empty());
    }

    #[test]
    fn test_0_2_process_groups() {
        assert!(groups(ps_parse(OUT_0).unwrap().as_slice()).is_empty());
    }

    #[test]
    fn test_0_3_children() {
        assert!(children(ps_parse(OUT_0).unwrap().as_slice()).is_empty());
    }

    #[test]
    fn test_1_1_parse() {
        let out = OUT_1;
        let list_expected = vec![
            Info {
                pid: 1,
                ppid: 0,
                pgrp: 1,
            },
            Info {
                pid: 2,
                ppid: 1,
                pgrp: 2,
            },
            Info {
                pid: 3,
                ppid: 1,
                pgrp: 3,
            },
            Info {
                pid: 4,
                ppid: 1,
                pgrp: 4,
            },
            Info {
                pid: 5,
                ppid: 4,
                pgrp: 4,
            },
        ];
        let list_actual = ps_parse(out).unwrap();
        assert_eq!(list_expected, list_actual);
    }

    #[test]
    fn test_1_2_process_groups() {
        let out = OUT_1;
        let groups_expected = HashMap::from([
            (1, HashSet::from([1])),
            (2, HashSet::from([2])),
            (3, HashSet::from([3])),
            (4, HashSet::from([4, 5])),
        ]);
        let list = ps_parse(out).unwrap();
        let groups_actual = groups(&list[..]);
        assert_eq!(groups_expected, groups_actual);
    }

    #[test]
    fn test_1_3_children() {
        let out = OUT_1;
        let children_expected = HashMap::from([
            (0, HashSet::from([1])),
            (1, HashSet::from([2, 3, 4])),
            (4, HashSet::from([5])),
        ]);
        let list = ps_parse(out).unwrap();
        let children_actual = children(&list[..]);
        assert_eq!(children_expected, children_actual);
    }

    #[test]
    fn test_1_4_descendants() {
        let out = OUT_1;
        let descendants_expected = HashMap::from([
            (0, HashSet::from([1, 2, 3, 4, 5])),
            (1, HashSet::from([2, 3, 4, 5])),
            (4, HashSet::from([5])),
        ]);
        let list = ps_parse(out).unwrap();
        let children = children(&list[..]);
        let descendants_actual = descendants(&children);
        assert_eq!(descendants_expected, descendants_actual);
    }
}
