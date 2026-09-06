//! A worker guard owns only its own descendant tree; dropping inference kills it.
use super::*;
use std::path::Path;

#[cfg(windows)]
pub(super) struct Worker(usize);
#[cfg(windows)]
impl Drop for Worker {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::System::JobObjects::TerminateJobObject(self.0 as _, 1);
            // Termination is asynchronous. Keep ownership while descendants
            // release resources, before the provider releases its inference gate.
            let deadline = std::time::Instant::now() + Duration::from_secs(2);
            loop {
                use windows_sys::Win32::System::JobObjects::*;
                let mut accounting: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = std::mem::zeroed();
                if QueryInformationJobObject(
                    self.0 as _,
                    JobObjectBasicAccountingInformation,
                    &mut accounting as *mut _ as _,
                    std::mem::size_of_val(&accounting) as u32,
                    std::ptr::null_mut(),
                ) == 0
                    || accounting.ActiveProcesses == 0
                    || std::time::Instant::now() >= deadline
                {
                    break;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            windows_sys::Win32::Foundation::CloseHandle(self.0 as _);
        }
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    #[tokio::test]
    #[ignore = "starts an owned Ollama worker; requires KU_OLLAMA_EXE and KU_OLLAMA_MODELS"]
    async fn owned_ollama_worker_drop_closes_server_and_descendants() {
        use windows_sys::Win32::{
            Foundation::{CloseHandle, DuplicateHandle, DUPLICATE_SAME_ACCESS},
            System::{JobObjects::*, Threading::*},
        };
        let exe: std::path::PathBuf = std::env::var_os("KU_OLLAMA_EXE").unwrap().into();
        let models: std::path::PathBuf = std::env::var_os("KU_OLLAMA_MODELS").unwrap().into();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let worker = Worker::start(&exe, &models, port, 8 * 1024u64.pow(3)).unwrap();
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(1))
            .build()
            .unwrap();
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                if client
                    .get(format!("http://127.0.0.1:{port}/api/version"))
                    .send()
                    .await
                    .is_ok_and(|r| r.status().is_success())
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .unwrap();
        unsafe {
            let mut owned_job = std::ptr::null_mut();
            assert_ne!(
                DuplicateHandle(
                    GetCurrentProcess(),
                    worker.0 as _,
                    GetCurrentProcess(),
                    &mut owned_job,
                    0,
                    0,
                    DUPLICATE_SAME_ACCESS
                ),
                0
            );
            drop(worker);
            let mut accounting: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = std::mem::zeroed();
            assert_ne!(
                QueryInformationJobObject(
                    owned_job,
                    JobObjectBasicAccountingInformation,
                    &mut accounting as *mut _ as _,
                    std::mem::size_of_val(&accounting) as u32,
                    std::ptr::null_mut()
                ),
                0
            );
            CloseHandle(owned_job);
            assert_eq!(accounting.ActiveProcesses, 0);
        }
        assert!(client
            .get(format!("http://127.0.0.1:{port}/api/version"))
            .send()
            .await
            .is_err());
    }
}
#[cfg(windows)]
impl Worker {
    pub fn start(exe: &Path, models: &Path, port: u16, memory: u64) -> Result<Self> {
        use std::collections::BTreeMap;
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::{
            Foundation::*,
            System::{JobObjects::*, Threading::*},
        };
        let wide = |s: &std::ffi::OsStr| s.encode_wide().chain(Some(0)).collect::<Vec<_>>();
        let mut env: BTreeMap<String, std::ffi::OsString> = std::env::vars_os()
            .filter_map(|(k, v)| k.to_str().map(|k| (k.to_uppercase(), v)))
            .collect();
        for (k, v) in [
            ("OLLAMA_HOST", format!("127.0.0.1:{port}")),
            ("OLLAMA_NUM_PARALLEL", "1".into()),
            ("OLLAMA_MAX_LOADED_MODELS", "1".into()),
            ("OLLAMA_CONTEXT_LENGTH", "8192".into()),
            ("OLLAMA_KEEP_ALIVE", "0".into()),
            ("OLLAMA_NO_CLOUD", "1".into()),
            ("CUDA_VISIBLE_DEVICES", "-1".into()),
            ("ROCR_VISIBLE_DEVICES", "-1".into()),
            ("GGML_VK_VISIBLE_DEVICES", "-1".into()),
            ("OLLAMA_VULKAN", "0".into()),
            ("OLLAMA_DEBUG", "false".into()),
        ] {
            env.insert(k.into(), v.into());
        }
        env.insert("OLLAMA_MODELS".into(), models.as_os_str().to_owned());
        let mut environment = Vec::<u16>::new();
        for (k, v) in env {
            let mut item = std::ffi::OsString::from(k);
            item.push("=");
            item.push(v);
            environment.extend(wide(&item));
        }
        environment.push(0);
        let application = wide(exe.as_os_str());
        let mut command = wide(std::ffi::OsStr::new(&format!(
            "\"{}\" serve",
            exe.display()
        )));
        unsafe {
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            require(!job.is_null(), "worker_unavailable")?;
            let guard = Self(job as usize);
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
                | JOB_OBJECT_LIMIT_JOB_MEMORY
                | JOB_OBJECT_LIMIT_PROCESS_MEMORY;
            limits.JobMemoryLimit = memory as usize;
            limits.ProcessMemoryLimit = memory as usize;
            require(
                SetInformationJobObject(
                    job,
                    JobObjectExtendedLimitInformation,
                    &limits as *const _ as _,
                    std::mem::size_of_val(&limits) as u32,
                ) != 0,
                "worker_unavailable",
            )?;
            let mut startup: STARTUPINFOW = std::mem::zeroed();
            startup.cb = std::mem::size_of_val(&startup) as u32;
            startup.dwFlags = STARTF_USESTDHANDLES;
            startup.hStdInput = INVALID_HANDLE_VALUE;
            startup.hStdOutput = INVALID_HANDLE_VALUE;
            startup.hStdError = INVALID_HANDLE_VALUE;
            let mut process: PROCESS_INFORMATION = std::mem::zeroed();
            require(
                CreateProcessW(
                    application.as_ptr(),
                    command.as_mut_ptr(),
                    std::ptr::null(),
                    std::ptr::null(),
                    0,
                    CREATE_SUSPENDED | CREATE_NO_WINDOW | CREATE_UNICODE_ENVIRONMENT,
                    environment.as_ptr() as _,
                    std::ptr::null(),
                    &startup,
                    &mut process,
                ) != 0,
                "worker_unavailable",
            )?;
            let assigned = AssignProcessToJobObject(job, process.hProcess) != 0;
            let resumed = assigned && ResumeThread(process.hThread) != u32::MAX;
            if !resumed {
                TerminateProcess(process.hProcess, 1);
            }
            CloseHandle(process.hThread);
            CloseHandle(process.hProcess);
            require(resumed, "worker_unavailable")?;
            Ok(guard)
        }
    }
}
#[cfg(not(windows))]
pub(super) struct Worker;
#[cfg(not(windows))]
impl Worker {
    pub fn start(_: &Path, _: &Path, _: u16, _: u64) -> Result<Self> {
        Err(ExtractionError("worker_unavailable"))
    }
}
