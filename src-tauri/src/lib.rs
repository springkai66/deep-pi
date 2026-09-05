use tauri::Manager;

mod app_paths;
mod bridge;
mod credentials;
mod dsh;
mod dsh_api;
mod market;
mod package;
mod provider;
mod pty;
mod runtime;
mod settings;
mod task;

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
        .manage(runtime::UpdateCache::default())
        .manage(runtime::RuntimeOperationLock::default())
        .manage(package::PackageOperationLock::default())
        .manage(pty::PtyManager::default())
        .setup(|app| {
            let project_root =
                std::env::current_dir().map(pty::resolve_default_working_directory)?;
            let paths = app_paths::AppPaths::from_roots(
                app.path().app_data_dir()?,
                app.path().app_local_data_dir()?,
                project_root,
            )
            .map_err(std::io::Error::other)?;
            let store = task::TaskStore::open(&paths.database).map_err(std::io::Error::other)?;
            bridge::install(&paths).map_err(std::io::Error::other)?;
            let settings =
                settings::SettingsStore::open(paths.settings.clone(), paths.backups.clone())
                    .map_err(std::io::Error::other)?;
            app.manage(paths);
            app.manage(store);
            app.manage(settings);
            let prewarm_app = app.handle().clone();
            drop(tauri::async_runtime::spawn_blocking(move || {
                dsh::prewarm_dsh(prewarm_app)
            }));
            log::info!("DeepPi initialized");
            Ok(())
        })
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            dsh::create_dsh_webview,
            dsh::start_dsh,
            dsh::stop_dsh,
            package::list_pi_packages,
            package::package_operation,
            provider::delete_pi_provider,
            provider::list_pi_providers,
            provider::list_provider_models,
            provider::save_pi_provider,
            credentials::delete_provider_credential,
            credentials::provider_credential_status,
            credentials::save_provider_credential,
            credentials::test_provider_connection,
            market::pi_model_profile,
            market::pi_package_metadata,
            market::search_pi_models,
            market::search_pi_packages,
            pty::default_working_directory,
            pty::start_pi_task,
            pty::write_pi_task,
            pty::resize_pi_task,
            pty::restart_pi_task,
            pty::stop_all_pi_tasks,
            pty::stop_pi_task,
            runtime::runtime_status,
            runtime::check_runtime_updates,
            runtime::clear_runtime_update_cache,
            runtime::install_runtime,
            runtime::rollback_runtime,
            settings::get_settings,
            settings::save_settings,
            task::add_project,
            task::archive_task,
            task::delete_task,
            task::list_projects,
            task::list_tasks,
            task::remove_project,
            task::rename_task,
            task::restore_task,
            task::touch_project
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
