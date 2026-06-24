use std::time::{Duration, Instant};

const MEASURE_COOLDOWN: Duration = Duration::from_secs(5);
const TRIM_COOLDOWN: Duration = Duration::from_secs(45);
const EMERGENCY_RSS_CEILING_MB: usize = 500;

#[derive(Debug, PartialEq)]
pub enum TrimAction { None, EmergencyCrash }

pub struct OsTrimmer { 
    last_measurement: Option<Instant>,
    last_trim: Option<Instant>,
}

impl OsTrimmer {
    pub fn new() -> Self { Self { last_measurement: None, last_trim: None } }
    pub fn try_trim(&mut self, webview: Option<&wry::WebView>) -> Result<TrimAction, String> {
        let now = Instant::now();
        
        let should_measure = match self.last_measurement {
            Some(last) => now.duration_since(last) >= MEASURE_COOLDOWN,
            None => true,
        };

        let mut action = TrimAction::None;

        if should_measure {
            self.last_measurement = Some(now);
            if let Some(wv) = webview {
                let rss_bytes = get_webview_rss(wv).unwrap_or(0);
                if rss_bytes > (EMERGENCY_RSS_CEILING_MB * 1024 * 1024) { 
                    action = TrimAction::EmergencyCrash; 
                }
            }
        }

        let should_trim = match self.last_trim {
            Some(last) => now.duration_since(last) >= TRIM_COOLDOWN,
            None => true,
        };

        if should_trim {
            self.last_trim = Some(now);
            let _ = execute_host_trim();
            #[cfg(target_os = "windows")]
            if let Some(wv) = webview { let _ = execute_webview_trim_windows(wv); }
        }

        Ok(action)
    }
}

#[cfg(target_os = "windows")]
fn get_webview_rss(webview: &wry::WebView) -> Result<usize, String> {
    use wry::WebViewExtWindows;
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2_8;
    use windows_core::ComInterface;
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_INFORMATION};

    unsafe {
        let controller = webview.controller();
        let core = controller.CoreWebView2().map_err(|e| format!("COM falha: {}", e))?;
        let core_8 = core.cast::<ICoreWebView2_8>().map_err(|e| format!("COM falha: {}", e))?;
        let mut pid: u32 = 0;
        core_8.BrowserProcessId(&mut pid).map_err(|e| format!("BrowserProcessId falha: {}", e))?;
        if pid == 0 { return Err("PID zero".into()); }
        let handle = OpenProcess(PROCESS_QUERY_INFORMATION, 0, pid);
        if handle == 0 { return Err("OpenProcess falhou".into()); }
        let mut counters = std::mem::zeroed::<PROCESS_MEMORY_COUNTERS>();
        let size = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        let success = GetProcessMemoryInfo(handle, &mut counters as *mut _, size);
        CloseHandle(handle);
        if success == 0 { return Err("GetProcessMemoryInfo falhou".into()); }
        Ok(counters.WorkingSetSize)
    }
}

#[cfg(target_os = "linux")]
fn get_webview_rss(_webview: &wry::WebView) -> Result<usize, String> { Ok(0) }

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
fn get_webview_rss(_webview: &wry::WebView) -> Result<usize, String> { Ok(0) }

#[cfg(target_os = "windows")]
fn execute_host_trim() -> Result<(), String> {
    use windows_sys::Win32::System::ProcessStatus::EmptyWorkingSet;
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    unsafe { if EmptyWorkingSet(GetCurrentProcess()) == 0 { return Err("EmptyWorkingSet falhou".into()); } }
    Ok(())
}

#[cfg(target_os = "windows")]
fn execute_webview_trim_windows(webview: &wry::WebView) -> Result<(), String> {
    use wry::WebViewExtWindows;
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2_8;
    use windows_core::ComInterface;
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::ProcessStatus::EmptyWorkingSet;
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_SET_QUOTA};

    unsafe {
        let controller = webview.controller();
        let core = controller.CoreWebView2().map_err(|e| format!("Get CoreWebView2 falhou: {}", e))?;
        let core_8 = core.cast::<ICoreWebView2_8>().map_err(|e| format!("ICoreWebView2_8 indisponível: {}", e))?;
        let mut pid: u32 = 0;
        core_8.BrowserProcessId(&mut pid).map_err(|e| format!("BrowserProcessId falhou: {}", e))?;
        if pid == 0 { return Err("PID 0".into()); }
        let handle = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_SET_QUOTA, 0, pid);
        if handle == 0 { return Err("OpenProcess falhou".into()); }
        let success = EmptyWorkingSet(handle);
        CloseHandle(handle);
        if success == 0 { return Err("EmptyWorkingSet falhou".into()); }
        Ok(())
    }
}

#[cfg(target_os = "linux")]
fn execute_host_trim() -> Result<(), String> { unsafe { libc::malloc_trim(0); } Ok(()) }

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
fn execute_host_trim() -> Result<(), String> { Ok(()) }
