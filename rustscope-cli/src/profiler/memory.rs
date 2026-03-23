use anyhow::Result;

pub struct MemoryCollector {
    pid: u32,
}

impl MemoryCollector {
    pub fn new(pid: u32) -> Self {
        Self { pid }
    }

    #[cfg(target_os = "linux")]
    pub fn collect(&self) -> Result<f64> {
        use procfs::process::Process;
        let proc = Process::new(self.pid as i32)?;
        let smaps = proc.smaps()?;
        let mut private_dirty = 0;
        for entry in smaps {
            private_dirty += entry.map.get("Private_Dirty").cloned().unwrap_or(0);
        }
        Ok(private_dirty as f64 / 1024.0) // Convert KB to MB
    }

    #[cfg(target_os = "macos")]
    pub fn collect(&self) -> Result<f64> {
        use libc::{c_int, proc_pidinfo, proc_taskinfo, PROC_PIDTASKINFO};
        use std::mem;

        let mut task_info: proc_taskinfo = unsafe { mem::zeroed() };
        let size = mem::size_of::<proc_taskinfo>() as c_int;
        let res = unsafe {
            proc_pidinfo(self.pid as c_int, PROC_PIDTASKINFO, 0, &mut task_info as *mut _ as *mut _, size)
        };

        if res == size {
            // pti_resident_size is in bytes
            return Ok(task_info.pti_resident_size as f64 / (1024.0 * 1024.0));
        }

        // Fallback to ps if proc_pidinfo fails
        let output = std::process::Command::new("ps")
            .args(["-o", "rss=", "-p", &self.pid.to_string()])
            .output()?;
        
        let rss_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if rss_str.is_empty() {
            anyhow::bail!("Process not found");
        }
        let rss_kb: f64 = rss_str.parse().unwrap_or(0.0);
        
        Ok(rss_kb / 1024.0) // Convert KB to MB
    }
}
