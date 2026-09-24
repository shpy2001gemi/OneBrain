//! Native lifecycle signals only withdraw execution; they never enable a lane.
#[cfg(windows)]
mod windows {
    use crate::supervisor::Supervisor;
    use std::{ffi::c_void, sync::Arc};
    use tauri::Emitter;
    struct Context {
        supervisor: Arc<Supervisor>,
        app: tauri::AppHandle,
    }
    use windows_sys::Win32::{
        Foundation::HANDLE,
        NetworkManagement::IpHelper::{
            CancelMibChangeNotify2, NotifyIpInterfaceChange, MIB_IPINTERFACE_ROW,
            MIB_NOTIFICATION_TYPE,
        },
        Networking::WinSock::AF_UNSPEC,
        System::Power::{
            PowerRegisterSuspendResumeNotification, PowerUnregisterSuspendResumeNotification,
            DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS,
        },
    };
    pub struct NativeEvents {
        power: usize,
        network: usize,
        context: Box<Context>,
    }
    unsafe extern "system" fn power(
        context: *const c_void,
        _kind: u32,
        _setting: *const c_void,
    ) -> u32 {
        signal(context);
        0
    }
    unsafe extern "system" fn network(
        context: *const c_void,
        _row: *const MIB_IPINTERFACE_ROW,
        _kind: MIB_NOTIFICATION_TYPE,
    ) {
        signal(context);
    }
    unsafe fn signal(context: *const c_void) {
        let context = &*(context as *const Context);
        if !context.supervisor.fence() {
            return;
        }
        let _ = context.app.emit("desktop-lifecycle", ());
        let supervisor = context.supervisor.clone();
        let app = context.app.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(reason) = supervisor.shutdown().await {
                let _ = tauri::Manager::state::<crate::state::AppState>(&app)
                    .shutdown_issue
                    .set(reason);
                tracing::error!(reason);
                let _ = app.emit("desktop-lifecycle", ());
            }
        });
    }
    impl NativeEvents {
        pub fn register(
            supervisor: Arc<Supervisor>,
            app: tauri::AppHandle,
        ) -> Result<Self, &'static str> {
            let context = Box::new(Context { supervisor, app });
            let ptr = &*context as *const Context as *const c_void;
            let mut parameters = DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS {
                Callback: Some(power),
                Context: ptr as *mut c_void,
            };
            let mut power_handle = std::ptr::null_mut();
            let mut network_handle = std::ptr::null_mut();
            unsafe {
                // DEVICE_NOTIFY_CALLBACK = 2; callbacks do no OS blocking work.
                if PowerRegisterSuspendResumeNotification(
                    2,
                    &mut parameters as *mut _ as HANDLE,
                    &mut power_handle,
                ) != 0
                {
                    return Err("desktop_power_notifications_unavailable");
                }
                if NotifyIpInterfaceChange(
                    AF_UNSPEC,
                    Some(network),
                    ptr,
                    false,
                    &mut network_handle,
                ) != 0
                {
                    PowerUnregisterSuspendResumeNotification(power_handle as isize);
                    return Err("desktop_network_notifications_unavailable");
                }
            }
            Ok(Self {
                power: power_handle as usize,
                network: network_handle as usize,
                context,
            })
        }
    }
    impl Drop for NativeEvents {
        fn drop(&mut self) {
            unsafe {
                PowerUnregisterSuspendResumeNotification(self.power as _);
                CancelMibChangeNotify2(self.network as _);
            }
            let _ = &self.context;
        }
    }
}
#[cfg(windows)]
pub use windows::NativeEvents;

#[cfg(not(windows))]
pub struct NativeEvents;
#[cfg(not(windows))]
impl NativeEvents {
    pub fn register(
        _supervisor: std::sync::Arc<crate::supervisor::Supervisor>,
        _app: tauri::AppHandle,
    ) -> Result<Self, &'static str> {
        Err("desktop_native_lifecycle_adapter_unavailable")
    }
}
