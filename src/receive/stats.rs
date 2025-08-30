use log::error;
use metrics::gauge;
use procfs::process::Process;
use std::{collections::HashMap, io::Read, path::PathBuf};

fn stats_get_task_name(pid: i32, tid: i32) -> std::io::Result<String> {
    let mut path = PathBuf::from("/proc");
    path.push(pid.to_string());
    path.push("task");
    path.push(tid.to_string());
    path.push("comm");

    let mut task_name = String::new();

    let mut file = std::fs::File::open(&path)?;
    file.read_to_string(&mut task_name)?;
    Ok(task_name)
}

fn stats_fill_thread_usage(me: &Process, old: &mut HashMap<String, (u64, u64)>) {
    old.clear();
    let tasks = if let Ok(tasks) = me.tasks() {
        tasks
    } else {
        error!("Impossible to get process thread list");
        return;
    };

    for task in tasks.flatten() {
        if let Ok(stat) = task.stat() {
            if let Ok(task_name) = stats_get_task_name(task.pid, task.tid) {
                if task_name.starts_with("lidi_rx_") {
                    old.insert(
                        task_name,
                        (stat.utime, stat.stime), // user + system time en ticks
                    );
                }
            } else {
                log::warn!("Impossible to get thread name for tid {}", task.tid);
            }
        } else {
            log::warn!("Impossible to get stats for tid {}", task.tid);
        }
    }
}

pub fn stats_thread_usage(prev: &mut HashMap<String, (u64, u64)>, elapsed_secs: f64) {
    let me = if let Ok(proc) = Process::myself() {
        proc
    } else {
        error!("Impossible to get process info");
        return;
    };

    let ticks_per_sec = procfs::ticks_per_second() as f64;

    // snapshot initial
    if prev.is_empty() {
        stats_fill_thread_usage(&me, prev);
        return;
    }

    // snapshot suivant
    let tasks = if let Ok(tasks) = me.tasks() {
        tasks
    } else {
        error!("Impossible to get process thread list");
        return;
    };

    for task in tasks.flatten() {
        if let Ok(stat) = task.stat() {
            if let Ok(task_name) = stats_get_task_name(task.pid, task.tid) {
                if let Some((old_utime, old_stime)) = prev.get(&task_name) {
                    let du = stat.utime.saturating_sub(*old_utime);
                    let ds = stat.stime.saturating_sub(*old_stime);
                    let total_ticks = du + ds;

                    let cpu_usage = (total_ticks as f64 / ticks_per_sec) / elapsed_secs * 100.0;

                    gauge!("cpu_usage", &[("name", task_name)]).set(cpu_usage);
                }
            }
        }
    }

    stats_fill_thread_usage(&me, prev);
}

pub fn stats_proc_snmp() {
    if let Ok(snmp) = procfs::net::snmp() {
        gauge!("snmp_ip_in_discards").set(snmp.ip_in_discards as f64);
        gauge!("snmp_udp_in_errors").set(snmp.udp_in_errors as f64);
    }
}
