use anyhow::Result;

pub struct ThreadFdCollector {
    pid: u32,
}

impl ThreadFdCollector {
    pub fn new(pid: u32) -> Self {
        Self { pid }
    }

    #[cfg(target_os = "linux")]
    pub fn collect_threads(&self) -> Result<u32> {
        use procfs::process::Process;
        let proc = Process::new(self.pid as i32)?;
        let count = proc.tasks()?.count();
        Ok(count as u32)
    }

    #[cfg(target_os = "linux")]
    pub fn collect_fds(&self) -> Result<u32> {
        use procfs::process::Process;
        let proc = Process::new(self.pid as i32)?;
        let count = proc.fd()?.count();
        Ok(count as u32)
    }

    #[cfg(target_os = "macos")]
    pub fn collect_threads(&self) -> Result<u32> {
        // macOS thread count via proc_pidinfo
        use libc::{c_int, proc_pidinfo, proc_taskinfo, PROC_PIDTASKINFO};
        use std::mem;

        let mut task_info: proc_taskinfo = unsafe { mem::zeroed() };
        let size = mem::size_of::<proc_taskinfo>() as c_int;
        let res = unsafe {
            proc_pidinfo(self.pid as c_int, PROC_PIDTASKINFO, 0, &mut task_info as *mut _ as *mut _, size)
        };

        if res != size {
            anyhow::bail!("Process not found");
        }

        Ok(task_info.pti_threadnum as u32)
    }

    #[cfg(target_os = "macos")]
    pub fn collect_fds(&self) -> Result<u32> {
        // macOS FD count via proc_pidinfo with PROC_PIDLISTFDS
        use libc::{c_int, proc_pidinfo, PROC_PIDLISTFDS};
        use std::mem;

        // Passing 0 as the buffer gives us the number of bytes required
        let res = unsafe {
            proc_pidinfo(self.pid as c_int, PROC_PIDLISTFDS, 0, std::ptr::null_mut(), 0)
        };

        if res < 0 {
            anyhow::bail!("Process not found or access denied");
        }

        // Each FD entry is proc_fdinfo size
        use libc::proc_fdinfo;
        let entry_size = mem::size_of::<proc_fdinfo>() as c_int;
        Ok((res / entry_size) as u32)
    }
}
