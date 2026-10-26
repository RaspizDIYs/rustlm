use crate::services::network::{
    self, Installation, NetworkJobs, Patch, Progress, Report, RunContext,
};
use crate::services::network_hosts::{self, HostsPlan, Pin};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tauri::{ipc::Channel, Manager, State};

#[tauri::command]
pub fn network_inspect(path: String) -> Result<Installation, String> {
    network::installation(&path)?;
    network::migrate_backups(
        &path,
        &crate::services::repair_paths::root()?.join("backups"),
    )?;
    network::installation(&path)
}

#[tauri::command]
pub async fn network_run(
    app: tauri::AppHandle,
    jobs: State<'_, NetworkJobs>,
    mut context: RunContext,
    installation: Option<String>,
    on_progress: Channel<Progress>,
) -> Result<Report, String> {
    if context.connection == "zapret" {
        let folder = installation.as_deref().ok_or("Выберите папку Zapret")?;
        let strategy = context
            .strategy
            .as_deref()
            .ok_or("Выберите точный BAT-файл")?;
        let readiness_root = folder.to_owned();
        let readiness_strategy = strategy.to_owned();
        tokio::task::spawn_blocking(move || {
            crate::services::repair::readiness(&readiness_root, &readiness_strategy)
        })
        .await
        .map_err(|e| e.to_string())??
        .ensure_ready()?;
        let info = network::installation(folder)?;
        if !info.strategies.iter().any(|s| s == strategy) {
            return Err("Стратегия не найдена".into());
        }
        let path = std::path::Path::new(&info.root).join(strategy);
        let canonical = std::fs::canonicalize(&path).map_err(|e| e.to_string())?;
        if !canonical.starts_with(&info.root) {
            return Err("Стратегия вне папки Zapret".into());
        }
        context.strategy_hash = Some(network::hash(
            &std::fs::read(path).map_err(|e| e.to_string())?,
        ));
    } else {
        context.strategy = None;
        context.strategy_hash = None;
    }
    let root = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("Repair Tools")
        .join("network-runs");
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut current = jobs.0.lock().map_err(|e| e.to_string())?;
        if current.is_some() {
            return Err("Предыдущий прогон ещё выполняется".into());
        }
        *current = Some(cancel.clone());
    }
    context.observed_strategy = None;
    context.observed_command_hash = None;
    if let Ok(Ok(Some(active))) = tokio::task::spawn_blocking(crate::services::repair::detect).await
    {
        context.observed_strategy = active.strategy;
        context.observed_command_hash = Some(active.fingerprint);
    }
    context.network_fingerprint = None;
    if let Ok(Ok(identity)) =
        tokio::task::spawn_blocking(crate::services::repair::network_identity).await
    {
        context.network_fingerprint = Some(identity);
    }
    let result = network::diagnose(context, root, cancel, |p| {
        let _ = on_progress.send(p);
    })
    .await;
    *jobs.0.lock().map_err(|e| e.to_string())? = None;
    result
}
#[tauri::command]
pub fn network_cancel(jobs: State<'_, NetworkJobs>) -> Result<(), String> {
    if let Some(cancel) = jobs.0.lock().map_err(|e| e.to_string())?.as_ref() {
        cancel.store(true, Ordering::Relaxed);
    }
    Ok(())
}
#[tauri::command]
pub fn network_history(app: tauri::AppHandle) -> Result<Vec<Report>, String> {
    let mut reports = network::history(
        &app.path()
            .app_data_dir()
            .map_err(|e| e.to_string())?
            .join("Repair Tools")
            .join("network-runs"),
    )?;
    reports.extend(network::history(
        &app.path()
            .app_data_dir()
            .map_err(|e| e.to_string())?
            .join("network-runs"),
    )?);
    reports.sort_by(|a, b| b.id.cmp(&a.id));
    reports.dedup_by(|a, b| a.id == b.id);
    Ok(reports)
}
#[tauri::command]
pub fn network_preview(
    app: tauri::AppHandle,
    path: String,
    strategy: String,
    variant: Option<u8>,
    report_id: Option<String>,
) -> Result<Patch, String> {
    let report = report_id.map(|id| report_by_id(&app, &id)).transpose()?;
    network::preview_variant(&path, &strategy, variant.unwrap_or(0), report.as_ref())
}
#[tauri::command]
pub fn network_apply(app: tauri::AppHandle, patch: Patch) -> Result<String, String> {
    let report = patch
        .source_report
        .as_ref()
        .map(|id| report_by_id(&app, id))
        .transpose()?;
    network::apply(
        patch,
        report.as_ref(),
        &crate::services::repair_paths::root()?.join("backups"),
    )
}
#[tauri::command]
pub fn network_snapshot(path: String, strategy: String) -> Result<String, String> {
    let store = crate::services::repair_paths::root()?.join("backups");
    let id = network::snapshot(&path, &strategy, &store)?;
    let root = network::installation(&path)?.root;
    network_hosts::save_snapshot(
        &network::backup_directory(std::path::Path::new(&root), &store)
            .join(&id)
            .join("hosts.original"),
    )?;
    Ok(id)
}
#[tauri::command]
pub fn network_restore(path: String, backup_id: String) -> Result<(), String> {
    network::restore(
        &path,
        &backup_id,
        true,
        &crate::services::repair_paths::root()?.join("backups"),
    )
}
#[tauri::command]
pub fn network_select_profile(
    path: String,
    strategy: String,
    configured: bool,
) -> Result<String, String> {
    network::select_profile(
        &path,
        &strategy,
        configured,
        &crate::services::repair_paths::root()?.join("backups"),
    )
}

fn report_by_id(app: &tauri::AppHandle, id: &str) -> Result<Report, String> {
    network_history(app.clone())?
        .into_iter()
        .find(|r| r.id == id)
        .ok_or("Замер не найден в локальной истории".into())
}
#[tauri::command]
pub fn network_hosts_preview(
    app: tauri::AppHandle,
    id: String,
    pins: Vec<Pin>,
) -> Result<HostsPlan, String> {
    network_hosts::preview(&report_by_id(&app, &id)?, pins)
}
#[tauri::command]
pub async fn network_hosts_apply(app: tauri::AppHandle, plan: HostsPlan) -> Result<String, String> {
    let report = report_by_id(&app, &plan.report_id)?;
    let root = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("Repair Tools")
        .join("hosts-backups");
    tokio::task::spawn_blocking(move || network_hosts::apply(&report, plan, &root))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn network_hosts_restore(app: tauri::AppHandle, backup_id: String) -> Result<(), String> {
    let mut root = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("Repair Tools")
        .join("hosts-backups");
    if !root.join(format!("{backup_id}.json")).exists() {
        root = app
            .path()
            .app_data_dir()
            .map_err(|e| e.to_string())?
            .join("hosts-backups");
    }
    tokio::task::spawn_blocking(move || network_hosts::restore(&backup_id, &root))
        .await
        .map_err(|e| e.to_string())?
}
