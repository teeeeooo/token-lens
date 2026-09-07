use crate::antigravity_local;
use crate::domain::{QuotaProvider, QuotaReport, QuotaWindow, QuotaWindowKind, SupportedProvider};
use crate::google_code_assist;
use serde::Deserialize;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const OAUTH_SOURCE: &str = "antigravity-oauth";
const TOKEN_EXPIRY_SAFETY_MS: u64 = 30_000;

#[derive(Debug, Deserialize)]
struct ExternalOAuthCredential {
    #[serde(alias = "accessToken")]
    access_token: Option<String>,
    #[serde(alias = "expiresAt", alias = "expiry_date")]
    expires_at: Option<u64>,
    #[serde(alias = "projectId", alias = "project_id")]
    project_id: Option<String>,
    #[serde(alias = "accountEmail", alias = "account_email")]
    account_email: Option<String>,
}

struct ValidOAuthCredential {
    access_token: String,
    project_id: Option<String>,
    account_email: Option<String>,
}

pub(crate) async fn enrich_quota_report(home: &Path, report: QuotaReport) -> QuotaReport {
    if report
        .providers
        .iter()
        .find(|provider| provider.provider == SupportedProvider::Antigravity)
        .is_some_and(provider_has_usable_quota)
    {
        return report;
    }
    let fallback = report.clone();
    let home = home.to_path_buf();
    tokio::task::spawn_blocking(move || enrich_quota_report_sync(&home, report))
        .await
        .unwrap_or(fallback)
}

fn enrich_quota_report_sync(home: &Path, mut report: QuotaReport) -> QuotaReport {
    let provider = antigravity_local::probe()
        .map(provider_from_local)
        .or_else(|| probe_external_oauth(home));
    let Some(provider) = provider else {
        return report;
    };
    if let Some(existing) = report
        .providers
        .iter_mut()
        .find(|item| item.provider == SupportedProvider::Antigravity)
    {
        *existing = provider;
    } else {
        report.providers.push(provider);
    }
    report
}

fn provider_has_usable_quota(provider: &QuotaProvider) -> bool {
    !provider.windows.is_empty()
}

fn provider_from_local(snapshot: antigravity_local::Snapshot) -> QuotaProvider {
    QuotaProvider {
        provider: SupportedProvider::Antigravity,
        plan: snapshot.account_plan,
        account_email: snapshot.account_email,
        diagnostic: None,
        windows: snapshot.windows,
        reset_credits: None,
        credit_status: None,
        spend_control: None,
    }
}

fn model_family(label: &str, model_id: &str) -> Option<&'static str> {
    let normalized = format!("{label} {model_id}").to_ascii_lowercase();
    if normalized.contains("gemini") {
        Some("Gemini")
    } else if normalized.contains("claude") || normalized.contains("gpt") {
        Some("Claude + GPT")
    } else {
        None
    }
}

fn window_rank(window: &QuotaWindow) -> (u8, u8, String) {
    let family = if window.label.starts_with("Gemini") {
        0
    } else if window.label.starts_with("Claude + GPT") {
        1
    } else {
        2
    };
    let cadence = match window.kind {
        QuotaWindowKind::Session => 0,
        QuotaWindowKind::Weekly => 1,
        _ => 2,
    };
    (family, cadence, window.label.clone())
}

fn probe_external_oauth(_home: &Path) -> Option<QuotaProvider> {
    let credential = read_external_oauth_credential()?;
    let load = google_code_assist::load_antigravity_code_assist(
        &credential.access_token,
        credential.project_id.as_deref(),
    )
    .ok()?;
    let project = load.project_id.as_deref()?;
    let windows =
        google_code_assist::retrieve_user_quota_summary(&credential.access_token, project)
            .ok()
            .map(|summary| antigravity_local::grouped_windows(&summary, OAUTH_SOURCE))
            .filter(|windows| !windows.is_empty())
            .or_else(|| {
                google_code_assist::retrieve_user_quota(&credential.access_token, project)
                    .ok()
                    .map(remote_windows_from_buckets)
                    .filter(|windows| !windows.is_empty())
            })?;
    Some(QuotaProvider {
        provider: SupportedProvider::Antigravity,
        plan: load.plan,
        account_email: credential.account_email,
        diagnostic: None,
        windows,
        reset_credits: None,
        credit_status: None,
        spend_control: None,
    })
}

fn read_external_oauth_credential() -> Option<ValidOAuthCredential> {
    let path = PathBuf::from(env::var("ANTIGRAVITY_OAUTH_CREDENTIALS_FILE").ok()?);
    let metadata = fs::metadata(&path).ok()?;
    if !metadata.is_file() || metadata.len() > 64 * 1024 {
        return None;
    }
    let raw = fs::read(path).ok()?;
    let credential = serde_json::from_slice::<ExternalOAuthCredential>(&raw).ok()?;
    let access_token = credential.access_token?.trim().to_owned();
    if access_token.is_empty() {
        return None;
    }
    let raw_expiry = credential.expires_at?;
    let expiry_ms = if raw_expiry > 20_000_000_000 {
        raw_expiry
    } else {
        raw_expiry.saturating_mul(1000)
    };
    if expiry_ms <= now_ms().saturating_add(TOKEN_EXPIRY_SAFETY_MS) {
        return None;
    }
    Some(ValidOAuthCredential {
        access_token,
        project_id: credential.project_id.and_then(clean_string),
        account_email: credential
            .account_email
            .map(|value| value.to_ascii_lowercase())
            .and_then(clean_string),
    })
}

fn remote_windows_from_buckets(buckets: Vec<google_code_assist::QuotaBucket>) -> Vec<QuotaWindow> {
    let mut families: HashMap<&'static str, (f64, Option<String>)> = HashMap::new();
    for bucket in buckets {
        let model = bucket.model_id.as_deref().unwrap_or("");
        let Some(family) = model_family(model, model) else {
            continue;
        };
        let Some(fraction) = bucket
            .remaining_fraction
            .filter(|value| value.is_finite())
            .map(|value| value.clamp(0.0, 1.0))
        else {
            continue;
        };
        let replace = families
            .get(family)
            .map_or(true, |(current, _)| fraction < *current);
        if replace {
            families.insert(family, (fraction, bucket.reset_time.and_then(clean_string)));
        }
    }
    let mut windows = families
        .into_iter()
        .map(|(family, (fraction, reset))| QuotaWindow {
            kind: QuotaWindowKind::Other,
            label: family.to_owned(),
            metric: "quota",
            additional: false,
            used: None,
            limit: None,
            remaining: None,
            used_percent: Some((1.0 - fraction) * 100.0),
            remaining_percent: Some(fraction * 100.0),
            remaining_label: None,
            resets_at: reset,
            currency: None,
            show_meter: true,
            source: OAUTH_SOURCE,
        })
        .collect::<Vec<_>>();
    windows.sort_by_key(window_rank);
    windows
}

fn clean_string(value: String) -> Option<String> {
    let value = value.trim().to_owned();
    (!value.is_empty()).then_some(value)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_oauth_schema_does_not_deserialize_refresh_tokens() {
        let raw = serde_json::from_str::<ExternalOAuthCredential>(
            r#"{
            "accessToken":"token",
            "expiresAt":9999999999999,
            "projectId":"project",
            "accountEmail":"USER@example.com",
            "refreshToken":"DO_NOT_USE"
        }"#,
        )
        .unwrap();
        assert_eq!(raw.access_token.as_deref(), Some("token"));
        assert_eq!(raw.project_id.as_deref(), Some("project"));
        assert_eq!(raw.account_email.as_deref(), Some("USER@example.com"));
    }

    #[test]
    fn remote_grouped_summary_preserves_cadence_and_oauth_source() {
        let summary = serde_json::json!({ "groups": [{
            "displayName": "Gemini",
            "buckets": [
                { "window": "session", "remainingFraction": 0.2 },
                { "window": "weekly", "remainingFraction": 0.6 }
            ]
        }]});
        let windows = antigravity_local::grouped_windows(&summary, OAUTH_SOURCE);
        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0].kind, QuotaWindowKind::Session);
        assert_eq!(windows[1].kind, QuotaWindowKind::Weekly);
        assert!(windows.iter().all(|window| window.source == OAUTH_SOURCE));
    }

    #[test]
    fn remote_model_buckets_are_conservative_family_windows_without_fake_cadence() {
        let windows = remote_windows_from_buckets(vec![
            google_code_assist::QuotaBucket {
                model_id: Some("gemini-future-pro".into()),
                remaining_fraction: Some(0.4),
                remaining_amount: None,
                reset_time: Some("2026-09-06T00:00:00Z".into()),
            },
            google_code_assist::QuotaBucket {
                model_id: Some("claude-opus-4".into()),
                remaining_fraction: Some(0.2),
                remaining_amount: None,
                reset_time: None,
            },
        ]);
        assert_eq!(windows.len(), 2);
        assert!(windows
            .iter()
            .all(|window| window.kind == QuotaWindowKind::Other));
        assert!(windows.iter().all(|window| window.source == OAUTH_SOURCE));
    }

    #[test]
    fn existing_tokscale_antigravity_quota_remains_authoritative() {
        let provider = QuotaProvider {
            provider: SupportedProvider::Antigravity,
            plan: None,
            account_email: None,
            diagnostic: None,
            windows: vec![QuotaWindow {
                kind: QuotaWindowKind::Weekly,
                label: "Weekly".into(),
                metric: "quota",
                additional: false,
                used: None,
                limit: None,
                remaining: None,
                used_percent: Some(10.0),
                remaining_percent: Some(90.0),
                remaining_label: None,
                resets_at: None,
                currency: None,
                show_meter: true,
                source: "tokscale",
            }],
            reset_credits: None,
            credit_status: None,
            spend_control: None,
        };
        assert!(provider_has_usable_quota(&provider));
    }
}
