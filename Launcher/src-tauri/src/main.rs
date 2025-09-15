use std::{
    env, fs as stdfs,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

use chrono::Local;
use dotenvy::from_filename;
use std::process::Stdio;
use tauri::{
    api::process::{Command, CommandChild, CommandEvent, Signal},
    AppHandle, Manager,
};
use tokio::{
    fs::{self as tokio_fs, OpenOptions},
    io::AsyncWriteExt,
    sync::Mutex as AsyncMutex,
    time::sleep,
};

#[cfg(windows)]
use windows::{
    Win32::Foundation::{CloseHandle, HANDLE},
    Win32::System::Threading::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation, OpenProcess,
        SetInformationJobObject, TerminateJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, PROCESS_ALL_ACCESS,
    },
};

struct ServerState {
    child: Mutex<Option<CommandChild>>,
    #[cfg(windows)]
    job: Mutex<Option<windows::Win32::Foundation::HANDLE>>,
}

#[tokio::main]
async fn main() {
    tauri::Builder::default()
        .manage(ServerState {
            child: Mutex::new(None),
            #[cfg(windows)]
            job: Mutex::new(None),
        })
        .setup(|app| {
            let app_handle = app.handle();
            let state = app.state::<ServerState>().clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = launch(&app_handle, &state).await {
                    let _ = app_handle.emit_all("log", format!("startup error: {}", e));
                }
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let app = window.app_handle();
                let state = window.state::<ServerState>().clone();
                tauri::async_runtime::spawn(async move {
                    shutdown(&state).await;
                    app.exit(0);
                });
            }
        })
        .run(tauri::generate_context!())
        .expect("error running tauri app");
}

async fn launch(app: &AppHandle, state: &tauri::State<'_, ServerState>) -> Result<(), String> {
    let _ = from_filename("../.env").or_else(|_| from_filename(".env"));

    let silly_dir = PathBuf::from(
        env::var("SILLYTAVERN_DIR").unwrap_or_else(|_| "./vendor/WeylandTavern/SillyTavern".into()),
    );
    if !silly_dir.exists() {
        return Err(format!(
            "SILLYTAVERN_DIR does not exist at {}. Set SILLYTAVERN_DIR in .env",
            silly_dir.display()
        ));
    }

    for bin in ["node", "npm"] {
        if Command::new(bin).arg("--version").status().await.is_err() {
            return Err(format!("{} not found", bin));
        }
    }

    let run_npm = env::var("RUN_NPM_INSTALL").unwrap_or_else(|_| "auto".into());
    let npm_mode = env::var("NPM_MODE").unwrap_or_else(|_| "ci".into());
    if should_npm_install(&run_npm, &silly_dir)? {
        let mut cmd = Command::new("npm");
        cmd.current_dir(&silly_dir);
        if npm_mode == "ci" {
            cmd.arg("ci");
        } else {
            cmd.args(["install", "--omit=dev"]);
        }
        log_line(app, "running npm install").await;
        if !cmd.status().await.map_err(|e| e.to_string())?.success() {
            return Err("npm install failed".into());
        }
    }

    if env::var("RUN_CHARACTER_SYNC").unwrap_or_else(|_| "false".into()) == "true" {
        let url = env::var("CHARACTER_SYNC_URL").unwrap_or_default();
        if !url.is_empty() {
            let mut cmd = Command::new("node");
            cmd.current_dir(&silly_dir);
            cmd.args(["character-downloader.js", &url, "-u"]);
            cmd.env("NODE_ENV", "production");
            cmd.env("NO_BROWSER", "1");
            cmd.env("BROWSER", "none");
            if cmd.status().await.is_err() {
                log_line(app, "character sync failed").await;
            }
        } else {
            log_line(app, "character sync url missing").await;
        }
    }

    let host = env::var("SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".into());
    let port: u16 = env::var("SERVER_PORT")
        .unwrap_or_else(|_| "8000".into())
        .parse()
        .unwrap_or(8000);
    let mut args: Vec<String> = env::var("SERVER_ARGS")
        .unwrap_or_default()
        .split_whitespace()
        .map(|s| s.to_string())
        .collect();
    if !args.iter().any(|a| a == "--no-open") {
        args.push("--no-open".into());
    }

    let logs_dir = PathBuf::from("logs");
    tokio_fs::create_dir_all(&logs_dir)
        .await
        .map_err(|e| e.to_string())?;
    let log_path = logs_dir.join(format!("server-{}.log", Local::now().format("%Y%m%d")));
    let file = Arc::new(AsyncMutex::new(
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .await
            .map_err(|e| e.to_string())?,
    ));

    let mut cmd = Command::new("node");
    cmd.current_dir(&silly_dir);
    cmd.env("NODE_ENV", "production");
    cmd.env("NO_BROWSER", "1");
    cmd.env("BROWSER", "none");
    cmd.args([
        "server.js",
        "--listen",
        "true",
        "--listen-host",
        "0.0.0.0",
        "--listen-port",
        &port.to_string(),
    ]);
    for a in args {
        cmd.arg(a);
    }
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    let (mut rx, mut child) = cmd.spawn().map_err(|e| e.to_string())?;

    #[cfg(windows)]
    unsafe {
        let job = CreateJobObjectW(None, None);
        if job.is_invalid() {
            return Err("CreateJobObjectW failed".into());
        }
        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if !SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const _,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
        .as_bool()
        {
            CloseHandle(job);
            return Err("SetInformationJobObject failed".into());
        }
        let pid = child.pid().ok_or("pid unavailable")? as u32;
        let process = OpenProcess(PROCESS_ALL_ACCESS, false, pid);
        if process.is_invalid() {
            CloseHandle(job);
            return Err("OpenProcess failed".into());
        }
        if !AssignProcessToJobObject(job, process).as_bool() {
            CloseHandle(process);
            CloseHandle(job);
            return Err("AssignProcessToJobObject failed".into());
        }
        CloseHandle(process);
        state.job.lock().unwrap().replace(job);
    }

    state.child.lock().unwrap().replace(child);

    let app_for_logs = app.clone();
    let log_file = file.clone();
    tauri::async_runtime::spawn(async move {
        while let Some(ev) = rx.recv().await {
            if let CommandEvent::Stdout(line) | CommandEvent::Stderr(line) = ev {
                if let Ok(txt) = String::from_utf8(line) {
                    let _ = append_log(&app_for_logs, &log_file, &txt).await;
                }
            }
        }
    });

    let url = format!("http://{}:{}/", host, port);
    if wait_for_health(&url).await {
        app.emit_all("server-ready", url).ok();
    } else {
        log_line(app, "health check failed").await;
    }

    Ok(())
}

async fn append_log(
    app: &AppHandle,
    file: &Arc<AsyncMutex<tokio::fs::File>>,
    line: &str,
) -> Result<(), ()> {
    let mut f = file.lock().await;
    let _ = f.write_all(line.as_bytes()).await;
    let _ = f.write_all(b"\n").await;
    let _ = app.emit_all("log", line.to_string());
    Ok(())
}

async fn log_line(app: &AppHandle, line: &str) {
    let _ = app.emit_all("log", line.to_string());
}

fn should_npm_install(mode: &str, dir: &PathBuf) -> Result<bool, String> {
    if mode == "never" {
        return Ok(false);
    }
    if mode == "always" {
        return Ok(true);
    }
    let node_modules = dir.join("node_modules");
    if !node_modules.exists() {
        return Ok(true);
    }
    let lock_file = dir.join("package-lock.json");
    if lock_file.exists() {
        let lm = stdfs::metadata(&lock_file)
            .map_err(|e| e.to_string())?
            .modified()
            .map_err(|e| e.to_string())?;
        let nm = stdfs::metadata(&node_modules)
            .map_err(|e| e.to_string())?
            .modified()
            .map_err(|e| e.to_string())?;
        return Ok(lm > nm);
    }
    Ok(false)
}

async fn wait_for_health(url: &str) -> bool {
    let client = reqwest::Client::new();
    for i in 0..30u64 {
        if client
            .get(url)
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
        {
            return true;
        }
        sleep(Duration::from_millis(500 + i * 100)).await;
    }
    false
}

async fn shutdown(state: &tauri::State<'_, ServerState>) {
    if let Some(mut child) = state.child.lock().unwrap().take() {
        #[cfg(windows)]
        let _ = child.signal(Signal::CtrlBreak);
        #[cfg(not(windows))]
        let _ = child.signal(Signal::Sigint);
        if tokio::time::timeout(Duration::from_secs(5), child.wait())
            .await
            .is_err()
        {
            #[cfg(windows)]
            {
                if let Some(job) = state.job.lock().unwrap().take() {
                    unsafe {
                        TerminateJobObject(job, 1);
                        CloseHandle(job);
                    }
                }
            }
            #[cfg(not(windows))]
            {
                let _ = child.kill();
            }
        } else {
            #[cfg(windows)]
            if let Some(job) = state.job.lock().unwrap().take() {
                unsafe {
                    CloseHandle(job);
                }
            }
        }
    } else {
        #[cfg(windows)]
        if let Some(job) = state.job.lock().unwrap().take() {
            unsafe {
                CloseHandle(job);
            }
        }
    }
}
