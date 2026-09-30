use super::*;
use crate::legacy_core::config::ConfigBuilder;
use crate::legacy_core::config::ConfigOverrides;
use codex_protocol::config_types::SandboxMode;
use codex_protocol::models::BUILT_IN_PERMISSION_PROFILE_READ_ONLY;
use codex_protocol::protocol::AskForApproval;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn owned_local_resume_preserves_named_launch_profile_without_local_path_overrides() {
    let home = tempfile::tempdir().unwrap();
    let config = ConfigBuilder::default()
        .codex_home(home.path().to_path_buf())
        .cli_overrides(vec![(
            "default_permissions".to_string(),
            toml::Value::String(BUILT_IN_PERMISSION_PROFILE_READ_ONLY.to_string()),
        )])
        .build()
        .await
        .unwrap();
    let thread_id = ThreadId::new();
    let expected_approval = Some(config.permissions.approval_policy.value().into());
    let expected_reviewer = approvals_reviewer_override_from_config(&config);
    let restored = thread_resume_params_from_config(
        config.clone(),
        thread_id,
        ThreadParamsMode::Remote,
        /*remote_cwd_override*/ None,
        ResumeModelSettings::RestoreFromThread,
        ResumePermissionSettings::RestoreFromThread,
    );
    let owned = thread_resume_params_from_config(
        config,
        thread_id,
        ThreadParamsMode::Remote,
        /*remote_cwd_override*/ None,
        ResumeModelSettings::RestoreFromThread,
        ResumePermissionSettings::OverrideFromLaunchConfig,
    );
    assert_eq!(
        (
            restored.approval_policy,
            restored.approvals_reviewer,
            restored.sandbox,
            restored.permissions
        ),
        (None, None, None, None)
    );
    assert_eq!(
        (
            owned.approval_policy,
            owned.approvals_reviewer,
            owned.sandbox,
            owned.permissions
        ),
        (
            expected_approval,
            expected_reviewer,
            None,
            Some(BUILT_IN_PERMISSION_PROFILE_READ_ONLY.to_string())
        )
    );
    assert_eq!(
        (
            owned.cwd,
            owned.runtime_workspace_roots,
            owned.model_provider
        ),
        (None, None, None)
    );
    assert_eq!(
        owned
            .config
            .as_ref()
            .and_then(|config| config.get("default_permissions")),
        Some(&serde_json::Value::String(
            BUILT_IN_PERMISSION_PROFILE_READ_ONLY.to_string()
        ))
    );
    assert!(
        restored
            .config
            .as_ref()
            .is_none_or(|config| !config.contains_key("default_permissions"))
    );
}

#[tokio::test]
async fn owned_local_resume_preserves_legacy_launch_permissions_and_explicit_remote_cwd() {
    let home = tempfile::tempdir().unwrap();
    let config = ConfigBuilder::default()
        .codex_home(home.path().to_path_buf())
        .harness_overrides(ConfigOverrides {
            cwd: Some(home.path().to_path_buf()),
            approval_policy: Some(AskForApproval::OnRequest),
            sandbox_mode: Some(SandboxMode::WorkspaceWrite),
            ..Default::default()
        })
        .build()
        .await
        .unwrap();
    let expected_sandbox = sandbox_mode_from_permission_profile(
        &config.permissions.effective_permission_profile(),
        config.cwd.as_path(),
    );
    assert!(expected_sandbox.is_some());
    let params = thread_resume_params_from_config(
        config,
        ThreadId::new(),
        ThreadParamsMode::Remote,
        Some(home.path()),
        ResumeModelSettings::OverrideFromCurrentConfig,
        ResumePermissionSettings::OverrideFromLaunchConfig,
    );
    assert_eq!(
        (params.approval_policy, params.sandbox, params.permissions),
        (
            Some(codex_app_server_protocol::AskForApproval::OnRequest),
            expected_sandbox,
            None
        )
    );
    assert_eq!(params.cwd, Some(home.path().to_string_lossy().to_string()));
    assert_eq!(
        (params.runtime_workspace_roots, params.model_provider),
        (None, None)
    );
}
