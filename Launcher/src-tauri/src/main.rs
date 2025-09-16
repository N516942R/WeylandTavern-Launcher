use std::{
    env,
    ffi::{OsStr, OsString},
    fs as stdfs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

use chrono::Local;
use dotenvy::from_filename;
use serde::Serialize;
use std::process::Stdio;
use tauri::{AppHandle, Emitter, Manager};
use tokio::{
    fs::{self as tokio_fs, OpenOptions},
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child as TokioChild, Command as TokioCommand},
    sync::Mutex as AsyncMutex,
    time::sleep,
};

#[cfg(windows)]
use windows::{
    core::PCWSTR,
    Win32::Foundation::{CloseHandle, HANDLE},
    Win32::System::{
        JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
            SetInformationJobObject, TerminateJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        },
        Threading::{OpenProcess, PROCESS_ALL_ACCESS},
    },
};

#[cfg(windows)]
struct JobHandle(HANDLE);

#[cfg(windows)]
impl JobHandle {
    fn new(handle: HANDLE) -> Self {
        Self(handle)
    }

    fn raw(&self) -> HANDLE {
        self.0
    }
}

#[cfg(windows)]
impl Drop for JobHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

#[cfg(windows)]
unsafe impl Send for JobHandle {}

#[cfg(windows)]
unsafe impl Sync for JobHandle {}

struct ServerState {
    child: Mutex<Option<TokioChild>>,
    #[cfg(windows)]
    job: Mutex<Option<JobHandle>>,
}

#[cfg(windows)]
const NPM_CANDIDATES: &[&str] = &["npm.cmd", "npm"];

#[cfg(not(windows))]
const NPM_CANDIDATES: &[&str] = &["npm"];

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
enum UpdateStatus {
    Success,
    UpToDate,
    NeedRetry,
    Failed,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateResponse {
    status: UpdateStatus,
    message: String,
    log_path: Option<String>,
    diff: Option<String>,
    stash_used: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CharacterResponse {
    success: bool,
    message: String,
}

enum NpmTool {
    Binary(OsString),
    Script(PathBuf),
}

impl NpmTool {
    fn into_command(self) -> TokioCommand {
        match self {
            Self::Binary(bin) => TokioCommand::new(bin),
            Self::Script(path) => {
                let mut cmd = TokioCommand::new("node");
                cmd.arg(path.as_os_str());
                cmd
            }
        }
    }
}

#[tokio::main]
async fn main() {
    tauri::Builder::default()
        .manage(ServerState {
            child: Mutex::new(None),
            #[cfg(windows)]
            job: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![
            update_vendor,
            finalize_stash,
            run_character_sync,
            start_server
        ])
        .setup(|_| {
            load_env();
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let app = window.app_handle().clone();
                tauri::async_runtime::spawn(async move {
                    let state = app.state::<ServerState>();
                    shutdown(state).await;
                    app.exit(0);
                });
            }
        })
        .run(tauri::generate_context!())
        .expect("error running tauri app");
}

fn load_env() {
    let _ = from_filename("../.env").or_else(|_| from_filename(".env"));
}

fn allow_git_pull_in_app() -> bool {
    let raw = env::var("ALLOW_GIT_PULL_IN_APP").unwrap_or_default();
    matches!(
        raw.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

fn silly_dir() -> Result<PathBuf, String> {
    let path =
        env::var("SILLYTAVERN_DIR").unwrap_or_else(|_| "./vendor/WeylandTavern/SillyTavern".into());
    let path = PathBuf::from(path);
    if path.exists() {
        Ok(path)
    } else {
        Err(format!(
            "SILLYTAVERN_DIR does not exist at {}. Set SILLYTAVERN_DIR in .env",
            path.display()
        ))
    }
}

fn vendor_dir() -> Result<PathBuf, String> {
    let silly = silly_dir()?;
    silly
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "Unable to determine vendor directory".to_string())
}

async fn run_git(dir: &Path, args: &[&str]) -> Result<std::process::Output, String> {
    TokioCommand::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .await
        .map_err(|e| e.to_string())
}

async fn write_update_log(log_path: &Path, pull: &str, diff: &str) -> Result<(), String> {
    let mut file = tokio_fs::File::create(log_path)
        .await
        .map_err(|e| e.to_string())?;
    let mut contents = String::from("git pull output:\n");
    let trimmed_pull = pull.trim();
    if trimmed_pull.is_empty() {
        contents.push_str("(no output)");
    } else {
        contents.push_str(trimmed_pull);
    }
    contents.push_str("\n\nGit diff --compact-summary:\n");
    if diff.trim().is_empty() {
        contents.push_str("No differences.\n");
    } else {
        contents.push_str(diff.trim());
        contents.push('\n');
    }
    file.write_all(contents.as_bytes())
        .await
        .map_err(|e| e.to_string())?;
    file.flush().await.map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn update_vendor(app: AppHandle, attempt_overwrite: bool) -> Result<UpdateResponse, String> {
    load_env();
    let silly = silly_dir()?;
    let repo = vendor_dir()?;
    let log_path = silly.join("WTUpdate.log");

    if !allow_git_pull_in_app() {
        let script_hint = env::var("UPDATE_SCRIPT")
            .ok()
            .filter(|value| !value.trim().is_empty());
        let mut message =
            String::from("Skipping vendor update: in-app git pull is disabled by policy.");
        if let Some(script) = script_hint {
            message.push(' ');
            message.push_str(&format!("Use {} to update WeylandTavern manually.", script));
        }
        log_line(&app, &message).await;
        return Ok(UpdateResponse {
            status: UpdateStatus::UpToDate,
            message,
            log_path: None,
            diff: None,
            stash_used: false,
        });
    }

    let mut stash_used = false;

    if attempt_overwrite {
        log_line(&app, "Stashing local changes before retrying update...").await;
        let output = run_git(&repo, &["stash"]).await?;
        if !output.status.success() {
            let details = format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return Err(if details.trim().is_empty() {
                "git stash failed".into()
            } else {
                format!("git stash failed: {}", details.trim())
            });
        }
        stash_used = true;
    } else {
        log_line(&app, "Attempting to update WeylandTavern...").await;
    }

    let pull_output = run_git(&repo, &["pull"]).await?;
    let pull_text = format!(
        "{}{}",
        String::from_utf8_lossy(&pull_output.stdout),
        String::from_utf8_lossy(&pull_output.stderr)
    );

    if pull_output.status.success() {
        let lower = pull_text.to_lowercase();
        let (status, message) = if lower.contains("already up to date") {
            (
                UpdateStatus::UpToDate,
                "WeylandTavern is up to date!".to_string(),
            )
        } else {
            (
                UpdateStatus::Success,
                "WeylandTavern updated successfully.".to_string(),
            )
        };
        log_line(&app, &message).await;
        return Ok(UpdateResponse {
            status,
            message,
            log_path: None,
            diff: None,
            stash_used,
        });
    }

    log_line(&app, "There was an error updating WeylandTavern...").await;
    log_line(&app, "Generating log file SillyTavern/WTUpdate.log...").await;

    let diff_output = run_git(&repo, &["diff", "--compact-summary"]).await?;
    let diff_text = format!(
        "{}{}",
        String::from_utf8_lossy(&diff_output.stdout),
        String::from_utf8_lossy(&diff_output.stderr)
    );

    write_update_log(&log_path, &pull_text, &diff_text).await?;

    let combined = {
        let mut combined = pull_text.trim().to_string();
        if !diff_text.trim().is_empty() {
            if !combined.is_empty() {
                combined.push_str("\n\n");
            }
            combined.push_str(diff_text.trim());
        }
        combined
    };

    let response = UpdateResponse {
        status: if attempt_overwrite {
            UpdateStatus::Failed
        } else {
            UpdateStatus::NeedRetry
        },
        message: if attempt_overwrite {
            "Update failed even after stashing local changes.".to_string()
        } else {
            "There was an error updating WeylandTavern.".to_string()
        },
        log_path: Some(log_path.to_string_lossy().into_owned()),
        diff: if combined.is_empty() {
            None
        } else {
            Some(combined)
        },
        stash_used,
    };

    Ok(response)
}

#[tauri::command]
async fn finalize_stash(app: AppHandle, revert: bool) -> Result<(), String> {
    load_env();
    let repo = vendor_dir()?;
    let args: [&str; 2] = if revert {
        ["stash", "pop"]
    } else {
        ["stash", "clear"]
    };
    if revert {
        log_line(&app, "Reverting differing files post update...").await;
    } else {
        log_line(&app, "Discarding stashed changes...").await;
    }
    let output = run_git(&repo, &args).await?;
    if !output.status.success() {
        let details = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return Err(if details.trim().is_empty() {
            "Failed to finalize stash".into()
        } else {
            details.trim().to_string()
        });
    }
    Ok(())
}

#[tauri::command]
async fn run_character_sync(app: AppHandle) -> Result<CharacterResponse, String> {
    load_env();
    let silly = silly_dir()?;
    let url = env::var("CHARACTER_SYNC_URL")
        .unwrap_or_else(|_| "https://mega.nz/folder/J5ARwZRI#2hnLHnLjXXNk3GGve7fjlw".into());

    if url.trim().is_empty() {
        return Ok(CharacterResponse {
            success: false,
            message: "Character sync URL is not configured.".into(),
        });
    }

    log_line(&app, "Checking for character updates...").await;
    let mut cmd = TokioCommand::new("node");
    cmd.current_dir(&silly);
    cmd.env("NODE_ENV", "production");
    cmd.env("NO_BROWSER", "1");
    cmd.env("BROWSER", "none");
    cmd.args(["character-downloader.js", &url, "-u"]);

    let output = cmd.output().await.map_err(|e| e.to_string())?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if output.status.success() {
        if !stdout.trim().is_empty() {
            log_line(&app, stdout.trim()).await;
        }
        Ok(CharacterResponse {
            success: true,
            message: "Character update completed.".into(),
        })
    } else {
        let combined = format!("{}{}", stdout, stderr);
        if !combined.trim().is_empty() {
            log_line(&app, combined.trim()).await;
        }
        Ok(CharacterResponse {
            success: false,
            message: "Character update failed. Check logs for details.".into(),
        })
    }
}

#[tauri::command]
async fn start_server(app: AppHandle, state: tauri::State<'_, ServerState>) -> Result<(), String> {
    launch(&app, state).await
}

async fn command_exists(program: &OsStr) -> bool {
    TokioCommand::new(program.to_os_string())
        .arg("--version")
        .status()
        .await
        .map(|status| status.success())
        .unwrap_or(false)
}

async fn locate_npm(app: &AppHandle) -> Result<NpmTool, String> {
    if let Some(custom) = env::var_os("NPM_BIN").filter(|value| !value.is_empty()) {
        if command_exists(custom.as_os_str()).await {
            let location = PathBuf::from(&custom);
            log_line(
                app,
                &format!(
                    "Using npm from {} as configured via NPM_BIN.",
                    location.display()
                ),
            )
            .await;
            return Ok(NpmTool::Binary(custom));
        } else {
            let location = PathBuf::from(&custom);
            return Err(format!(
                "Configured NPM_BIN at {} is not executable. Install npm or update NPM_BIN.",
                location.display()
            ));
        }
    }

    for candidate in NPM_CANDIDATES {
        if command_exists(OsStr::new(candidate)).await {
            return Ok(NpmTool::Binary(OsString::from(candidate)));
        }
    }

    log_line(
        app,
        "npm executable not found on PATH; attempting to use the npm-cli.js bundled with Node.",
    )
    .await;

    let output = TokioCommand::new("node")
        .args(["-p", "require.resolve('npm/bin/npm-cli.js')"])
        .output()
        .await
        .map_err(|e| format!("Unable to locate npm via node: {e}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let script = stdout.trim();

    if output.status.success() && !script.is_empty() {
        let path = PathBuf::from(script);
        log_line(
            app,
            &format!(
                "Resolved npm-cli.js at {}. Falling back to running npm via node.",
                path.display()
            ),
        )
        .await;
        Ok(NpmTool::Script(path))
    } else {
        let mut message = String::from(
            "npm not found. Install Node.js (which includes npm) or set NPM_BIN to the npm executable path.",
        );
        let details = stderr.trim();
        if !details.is_empty() {
            message.push(' ');
            message.push_str(details);
        }
        Err(message)
    }
}

async fn ensure_command(bin: &str) -> Result<(), String> {
    match TokioCommand::new(bin).arg("--version").status().await {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => Err(match status.code() {
            Some(code) => format!("{bin} --version exited with status {code}"),
            None => format!("{bin} --version failed"),
        }),
        Err(err) => Err(format!(
            "{bin} not found. Install {bin} and ensure it is on your PATH. ({err})"
        )),
    }
}

async fn launch(app: &AppHandle, state: tauri::State<'_, ServerState>) -> Result<(), String> {
    load_env();
    let silly_dir = silly_dir()?;

    if state.inner().child.lock().unwrap().is_some() {
        log_line(app, "WeylandTavern is already running.").await;
        return Ok(());
    }

    let run_npm = env::var("RUN_NPM_INSTALL").unwrap_or_else(|_| "always".into());
    let needs_npm_install = should_npm_install(&run_npm, &silly_dir)?;

    ensure_command("node").await?;

    if needs_npm_install {
        let npm_tool = locate_npm(app).await?;
        let npm_mode = env::var("NPM_MODE").unwrap_or_else(|_| "install".into());
        let mut cmd = npm_tool.into_command();
      
        cmd.current_dir(&silly_dir);
        cmd.env("NODE_ENV", "production");
        if npm_mode == "ci" {
            cmd.arg("ci");
        } else {
            cmd.args([
                "install",
                "--no-audit",
                "--no-fund",
                "--loglevel=error",
                "--no-progress",
                "--omit=dev",
            ]);
        }
        log_line(app, "Installing Node modules...").await;
        if !cmd.status().await.map_err(|e| e.to_string())?.success() {
            return Err("npm install failed".into());
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

    log_line(app, "Starting WeylandTavern...").await;

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

    let mut cmd = TokioCommand::new("node");
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
    let mut child = cmd.spawn().map_err(|e| e.to_string())?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    #[cfg(windows)]
    unsafe {
        let job_handle = CreateJobObjectW(None, PCWSTR::null())
            .map_err(|e| format!("CreateJobObjectW failed: {e}"))?;
        let job = JobHandle::new(job_handle);
        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        SetInformationJobObject(
            job.raw(),
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const _,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
        .map_err(|e| format!("SetInformationJobObject failed: {e}"))?;
        let pid = child.id().ok_or("pid unavailable")? as u32;
        let process = OpenProcess(PROCESS_ALL_ACCESS, false, pid)
            .map_err(|e| format!("OpenProcess failed: {e}"))?;
        let assign_result = AssignProcessToJobObject(job.raw(), process);
        let _ = CloseHandle(process);
        assign_result.map_err(|e| format!("AssignProcessToJobObject failed: {e}"))?;
        state.inner().job.lock().unwrap().replace(job);
    }

    if let Some(stdout) = stdout {
        let app_for_logs = app.clone();
        let log_file = file.clone();
        tauri::async_runtime::spawn(async move {
            let mut reader = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                let _ = append_log(&app_for_logs, &log_file, &line).await;
            }
        });
    }

    if let Some(stderr) = stderr {
        let app_for_logs = app.clone();
        let log_file = file.clone();
        tauri::async_runtime::spawn(async move {
            let mut reader = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                let _ = append_log(&app_for_logs, &log_file, &line).await;
            }
        });
    }

    state.inner().child.lock().unwrap().replace(child);

    let url = format!("http://{}:{}/", host, port);
    if wait_for_health(&url).await {
        let friendly = format!(
            "WeylandTavern is now active on {}:{} (By default)",
            host, port
        );
        log_line(app, &friendly).await;
        app.emit("server-ready", &url).ok();
        Ok(())
    } else {
        let message = format!(
            "Failed to verify server health at {}. Please check the logs.",
            url
        );
        log_line(app, &message).await;
        shutdown(app.state::<ServerState>()).await;
        Err(message)
    }
}

async fn append_log(
    app: &AppHandle,
    file: &Arc<AsyncMutex<tokio::fs::File>>,
    line: &str,
) -> Result<(), ()> {
    let mut f = file.lock().await;
    let _ = f.write_all(line.as_bytes()).await;
    let _ = f.write_all(b"\n").await;
    let _ = app.emit("log", line.to_string());
    Ok(())
}

async fn log_line(app: &AppHandle, line: &str) {
    let _ = app.emit("log", line.to_string());
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

async fn shutdown(state: tauri::State<'_, ServerState>) {
    let child = {
        let mut guard = state.inner().child.lock().unwrap();
        guard.take()
    };

    if let Some(mut child) = child {
        let _ = child.kill().await;
        let _ = child.wait().await;
        #[cfg(windows)]
        {
            if let Some(job) = {
                let mut guard = state.inner().job.lock().unwrap();
                guard.take()
            } {
                unsafe {
                    let _ = TerminateJobObject(job.raw(), 1);
                }
            }
        }
    } else {
        #[cfg(windows)]
        {
            if let Some(_job) = {
                let mut guard = state.inner().job.lock().unwrap();
                guard.take()
            } {}
        }
    }
}
