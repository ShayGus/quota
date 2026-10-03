use tauri::{Context, Runtime};

pub(crate) fn for_build<R: Runtime>(
    mut context: Context<R>,
    debug: bool,
) -> Result<Context<R>, String> {
    if !debug {
        return Ok(context);
    }

    let development: tauri::utils::config::Config =
        serde_json::from_str(include_str!("../tauri.dev.conf.json"))
            .map_err(|error| format!("development_identity_invalid:{error}"))?;
    let name = development
        .product_name
        .ok_or("development_product_name_missing")?;
    context.package_info_mut().name.clone_from(&name);
    let config = context.config_mut();
    config.identifier = development.identifier;
    config.product_name = Some(name);
    config.main_binary_name = development.main_binary_name;
    for window in &mut config.app.windows {
        if let Some(dev_window) = development
            .app
            .windows
            .iter()
            .find(|candidate| candidate.label == window.label)
        {
            window.title.clone_from(&dev_window.title);
        }
    }
    Ok(context)
}

pub(crate) fn credential_service<R: Runtime>(app: &tauri::AppHandle<R>) -> &str {
    &app.config().identifier
}

#[cfg(test)]
mod tests {
    use tauri::Manager;
    use tauri::test::{MockRuntime, mock_builder, mock_context, noop_assets};

    use super::*;

    fn production_context() -> Context<MockRuntime> {
        let mut context = mock_context(noop_assets());
        *context.config_mut() = serde_json::from_str(include_str!("../tauri.conf.json"))
            .expect("the production configuration is valid");
        context.package_info_mut().name = "Quota".to_owned();
        context
    }

    #[test]
    fn debug_and_release_resolve_separate_storage_and_credential_namespaces() {
        let production = mock_builder()
            .build(for_build(production_context(), false).expect("release identity"))
            .expect("release app");
        let development = mock_builder()
            .build(for_build(production_context(), true).expect("debug identity"))
            .expect("debug app");

        let production_config = production.path().app_config_dir().expect("config path");
        let development_config = development.path().app_config_dir().expect("config path");
        let production_data = production.path().app_data_dir().expect("data path");
        let development_data = development.path().app_data_dir().expect("data path");
        assert_ne!(production_config, development_config);
        assert_ne!(production_data, development_data);
        assert!(production_config.ends_with("app.quota.monitor"));
        assert!(development_config.ends_with("app.quota.monitor.dev"));
        assert!(production_data.ends_with("app.quota.monitor"));
        assert!(development_data.ends_with("app.quota.monitor.dev"));
        assert_eq!(credential_service(production.handle()), "app.quota.monitor");
        assert_eq!(
            credential_service(development.handle()),
            "app.quota.monitor.dev"
        );
        assert_ne!(
            credential_service(production.handle()),
            credential_service(development.handle())
        );
        assert_eq!(production.package_info().name, "Quota");
        assert_eq!(development.package_info().name, "Quota Dev");
    }

    #[test]
    fn debug_identity_preserves_window_and_security_overrides() {
        let mut original = production_context();
        let config = original.config_mut();
        config.app.windows.first_mut().expect("overview").width = 900.0;
        config.build.dev_url = Some("http://localhost:1433".parse().expect("dev URL"));
        let expected_security = config.app.security.clone();
        let expected_url = config.build.dev_url.clone();
        let context = for_build(original, true).expect("debug identity");
        let config = context.config();
        assert_eq!(config.identifier, "app.quota.monitor.dev");
        assert_eq!(config.product_name.as_deref(), Some("Quota Dev"));
        assert_eq!(config.main_binary_name.as_deref(), Some("quota-dev"));
        assert_eq!(
            config
                .app
                .windows
                .first()
                .expect("overview")
                .width
                .to_bits(),
            900.0_f64.to_bits()
        );
        assert_eq!(config.build.dev_url, expected_url);
        assert_eq!(config.app.security, expected_security);
        assert_eq!(
            config
                .app
                .windows
                .iter()
                .map(|window| window.title.as_str())
                .collect::<Vec<_>>(),
            ["Quota Dev", "Quota Dev settings", "Quota Dev widget"]
        );
    }

    #[test]
    fn release_keeps_the_supplied_identity_and_packaging_overlay() {
        let original = production_context();
        let expected = serde_json::to_value(original.config()).expect("config value");
        let context = for_build(original, false).expect("release identity");
        assert_eq!(
            serde_json::to_value(context.config()).expect("config value"),
            expected
        );
        assert_eq!(context.package_info().name, "Quota");

        let dev_package = for_build(production_context(), true).expect("development package");
        let expected = serde_json::to_value(dev_package.config()).expect("config value");
        let context = for_build(dev_package, false).expect("development release identity");
        assert_eq!(
            serde_json::to_value(context.config()).expect("config value"),
            expected
        );
        assert_eq!(context.package_info().name, "Quota Dev");
    }

    #[test]
    fn selecting_the_development_overlay_twice_is_idempotent() {
        let context = for_build(production_context(), true).expect("debug identity");
        let expected = serde_json::to_value(context.config()).expect("config value");
        let context = for_build(context, true).expect("debug identity");
        assert_eq!(
            serde_json::to_value(context.config()).expect("config value"),
            expected
        );
        assert_eq!(context.package_info().name, "Quota Dev");
    }
}
