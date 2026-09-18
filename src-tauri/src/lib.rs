use tauri::Manager;

mod agent_config;
mod agentic_translate;
mod agenticskills;
mod app_paths;
mod board;
mod bridge;
mod checklist;
mod credentials;
mod diagnostics;
mod dialog_text;
mod dsh;
mod dsh_api;
mod durable_file;
mod external_editor;
mod file_recovery;
mod fonts;
mod git_commit;
mod git_diff;
mod git_index;
mod git_operation;
mod git_push;
mod git_repository;
mod git_status;
mod git_sync;
#[cfg(all(test, windows))]
mod git_test_support;
mod market;
pub mod message;
mod native_pi;
mod operation;
mod package;
mod pi_auth;
mod process_runner;
mod project_edit;
mod project_files;
mod project_watch;
mod prompt_enhance;
mod provider;
mod pty;
mod recovery;
mod rpc;
mod rpc_history;
#[cfg(test)]
mod rpc_integration_tests;
mod rpc_transport;
mod runtime;
mod runtime_pointer;
mod settings;
mod shell;
mod snapshot;
mod startup;
mod task;
mod theme;

pub fn credential_helper(provider_id: &str) -> Result<(), String> {
    credentials::credential_helper(provider_id)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();
    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, _, _| {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.unminimize();
            let _ = window.show();
            let _ = window.set_focus();
        }
    }));

    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_process::init());
    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_updater::Builder::new().build());

    builder
        .plugin(
            tauri_plugin_log::Builder::new()
                .clear_targets()
                .level(log::LevelFilter::Info)
                .target(tauri_plugin_log::Target::new(
                    tauri_plugin_log::TargetKind::LogDir {
                        file_name: Some("deeppi".into()),
                    },
                ))
                .max_file_size(5_000_000)
                .rotation_strategy(tauri_plugin_log::RotationStrategy::KeepSome(3))
                .build(),
        )
        .manage(dsh::DshManager::default())
        .manage(market::MarketCache::default())
        .manage(market::ModelCatalogCache::default())
        .manage(market::McpRegistryCache::default())
        .manage(agenticskills::AgenticCatalogCache::default())
        .manage(runtime::UpdateCache::default())
        .manage(runtime::RuntimeOperationLock::default())
        .manage(operation::OperationManager::default())
        .manage(package::PackageOperationLock::default())
        .manage(pty::PtyManager::default())
        .manage(shell::ShellManager::default())
        .manage(rpc::RpcManager::default())
        .manage(rpc_history::HistoryStore::default())
        .manage(project_files::FileIndexGate::default())
        .manage(project_watch::ProjectWatchManager::default())
        .manage(project_edit::FileEditGate::default())
        .manage(project_files::SearchManager::default())
        .manage(git_status::GitStatusGate::default())
        .manage(git_operation::GitOperations::default())
        .manage(startup::StartupState::default())
        .manage(provider::ProviderConfigGate::default())
        .manage(pi_auth::PiAuthManager::default())
        .setup(|app| {
            let handle = app.handle().clone();
            board::install_main_window_hooks(app.handle());
            let roaming = app.path().app_data_dir()?;
            let local = app.path().app_local_data_dir()?;
            // Recovery and database migration must finish before publishing any store.
            // They never run on the native window event loop.
            drop(tauri::async_runtime::spawn(async move {
                let worker_app = handle.clone();
                let result = tauri::async_runtime::spawn_blocking(move || {
                    let started = std::time::Instant::now();
                    let project_root = std::env::current_dir()
                        .map(pty::resolve_default_working_directory)
                        .map_err(|error| error.to_string())?;
                    // Pi/DSH 运行时安装在 DeepPi 安装目录下的 runtimes 子文件夹，
                    // 跟随安装盘符，不写 C 盘 AppData。
                    let install_dir = std::env::current_exe()
                        .ok()
                        .and_then(|exe| exe.parent().map(std::path::Path::to_path_buf))
                        .ok_or_else(|| crate::message::msg("app.install_dir_unknown"))?;
                    let runtimes = app_paths::managed_runtimes_root(&install_dir, &project_root);
                    let paths = app_paths::AppPaths::from_roots_with_runtimes(
                        roaming,
                        local,
                        runtimes,
                        project_root,
                    )?;
                    snapshot::Snapshot::recover_pending(&paths.backups)?;
                    paths.managed_pi_runtime()?;
                    paths.managed_dsh_runtime()?;
                    let store = task::TaskStore::open(&paths.database)?;
                    let checklist_store = checklist::ChecklistStore::open(&paths.checklist)?;
                    let file_recovery = file_recovery::RecoveryStore::open(
                        &paths.database.with_file_name("file-recovery.db"),
                    )?;
                    bridge::install(&paths)?;
                    let settings = settings::SettingsStore::open(
                        paths.settings.clone(),
                        paths.backups.clone(),
                    )?;
                    worker_app.manage(paths);
                    worker_app.manage(store);
                    worker_app.manage(file_recovery);
                    worker_app.manage(checklist_store);
                    worker_app.manage(settings);
                    log::info!(
                        "event=app_initialization status=ready duration_ms={}",
                        started.elapsed().as_millis()
                    );
                    Ok::<(), String>(())
                })
                .await
                .unwrap_or_else(|_| Err(crate::message::msg("app.init_thread_failed")));
                handle.state::<startup::StartupState>().finish(result);
            }));
            Ok(())
        })
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(|invoke| {
            if invoke.message.command() != "await_startup"
                && !startup::is_ready(invoke.message.webview_ref().state())
            {
                invoke
                    .resolver
                    .reject(crate::message::msg("app.startup.not_ready"));
                return true;
            }
            let handler: fn(tauri::ipc::Invoke<tauri::Wry>) -> bool = tauri::generate_handler![
                startup::await_startup,
                diagnostics::diagnostics_snapshot,
                diagnostics::diagnostics_clear,
                diagnostics::diagnostics_export,
                board::open_board_window,
                checklist::open_checklist_window,
                checklist::list_checklist_items,
                checklist::add_checklist_item,
                checklist::toggle_checklist_item,
                checklist::delete_checklist_item,
                rpc::start_rpc_task,
                rpc::subscribe_rpc,
                rpc::rpc_command,
                rpc::rpc_history_open,
                rpc::rpc_history_page,
                rpc::rpc_history_close,
                rpc::stop_rpc_task,
                rpc::save_last_model_choice,
                rpc::get_last_model_choice,
                project_files::list_project_files,
                project_watch::start_project_watch,
                project_watch::stop_project_watch,
                project_watch::ping_project_watch,
                project_files::read_project_file,
                project_edit::save_project_file,
                file_recovery::list_project_recoveries,
                file_recovery::read_project_recovery,
                file_recovery::delete_project_recovery,
                file_recovery::restore_project_recovery,
                project_files::search_project_files,
                project_files::cancel_project_search,
                external_editor::open_project_in_editor,
                external_editor::save_external_editor,
                git_status::project_git_status,
                git_diff::project_git_diff,
                git_index::project_git_change_index,
                git_commit::project_git_prepare_commit,
                git_commit::project_git_commit,
                git_push::project_git_push_targets,
                git_push::project_git_push,
                git_push::project_git_verify_remote,
                git_sync::project_git_sync_tracking,
                git_operation::cancel_git_read,
                dsh::create_dsh_webview,
                dsh::start_dsh,
                dsh::stop_dsh,
                dsh::dsh_diagnose,
                dsh::dsh_repair,
                package::list_pi_packages,
                package::package_operation,
                agent_config::list_mcp_servers,
                agent_config::save_mcp_server,
                agent_config::delete_mcp_server,
                agent_config::list_skills,
                agent_config::save_skill,
                agent_config::delete_skill,
                operation::cancel_operation,
                provider::delete_pi_provider,
                provider::list_pi_providers,
                provider::list_provider_models,
                provider::save_pi_provider,
                credentials::delete_provider_credential,
                credentials::provider_credential_status,
                credentials::save_provider_credential,
                credentials::test_provider_connection,
                credentials::test_model_connection,
                pi_auth::pi_auth_providers,
                pi_auth::pi_auth_status,
                pi_auth::pi_auth_start_login,
                pi_auth::pi_auth_respond,
                pi_auth::pi_auth_cancel,
                pi_auth::pi_auth_logout,
                pi_auth::pi_auth_provider_models,
                agenticskills::agentic_mcp_detail,
                agenticskills::agentic_skill_detail,
                agenticskills::agentic_workflow_detail,
                agenticskills::install_agentic_mcp,
                agenticskills::install_agentic_skill,
                agenticskills::install_agentic_workflow,
                agenticskills::list_installed_workflows,
                agenticskills::prefetch_agentic_details,
                agenticskills::search_agentic_mcp,
                agenticskills::search_agentic_skills,
                agenticskills::search_agentic_workflows,
                market::pi_model_profile,
                market::pi_package_metadata,
                market::search_pi_models,
                market::search_pi_packages,
                market::search_mcp_registry,
                pty::default_working_directory,
                pty::acknowledge_pi_output,
                pty::start_pi_task,
                pty::write_pi_task,
                pty::resize_pi_task,
                pty::restart_pi_task,
                pty::stop_all_pi_tasks,
                pty::stop_pi_task,
                pty::stop_pi_run,
                shell::start_shell,
                shell::write_shell,
                shell::resize_shell,
                shell::stop_shell,
                runtime::runtime_status,
                runtime::check_runtime_updates,
                runtime::clear_runtime_update_cache,
                runtime::install_runtime,
                runtime::rollback_runtime,
                fonts::list_system_fonts,
                prompt_enhance::enhance_prompt,
                agentic_translate::translate_agentic_texts,
                settings::get_settings,
                settings::save_settings,
                theme::theme_export,
                theme::theme_import,
                task::add_project,
                task::archive_task,
                task::delete_task,
                task::list_projects,
                task::list_tasks,
                task::remove_project,
                task::rename_task,
                task::restore_task,
                task::touch_project
            ];
            handler(invoke)
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
