use std::sync::Arc;

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use ssh_core::{
    AppSettings, CoreEvent, FileEntry, ForwardInfo, ForwardSpec, HealthReport, HostEntry,
    JobInfo, Manager, SessionInfo,
};
use tauri::{Emitter, Manager as TauriManager, State};

type Core = Arc<Manager>;

fn err(e: ssh_core::CoreError) -> String {
    e.to_string()
}

#[tauri::command]
fn list_hosts(core: State<Core>) -> Vec<HostEntry> {
    core.list_hosts()
}

#[tauri::command]
fn save_host(core: State<Core>, host: HostEntry) -> Result<HostEntry, String> {
    core.save_host(host).map_err(err)
}

#[tauri::command]
fn delete_host(core: State<Core>, id: String) -> Result<(), String> {
    core.delete_host(&id).map_err(err)
}

#[tauri::command]
fn set_secret(core: State<Core>, host_id: String, secret: String) -> Result<(), String> {
    core.set_secret(&host_id, &secret).map_err(err)
}

#[tauri::command]
fn has_secret(core: State<Core>, host_id: String) -> bool {
    core.has_secret(&host_id)
}

#[tauri::command]
fn delete_secret(core: State<Core>, host_id: String) -> Result<(), String> {
    core.delete_secret(&host_id).map_err(err)
}

#[tauri::command]
async fn connect_host(core: State<'_, Core>, host_id: String) -> Result<SessionInfo, String> {
    core.connect_host(&host_id).await.map_err(err)
}

#[tauri::command]
async fn disconnect_session(core: State<'_, Core>, session_id: String) -> Result<(), String> {
    core.disconnect_session(&session_id).await.map_err(err)
}

#[tauri::command]
fn list_sessions(core: State<Core>) -> Vec<SessionInfo> {
    core.list_sessions()
}

#[tauri::command]
async fn open_terminal(
    core: State<'_, Core>,
    session_id: String,
    cols: u32,
    rows: u32,
) -> Result<String, String> {
    core.open_terminal(&session_id, cols, rows).await.map_err(err)
}

#[tauri::command]
async fn term_write(core: State<'_, Core>, term_id: String, data: String) -> Result<(), String> {
    let bytes = B64.decode(data).map_err(|e| e.to_string())?;
    core.term_write(&term_id, &bytes).await.map_err(err)
}

#[tauri::command]
async fn term_resize(
    core: State<'_, Core>,
    term_id: String,
    cols: u32,
    rows: u32,
) -> Result<(), String> {
    core.term_resize(&term_id, cols, rows).await.map_err(err)
}

#[tauri::command]
async fn close_terminal(core: State<'_, Core>, term_id: String) -> Result<(), String> {
    core.close_terminal(&term_id).await.map_err(err)
}

#[tauri::command]
async fn sftp_home(core: State<'_, Core>, session_id: String) -> Result<String, String> {
    core.sftp_home(&session_id).await.map_err(err)
}

#[tauri::command]
async fn sftp_list(
    core: State<'_, Core>,
    session_id: String,
    path: String,
) -> Result<Vec<FileEntry>, String> {
    core.sftp_list(&session_id, &path).await.map_err(err)
}

#[tauri::command]
async fn sftp_mkdir(core: State<'_, Core>, session_id: String, path: String) -> Result<(), String> {
    core.sftp_mkdir(&session_id, &path).await.map_err(err)
}

#[tauri::command]
async fn sftp_rename(
    core: State<'_, Core>,
    session_id: String,
    from: String,
    to: String,
) -> Result<(), String> {
    core.sftp_rename(&session_id, &from, &to).await.map_err(err)
}

#[tauri::command]
async fn sftp_delete(
    core: State<'_, Core>,
    session_id: String,
    path: String,
    is_dir: bool,
) -> Result<(), String> {
    core.sftp_delete(&session_id, &path, is_dir).await.map_err(err)
}

#[tauri::command]
fn sftp_download(
    core: State<Core>,
    session_id: String,
    remote: String,
    local: String,
) -> Result<String, String> {
    core.sftp_download(&session_id, &remote, &local)
        .map(|j| j.id)
        .map_err(err)
}

#[tauri::command]
fn sftp_upload(
    core: State<Core>,
    session_id: String,
    local: String,
    remote: String,
) -> Result<String, String> {
    core.sftp_upload(&session_id, &local, &remote)
        .map(|j| j.id)
        .map_err(err)
}

#[tauri::command]
async fn start_forward(
    core: State<'_, Core>,
    session_id: String,
    spec: ForwardSpec,
) -> Result<ForwardInfo, String> {
    core.start_forward(&session_id, spec).await.map_err(err)
}

#[tauri::command]
async fn stop_forward(core: State<'_, Core>, forward_id: String) -> Result<(), String> {
    core.stop_forward(&forward_id).await.map_err(err)
}

#[tauri::command]
async fn list_forwards(core: State<'_, Core>) -> Result<Vec<ForwardInfo>, String> {
    Ok(core.list_forwards().await)
}

#[tauri::command]
async fn run_health_check(core: State<'_, Core>, host_id: String) -> Result<HealthReport, String> {
    core.run_health_check(&host_id).await.map_err(err)
}

#[tauri::command]
fn get_health_history(core: State<Core>, host_id: String) -> Vec<HealthReport> {
    core.get_health_history(&host_id)
}

#[tauri::command]
fn list_jobs(core: State<Core>) -> Vec<JobInfo> {
    core.jobs.list()
}

#[tauri::command]
fn cancel_job(core: State<Core>, job_id: String) -> Result<(), String> {
    core.jobs.cancel(&job_id).map_err(err)
}

#[tauri::command]
fn get_settings(core: State<Core>) -> AppSettings {
    core.get_settings()
}

#[tauri::command]
fn set_settings(core: State<Core>, settings: AppSettings) -> Result<(), String> {
    core.save_settings(&settings).map_err(err)
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .expect("no app data dir");
            // Manager spawns background tasks at construction, so build it
            // inside Tauri's tokio runtime.
            let core = tauri::async_runtime::block_on(async { Manager::new(data_dir) })?;

            // Forward core events to the webview.
            let mut rx = core.subscribe();
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                while let Ok(event) = rx.recv().await {
                    let _ = match event {
                        CoreEvent::TermData { term_id, data } => {
                            handle.emit(&format!("term-data-{term_id}"), B64.encode(data))
                        }
                        CoreEvent::TermExit { term_id } => {
                            handle.emit(&format!("term-exit-{term_id}"), ())
                        }
                        CoreEvent::Job(job) => handle.emit("job-update", job),
                        CoreEvent::Health(report) => handle.emit("health-update", report),
                        CoreEvent::SessionClosed(id) => handle.emit("session-closed", id),
                    };
                }
            });

            app.manage(core);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_hosts,
            save_host,
            delete_host,
            set_secret,
            has_secret,
            delete_secret,
            connect_host,
            disconnect_session,
            list_sessions,
            open_terminal,
            term_write,
            term_resize,
            close_terminal,
            sftp_home,
            sftp_list,
            sftp_mkdir,
            sftp_rename,
            sftp_delete,
            sftp_download,
            sftp_upload,
            start_forward,
            stop_forward,
            list_forwards,
            run_health_check,
            get_health_history,
            list_jobs,
            cancel_job,
            get_settings,
            set_settings
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
