//! Session process supervision and PSS diagnostics.
//!
//! The supervisor spawns a `CommandSpec` from `gamehubx-runtime` in its own
//! process group so the whole PRoot/Wine tree can be terminated on exit, and
//! samples PSS from `/proc` for the diagnostics screen.

use std::fs;
use std::io;
use std::process::{Child, Command, ExitStatus};

use gamehubx_runtime::CommandSpec;

pub struct Supervisor;

impl Supervisor {
    pub fn spawn(spec: &CommandSpec) -> io::Result<RunningSession> {
        let mut command = Command::new(&spec.program);
        command.args(&spec.args);
        for (key, value) in &spec.env {
            command.env(key, value);
        }
        if !spec.cwd.as_os_str().is_empty() {
            command.current_dir(&spec.cwd);
        }
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
            let memory = spec.memory_limit_bytes;
            let affinity = spec.cpu_affinity;
            if memory.is_some() || affinity.is_some() {
                unsafe {
                    command.pre_exec(move || {
                        if let Some(bytes) = memory {
                            let limit = bytes as libc::rlim_t;
                            let rlimit = libc::rlimit {
                                rlim_cur: limit,
                                rlim_max: limit,
                            };
                            if libc::setrlimit(libc::RLIMIT_AS, &rlimit) != 0 {
                                return Err(io::Error::last_os_error());
                            }
                        }
                        if let Some(mask) = affinity {
                            let mut set: libc::cpu_set_t = std::mem::zeroed();
                            libc::CPU_ZERO(&mut set);
                            for cpu in 0..64usize {
                                if mask & (1u64 << cpu) != 0 {
                                    libc::CPU_SET(cpu, &mut set);
                                }
                            }
                            if libc::sched_setaffinity(
                                0,
                                std::mem::size_of::<libc::cpu_set_t>(),
                                &set,
                            ) != 0
                            {
                                return Err(io::Error::last_os_error());
                            }
                        }
                        Ok(())
                    });
                }
            }
        }
        Ok(RunningSession {
            child: command.spawn()?,
        })
    }
}

pub struct RunningSession {
    child: Child,
}

impl RunningSession {
    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        self.child.try_wait()
    }

    pub fn wait(&mut self) -> io::Result<ExitStatus> {
        self.child.wait()
    }

    /// Kills the whole process group and reaps it. Safe to call after exit.
    pub fn terminate(&mut self) -> io::Result<()> {
        if self.child.try_wait()?.is_some() {
            return Ok(());
        }
        #[cfg(unix)]
        unsafe {
            libc::kill(-(self.child.id() as i32), libc::SIGKILL);
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        Ok(())
    }
}

impl Drop for RunningSession {
    fn drop(&mut self) {
        let _ = self.terminate();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PssSample {
    pub total_kb: u64,
    pub anon_kb: u64,
    pub file_kb: u64,
    pub shmem_kb: u64,
}

pub fn read_pss(pid: u32) -> Option<PssSample> {
    if let Ok(text) = fs::read_to_string(format!("/proc/{pid}/smaps_rollup")) {
        if let Some(sample) = parse_smaps(&text) {
            return Some(sample);
        }
    }
    let text = fs::read_to_string(format!("/proc/{pid}/smaps")).ok()?;
    parse_smaps(&text)
}

pub fn parse_smaps(text: &str) -> Option<PssSample> {
    let mut total_kb = 0;
    let mut anon_kb = 0;
    let mut file_kb = 0;
    let mut shmem_kb = 0;
    let mut found = false;

    for line in text.lines() {
        let Some((field, rest)) = line.trim_start().split_once(':') else {
            continue;
        };
        let field = field.trim();
        if !matches!(field, "Pss" | "Pss_Anon" | "Pss_File" | "Pss_Shmem") {
            continue;
        }
        let Some(value) = rest
            .split_whitespace()
            .next()
            .and_then(|value| value.parse::<u64>().ok())
        else {
            continue;
        };
        match field {
            "Pss" => {
                total_kb += value;
                found = true;
            }
            "Pss_Anon" => anon_kb += value,
            "Pss_File" => file_kb += value,
            "Pss_Shmem" => shmem_kb += value,
            _ => {}
        }
    }

    if found && total_kb > 0 {
        Some(PssSample {
            total_kb,
            anon_kb,
            file_kb,
            shmem_kb,
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn spec(program: &str, args: &[&str]) -> CommandSpec {
        CommandSpec {
            program: PathBuf::from(program),
            args: args.iter().map(|arg| arg.to_string()).collect(),
            env: Vec::new(),
            cwd: PathBuf::new(),
            memory_limit_bytes: None,
            cpu_affinity: None,
        }
    }

    #[cfg(unix)]
    #[test]
    fn reports_child_exit_code() {
        let mut session = Supervisor::spawn(&spec("sh", &["-c", "exit 7"])).unwrap();
        let status = session.wait().unwrap();
        assert_eq!(status.code(), Some(7));
    }

    #[cfg(unix)]
    #[test]
    fn terminates_a_running_child() {
        let mut session = Supervisor::spawn(&spec("sh", &["-c", "sleep 30"])).unwrap();
        session.terminate().unwrap();
        let status = session.wait().unwrap();
        assert!(!status.success());
    }

    #[cfg(any(target_os = "linux", target_os = "android"))]
    #[test]
    fn reads_own_pss() {
        let sample = read_pss(std::process::id()).expect("PSS for self");
        assert!(sample.total_kb > 0);
    }

    #[test]
    fn parses_rollup_fields() {
        let text = "Pss: 100 kB\nPss_Anon: 40 kB\nPss_File: 50 kB\nPss_Shmem: 10 kB\n";
        assert_eq!(
            parse_smaps(text),
            Some(PssSample {
                total_kb: 100,
                anon_kb: 40,
                file_kb: 50,
                shmem_kb: 10
            })
        );
    }

    #[test]
    fn sums_repeated_pss_from_smaps() {
        let text = "Name: a\nPss: 10 kB\nName: b\nPss: 5 kB\n";
        assert_eq!(
            parse_smaps(text),
            Some(PssSample {
                total_kb: 15,
                anon_kb: 0,
                file_kb: 0,
                shmem_kb: 0
            })
        );
    }
}
