mod common;

use akooti_core::{
    AGENTS_MCP_FOLDER_ID, AuditStatus, DEV_LOCAL_FOLDER_ID, EphemeralAccountDraft, McpAction,
    McpScope, NewMcpToken, PERSONAL_FOLDER_ID, SecretString, Vault, VaultError,
};
use common::{fixture, item_draft};

fn token(scopes: Vec<McpScope>, folders: Vec<&str>, default_create: Option<&str>) -> NewMcpToken {
    NewMcpToken {
        name: "Claude Code - Docker Local".to_owned(),
        scopes,
        folder_ids: folders.into_iter().map(str::to_owned).collect(),
        default_create_folder_id: default_create.map(str::to_owned),
        expires_at: None,
    }
}

#[test]
fn a_token_secret_is_returned_once_and_never_listed() {
    let (_temp, paths) = fixture();
    let mut vault = Vault::create(&paths, "password").expect("create");

    let issued = vault
        .create_mcp_token(token(
            vec![McpScope::ReadMetadata],
            vec![DEV_LOCAL_FOLDER_ID],
            None,
        ))
        .expect("issue");
    assert!(issued.secret.starts_with("ak_mcp_"));
    assert_eq!(issued.token.token_prefix.len(), "ak_mcp_".len() + 8);
    assert!(issued.token.is_active(0));

    let listed = vault.mcp_tokens().expect("list");
    assert_eq!(listed.len(), 1);
    let serialized = format!("{listed:?}");
    assert!(!serialized.contains(issued.secret.as_str()));
    assert!(!serialized.contains("token_hash"));
    assert_eq!(listed[0].folder_ids, vec![DEV_LOCAL_FOLDER_ID]);
    assert_eq!(listed[0].scopes, vec![McpScope::ReadMetadata]);
}

#[test]
fn authentication_rejects_unknown_revoked_and_expired_tokens() {
    let (_temp, paths) = fixture();
    let mut vault = Vault::create(&paths, "password").expect("create");

    let issued = vault
        .create_mcp_token(token(
            vec![McpScope::ReadMetadata],
            vec![PERSONAL_FOLDER_ID],
            None,
        ))
        .expect("issue");
    let principal = vault
        .authenticate_mcp_token(&issued.secret)
        .expect("authenticate");
    assert_eq!(principal.name, "Claude Code - Docker Local");
    assert!(vault.mcp_tokens().expect("list")[0].last_used_at.is_some());

    assert!(matches!(
        vault.authenticate_mcp_token("ak_mcp_0000000000000000"),
        Err(VaultError::InvalidCredentials)
    ));

    vault.revoke_mcp_token(&issued.token.id).expect("revoke");
    assert!(matches!(
        vault.authenticate_mcp_token(&issued.secret),
        Err(VaultError::InvalidCredentials)
    ));

    let expired = vault
        .create_mcp_token(NewMcpToken {
            expires_at: Some(1),
            ..token(vec![McpScope::ReadMetadata], vec![PERSONAL_FOLDER_ID], None)
        })
        .err();
    assert!(matches!(expired, Some(VaultError::InvalidInput(_))));
}

#[test]
fn search_is_scoped_to_allowed_folders_and_never_returns_a_secret() {
    let (_temp, paths) = fixture();
    let mut vault = Vault::create(&paths, "password").expect("create");
    vault
        .create_item(item_draft(
            "Perso",
            "perso-secret-value",
            "https://example.com",
        ))
        .expect("personal");
    vault
        .create_item(item_draft(
            "Docker",
            "docker-secret-value",
            "http://localhost:2375",
        ))
        .expect("dev");

    let issued = vault
        .create_mcp_token(token(
            vec![McpScope::ReadMetadata],
            vec![DEV_LOCAL_FOLDER_ID],
            None,
        ))
        .expect("issue");
    let principal = vault
        .authenticate_mcp_token(&issued.secret)
        .expect("authenticate");

    let results = vault.mcp_search(&principal, "", 50).expect("search");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].title, "Docker");
    let serialized = format!("{results:?}");
    assert!(!serialized.contains("docker-secret-value"));
    assert!(!serialized.contains("perso-secret-value"));

    let denied = vault.create_mcp_token(token(
        vec![McpScope::BlindFill],
        vec![PERSONAL_FOLDER_ID],
        None,
    ));
    let issued = denied.expect("issue");
    let principal = vault
        .authenticate_mcp_token(&issued.secret)
        .expect("authenticate");
    assert!(matches!(
        vault.mcp_search(&principal, "", 10),
        Err(VaultError::Forbidden)
    ));
}

#[test]
fn fill_plans_need_the_scope_and_an_allowed_folder() {
    let (_temp, paths) = fixture();
    let mut vault = Vault::create(&paths, "password").expect("create");
    let mut draft = item_draft("Docker", "docker-secret-value", "http://localhost:2375");
    draft.totp_secret = Some(SecretString::new(
        "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ".to_owned(),
    ));
    let item = vault.create_item(draft).expect("item");

    let issued = vault
        .create_mcp_token(token(
            vec![McpScope::BlindFill],
            vec![DEV_LOCAL_FOLDER_ID],
            None,
        ))
        .expect("issue");
    let principal = vault
        .authenticate_mcp_token(&issued.secret)
        .expect("authenticate");

    let plan = vault
        .mcp_fill_plan(&principal, &item.id, McpScope::BlindFill)
        .expect("plan");
    assert_eq!(
        plan.password.as_ref().map(SecretString::expose),
        Some("docker-secret-value")
    );
    // The plan exposes a current code, never the stored seed.
    let code = plan.totp.as_ref().expect("totp").expose().to_owned();
    assert_eq!(code.len(), 6);
    assert!(!format!("{plan:?}").contains("docker-secret-value"));

    assert!(matches!(
        vault.mcp_fill_plan(&principal, &item.id, McpScope::ExecEnv),
        Err(VaultError::Forbidden)
    ));

    let other = vault
        .create_item(item_draft(
            "Perso",
            "perso-secret-value",
            "https://example.com",
        ))
        .expect("personal");
    assert!(matches!(
        vault.mcp_fill_plan(&principal, &other.id, McpScope::BlindFill),
        Err(VaultError::Forbidden)
    ));
}

#[test]
fn ephemeral_accounts_land_in_the_agents_folder_with_a_ttl() {
    let (_temp, paths) = fixture();
    let mut vault = Vault::create(&paths, "password").expect("create");

    let issued = vault
        .create_mcp_token(token(
            vec![McpScope::ReadMetadata, McpScope::CreateAndFill],
            vec![AGENTS_MCP_FOLDER_ID, DEV_LOCAL_FOLDER_ID],
            Some(AGENTS_MCP_FOLDER_ID),
        ))
        .expect("issue");
    let principal = vault
        .authenticate_mcp_token(&issued.secret)
        .expect("authenticate");

    let created = vault
        .mcp_create_ephemeral(
            &principal,
            EphemeralAccountDraft {
                title: "Test signup".to_owned(),
                primary_url: Some("http://localhost:3000/signup".to_owned()),
                username: Some("agent@example.test".to_owned()),
                ttl_seconds: 900,
                folder_id: None,
            },
        )
        .expect("create");

    // The signup URL is local, so additive routing also adds Dev Local; what
    // matters is that the account never lands in Personal.
    assert!(
        created
            .item
            .folder_ids
            .contains(&AGENTS_MCP_FOLDER_ID.to_owned())
    );
    assert!(
        !created
            .item
            .folder_ids
            .contains(&PERSONAL_FOLDER_ID.to_owned())
    );
    assert_eq!(
        created.item.created_by_agent.as_deref(),
        Some("Claude Code - Docker Local")
    );
    assert!(created.item.expires_at.is_some());
    assert!(created.item.has_password);
    assert_eq!(
        created
            .plan
            .password
            .as_ref()
            .map(SecretString::expose)
            .map(str::len),
        Some(24)
    );

    let forbidden = vault.mcp_create_ephemeral(
        &principal,
        EphemeralAccountDraft {
            title: "Ailleurs".to_owned(),
            primary_url: None,
            username: None,
            ttl_seconds: 900,
            folder_id: Some(PERSONAL_FOLDER_ID.to_owned()),
        },
    );
    assert!(matches!(forbidden, Err(VaultError::Forbidden)));
}

#[test]
fn exec_env_needs_its_scope_and_only_exports_metadata() {
    let (_temp, paths) = fixture();
    let mut vault = Vault::create(&paths, "password").expect("create");
    let item = vault
        .create_item(item_draft(
            "Docker",
            "docker-secret-value",
            "http://localhost:2375",
        ))
        .expect("item");

    let issued = vault
        .create_mcp_token(token(
            vec![McpScope::ExecEnv],
            vec![DEV_LOCAL_FOLDER_ID],
            None,
        ))
        .expect("issue");
    let principal = vault
        .authenticate_mcp_token(&issued.secret)
        .expect("authenticate");

    let variables = vault.mcp_env(&principal, &item.id).expect("env");
    let names = variables
        .iter()
        .map(|(name, _)| name.as_str())
        .collect::<Vec<_>>();
    assert!(names.contains(&"AKOOTI_PASSWORD"));
    assert!(names.contains(&"AKOOTI_USERNAME"));

    let issued = vault
        .create_mcp_token(token(
            vec![McpScope::ReadMetadata],
            vec![DEV_LOCAL_FOLDER_ID],
            None,
        ))
        .expect("issue");
    let principal = vault
        .authenticate_mcp_token(&issued.secret)
        .expect("authenticate");
    assert!(matches!(
        vault.mcp_env(&principal, &item.id),
        Err(VaultError::Forbidden)
    ));
}

#[test]
fn every_ai_action_is_audited_without_a_secret() {
    let (_temp, paths) = fixture();
    let mut vault = Vault::create(&paths, "password").expect("create");
    vault
        .create_item(item_draft(
            "Docker",
            "docker-secret-value",
            "http://localhost:2375",
        ))
        .expect("item");

    let issued = vault
        .create_mcp_token(token(
            vec![McpScope::ReadMetadata, McpScope::BlindFill],
            vec![DEV_LOCAL_FOLDER_ID],
            None,
        ))
        .expect("issue");
    let principal = vault
        .authenticate_mcp_token(&issued.secret)
        .expect("authenticate");

    vault.mcp_search(&principal, "docker", 10).expect("search");

    // The core audits the search; the daemon writes the action row once the
    // injection outcome is known (covered by src-tauri tests).
    let logs = vault.mcp_audit_logs(20).expect("logs");
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].action, McpAction::VaultSearch);
    assert_eq!(logs[0].status, AuditStatus::Success);

    let serialized = format!("{logs:?}");
    assert!(!serialized.contains("docker-secret-value"));
}

#[test]
fn ephemeral_accounts_are_purged_once_expired() {
    let (_temp, paths) = fixture();
    let mut vault = Vault::create(&paths, "password").expect("create");

    let issued = vault
        .create_mcp_token(token(
            vec![McpScope::CreateAndFill],
            vec![AGENTS_MCP_FOLDER_ID],
            Some(AGENTS_MCP_FOLDER_ID),
        ))
        .expect("issue");
    let principal = vault
        .authenticate_mcp_token(&issued.secret)
        .expect("authenticate");
    let created = vault
        .mcp_create_ephemeral(
            &principal,
            EphemeralAccountDraft {
                title: "Expiré".to_owned(),
                primary_url: None,
                username: None,
                ttl_seconds: 60,
                folder_id: None,
            },
        )
        .expect("create");

    // Nothing is expired yet, then the hourly pass sees the TTL as elapsed.
    assert_eq!(vault.purge_expired(0).expect("purge"), 0);
    assert!(vault.get_item(&created.item.id).is_ok());
    let later = created.item.expires_at.expect("ttl") + 1;
    assert_eq!(vault.purge_expired(later).expect("purge"), 1);
    assert!(vault.get_item(&created.item.id).is_err());
}

#[test]
fn token_creation_validates_its_inputs() {
    let (_temp, paths) = fixture();
    let mut vault = Vault::create(&paths, "password").expect("create");

    assert!(matches!(
        vault.create_mcp_token(NewMcpToken {
            name: "  ".to_owned(),
            scopes: vec![McpScope::ReadMetadata],
            folder_ids: vec![PERSONAL_FOLDER_ID.to_owned()],
            default_create_folder_id: None,
            expires_at: None,
        }),
        Err(VaultError::InvalidInput(_))
    ));
    assert!(matches!(
        vault.create_mcp_token(NewMcpToken {
            name: "No scopes".to_owned(),
            scopes: Vec::new(),
            folder_ids: vec![PERSONAL_FOLDER_ID.to_owned()],
            default_create_folder_id: None,
            expires_at: None,
        }),
        Err(VaultError::InvalidInput(_))
    ));
    assert!(matches!(
        vault.create_mcp_token(NewMcpToken {
            name: "Unknown folder".to_owned(),
            scopes: vec![McpScope::ReadMetadata],
            folder_ids: vec!["00000000-0000-4000-8000-00000000ffff".to_owned()],
            default_create_folder_id: None,
            expires_at: None,
        }),
        Err(VaultError::NotFound)
    ));
    assert!(matches!(
        vault.create_mcp_token(NewMcpToken {
            name: "Bad default".to_owned(),
            scopes: vec![McpScope::ReadMetadata],
            folder_ids: vec![DEV_LOCAL_FOLDER_ID.to_owned()],
            default_create_folder_id: Some(PERSONAL_FOLDER_ID.to_owned()),
            expires_at: None,
        }),
        Err(VaultError::InvalidInput(_))
    ));
}
