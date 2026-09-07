use crate::domain::{QuotaProvider, QuotaReport, QuotaWindow, QuotaWindowKind, SupportedProvider};
use chrono::{DateTime, SecondsFormat};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command};
use tokio::time::{sleep, timeout};

const CODEX_APP_SERVER_SOURCE: &str = "codex-app-server";
const CODEX_OAUTH_SOURCE: &str = "codex-oauth";
const DEFAULT_CODEX_BASE_URL: &str = "https://chatgpt.com/backend-api";
const CODEX_HTTP_TIMEOUT_SECONDS: u64 = 30;
const RPC_TIMEOUT: Duration = Duration::from_secs(20);
const EMPTY_LIMIT_RETRY_DELAY: Duration = Duration::from_millis(300);
const MAX_RPC_LINE_BYTES: usize = 2 * 1024 * 1024;
const MAX_APP_SERVER_STDERR_BYTES: usize = 16 * 1024;
const CODEX_APP_SERVER_ARGS: [&str; 5] = ["-s", "read-only", "-a", "never", "app-server"];

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RateLimitWindow {
    #[serde(alias = "used_percent")]
    used_percent: Option<f64>,
    #[serde(alias = "resets_at", alias = "reset_at")]
    resets_at: Option<Value>,
    #[serde(alias = "window_duration_mins")]
    window_duration_mins: Option<u64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RateLimitSnapshot {
    #[serde(alias = "limit_id")]
    limit_id: Option<String>,
    #[serde(alias = "plan_type")]
    plan_type: Option<String>,
    primary: Option<RateLimitWindow>,
    secondary: Option<RateLimitWindow>,
    #[serde(alias = "individual_limit")]
    individual_limit: Option<IndividualLimit>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RateLimitResponse {
    #[serde(default, alias = "rate_limits")]
    rate_limits: RateLimitSnapshot,
    #[serde(alias = "rate_limits_by_limit_id")]
    rate_limits_by_limit_id: Option<HashMap<String, RateLimitSnapshot>>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IndividualLimit {
    limit: NumberLike,
    used: NumberLike,
    #[serde(default, alias = "remaining_percent")]
    remaining_percent: Option<NumberLike>,
    #[serde(default, alias = "resets_at")]
    resets_at: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged)]
enum NumberLike {
    Text(String),
    Number(f64),
}

impl NumberLike {
    fn as_f64(&self) -> Option<f64> {
        let value = match self {
            Self::Text(value) => value.parse::<f64>().ok()?,
            Self::Number(value) => *value,
        };
        value.is_finite().then_some(value)
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccountReadResponse {
    account: Option<AccountInfo>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccountInfo {
    email: Option<String>,
    #[serde(alias = "plan_type")]
    plan_type: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct AppServerSnapshot {
    account_email: Option<String>,
    account_plan: Option<String>,
    rate_plan: Option<String>,
    primary: Option<RateLimitWindow>,
    secondary: Option<RateLimitWindow>,
    individual_limit: Option<IndividualLimit>,
}

#[derive(Debug, Default, Deserialize)]
struct StoredCodexAuth {
    tokens: Option<StoredCodexTokens>,
    #[serde(alias = "accessToken")]
    access_token: Option<String>,
    #[serde(alias = "idToken")]
    id_token: Option<String>,
    #[serde(alias = "accountId")]
    account_id: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct StoredCodexTokens {
    #[serde(alias = "accessToken")]
    access_token: Option<String>,
    #[serde(alias = "idToken")]
    id_token: Option<String>,
    #[serde(alias = "accountId")]
    account_id: Option<String>,
}

#[derive(Debug, Clone)]
struct CodexOauthContext {
    access_token: String,
    account_id: Option<String>,
    fedramp: bool,
}

#[derive(Debug, Deserialize)]
struct RpcEnvelope {
    id: Option<u64>,
    result: Option<Value>,
    error: Option<RpcError>,
}

#[derive(Debug, Deserialize)]
struct RpcError {
    message: Option<String>,
}

pub(crate) fn selected_workspace_id(home: &Path) -> Option<String> {
    let path = codex_home(home).join("auth.json");
    let auth = serde_json::from_slice::<StoredCodexAuth>(&fs::read(path).ok()?).ok()?;
    clean_identity(
        auth.tokens
            .and_then(|tokens| tokens.account_id)
            .or(auth.account_id),
    )
}

pub(crate) async fn enrich_quota_report(
    home: &Path,
    expected_workspace_id: Option<String>,
    mut report: QuotaReport,
) -> QuotaReport {
    let Some(index) = report
        .providers
        .iter()
        .position(|provider| provider.provider == SupportedProvider::Codex)
    else {
        return report;
    };

    if !provider_needs_enrichment(&report.providers[index]) {
        return report;
    }

    let before_workspace = selected_workspace_id(home);
    if before_workspace != expected_workspace_id {
        return report;
    }

    let mut diagnostics = Vec::new();
    if provider_needs_base_quota(&report.providers[index]) {
        let oauth_home = home.to_path_buf();
        match tokio::task::spawn_blocking(move || read_oauth_usage_snapshot(&oauth_home)).await {
            Ok(Ok(snapshot)) => {
                if selected_workspace_id(home) == expected_workspace_id {
                    apply_base_rate_limits(
                        &mut report.providers[index],
                        &snapshot,
                        CODEX_OAUTH_SOURCE,
                    );
                    if report.providers[index].plan.is_none() {
                        report.providers[index].plan = snapshot.rate_plan;
                    }
                } else {
                    diagnostics.push("Codex OAuth: workspace changed during refresh".to_owned());
                }
            }
            Ok(Err(error)) => diagnostics.push(error),
            Err(_) => diagnostics.push("Codex OAuth quota task failed".to_owned()),
        }
    }

    if !provider_needs_enrichment(&report.providers[index]) {
        report.providers[index].diagnostic = None;
        return report;
    }

    let expected_email = report.providers[index].account_email.clone();
    let snapshot = match read_app_server_snapshot().await {
        Ok(snapshot) => snapshot,
        Err(error) => {
            diagnostics.push(error);
            report.providers[index].diagnostic = Some(diagnostics.join(" · "));
            return report;
        }
    };

    let after_workspace = selected_workspace_id(home);
    if !enrichment_context_matches(
        expected_workspace_id.as_deref(),
        before_workspace.as_deref(),
        after_workspace.as_deref(),
        expected_email.as_deref(),
        snapshot.account_email.as_deref(),
    ) {
        diagnostics.push("Codex App Server: account/workspace mismatch".to_owned());
        report.providers[index].diagnostic = Some(diagnostics.join(" · "));
        return report;
    }

    apply_base_rate_limits(
        &mut report.providers[index],
        &snapshot,
        CODEX_APP_SERVER_SOURCE,
    );

    let business_plan = report.providers[index]
        .plan
        .as_deref()
        .or(snapshot.rate_plan.as_deref())
        .or(snapshot.account_plan.as_deref())
        .is_some_and(business_like_plan);
    if business_plan {
        if let Some(individual_limit) = snapshot.individual_limit.as_ref() {
            if !apply_individual_limit(&mut report.providers[index], individual_limit) {
                diagnostics
                    .push("Codex App Server: Business monthly limit was unusable".to_owned());
            }
        } else if provider_needs_individual_limit(&report.providers[index]) {
            diagnostics.push("Codex App Server: Business monthly limit was absent".to_owned());
        }
    }
    report.providers[index].diagnostic = if diagnostics.is_empty() {
        None
    } else {
        Some(diagnostics.join(" · "))
    };
    report
}

fn provider_needs_enrichment(provider: &QuotaProvider) -> bool {
    provider_needs_base_quota(provider) || provider_needs_individual_limit(provider)
}

fn provider_needs_base_quota(provider: &QuotaProvider) -> bool {
    !provider.windows.iter().any(|window| {
        !window.additional
            && matches!(
                window.kind,
                QuotaWindowKind::Session | QuotaWindowKind::Daily | QuotaWindowKind::Weekly
            )
            && window.remaining_percent.is_some()
    })
}

fn provider_needs_individual_limit(provider: &QuotaProvider) -> bool {
    if !provider.plan.as_deref().is_some_and(business_like_plan) {
        return false;
    }
    !provider.windows.iter().any(|window| {
        window.kind == QuotaWindowKind::Billing
            && window.metric.eq_ignore_ascii_case("credits")
            && window.used.is_some()
            && window.limit.is_some()
    })
}

fn business_like_plan(value: &str) -> bool {
    let normalized = value.trim().to_ascii_lowercase();
    normalized == "team" || normalized.contains("business") || normalized.contains("enterprise")
}

fn enrichment_context_matches(
    expected_workspace: Option<&str>,
    before_workspace: Option<&str>,
    after_workspace: Option<&str>,
    expected_email: Option<&str>,
    rpc_email: Option<&str>,
) -> bool {
    if normalize_identity(expected_workspace) != normalize_identity(before_workspace)
        || normalize_identity(expected_workspace) != normalize_identity(after_workspace)
    {
        return false;
    }

    let expected_email = normalize_identity(expected_email);
    let rpc_email = normalize_identity(rpc_email);
    !matches!((expected_email, rpc_email), (Some(left), Some(right)) if left != right)
}

fn apply_base_rate_limits(
    provider: &mut QuotaProvider,
    snapshot: &AppServerSnapshot,
    source: &'static str,
) -> usize {
    let mut added = 0;
    for (slot, raw) in [
        ("primary", snapshot.primary.as_ref()),
        ("secondary", snapshot.secondary.as_ref()),
    ] {
        let Some(raw) = raw else {
            continue;
        };
        let Some(used_percent) = raw.used_percent.filter(|value| value.is_finite()) else {
            continue;
        };
        let kind = rate_window_kind(slot, raw.window_duration_mins);
        if provider.windows.iter().any(|window| {
            !window.additional && window.kind == kind && window.remaining_percent.is_some()
        }) {
            continue;
        }
        let used_percent = used_percent.clamp(0.0, 100.0);
        let window = QuotaWindow {
            kind,
            label: rate_window_label(kind).to_owned(),
            metric: "quota",
            additional: false,
            used: None,
            limit: None,
            remaining: None,
            used_percent: Some(used_percent),
            remaining_percent: Some(100.0 - used_percent),
            remaining_label: None,
            resets_at: raw.resets_at.as_ref().and_then(rpc_reset_to_rfc3339),
            currency: None,
            show_meter: true,
            source,
        };
        let insert_at = provider
            .windows
            .iter()
            .position(|candidate| candidate.additional)
            .unwrap_or(provider.windows.len());
        provider.windows.insert(insert_at, window);
        added += 1;
    }
    added
}

fn rate_window_kind(slot: &str, minutes: Option<u64>) -> QuotaWindowKind {
    match minutes.unwrap_or_default() {
        value if value == 30 * 24 * 60 => QuotaWindowKind::Billing,
        value if value >= 7 * 24 * 60 => QuotaWindowKind::Weekly,
        value if value >= 24 * 60 => QuotaWindowKind::Daily,
        value if value == 5 * 60 => QuotaWindowKind::Session,
        _ if slot.eq_ignore_ascii_case("secondary") => QuotaWindowKind::Weekly,
        _ => QuotaWindowKind::Session,
    }
}

fn rate_window_label(kind: QuotaWindowKind) -> &'static str {
    match kind {
        QuotaWindowKind::Session => "5h",
        QuotaWindowKind::Daily => "Daily",
        QuotaWindowKind::Weekly => "Weekly",
        QuotaWindowKind::Billing => "Monthly",
        QuotaWindowKind::Other => "Quota",
    }
}

fn rpc_reset_to_rfc3339(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => {
            let value = value.trim();
            if value.is_empty() {
                None
            } else if let Ok(epoch) = value.parse::<i64>() {
                epoch_seconds_to_rfc3339(epoch)
            } else {
                Some(value.to_owned())
            }
        }
        Value::Number(value) => value.as_i64().and_then(epoch_seconds_to_rfc3339),
        _ => None,
    }
}

fn apply_individual_limit(provider: &mut QuotaProvider, raw: &IndividualLimit) -> bool {
    let Some(limit) = raw.limit.as_f64() else {
        return false;
    };
    let Some(used) = raw.used.as_f64() else {
        return false;
    };
    if limit <= 0.0 || used < 0.0 {
        return false;
    }

    let remaining_percent = raw
        .remaining_percent
        .as_ref()
        .and_then(NumberLike::as_f64)
        .unwrap_or_else(|| ((limit - used).max(0.0) / limit * 100.0).clamp(0.0, 100.0))
        .clamp(0.0, 100.0);
    let used_percent = (100.0 - remaining_percent).clamp(0.0, 100.0);
    let window = QuotaWindow {
        kind: QuotaWindowKind::Billing,
        label: "Monthly".to_owned(),
        metric: "credits",
        additional: false,
        used: Some(used),
        limit: Some(limit),
        remaining: Some((limit - used).max(0.0)),
        used_percent: Some(used_percent),
        remaining_percent: Some(remaining_percent),
        remaining_label: None,
        resets_at: raw.resets_at.as_ref().and_then(rpc_reset_to_rfc3339),
        currency: Some("CREDITS".to_owned()),
        show_meter: true,
        source: CODEX_APP_SERVER_SOURCE,
    };

    provider
        .windows
        .retain(|candidate| !(candidate.kind == QuotaWindowKind::Billing && !candidate.additional));
    let insert_at = provider
        .windows
        .iter()
        .position(|candidate| candidate.additional)
        .unwrap_or(provider.windows.len());
    provider.windows.insert(insert_at, window);
    true
}

fn epoch_seconds_to_rfc3339(value: i64) -> Option<String> {
    DateTime::from_timestamp(value, 0)
        .map(|timestamp| timestamp.to_rfc3339_opts(SecondsFormat::Secs, true))
}

fn codex_home(home: &Path) -> PathBuf {
    env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| home.join(".codex"))
}

fn clean_identity(value: Option<String>) -> Option<String> {
    normalize_identity(value.as_deref())
}

fn normalize_identity(value: Option<&str>) -> Option<String> {
    let value = value?.trim().to_ascii_lowercase();
    (!value.is_empty()).then_some(value)
}

fn read_oauth_usage_snapshot(home: &Path) -> Result<AppServerSnapshot, String> {
    let codex_dir = codex_home(home);
    let auth = read_oauth_context(&codex_dir.join("auth.json"))?;
    let base_url = read_chatgpt_base_url(&codex_dir.join("config.toml"));
    let endpoint = codex_usage_endpoint(&base_url);
    let mut request = minreq::get(endpoint)
        .with_header("accept", "application/json")
        .with_header("authorization", format!("Bearer {}", auth.access_token))
        .with_header("user-agent", "Token-Lens/2")
        .with_timeout(CODEX_HTTP_TIMEOUT_SECONDS)
        .with_follow_redirects(false);
    if let Some(account_id) = auth.account_id.as_deref() {
        request = request.with_header("chatgpt-account-id", account_id);
    }
    if auth.fedramp {
        request = request.with_header("x-openai-fedramp", "true");
    }
    let response = request.send().map_err(|error| {
        format!(
            "Codex OAuth usage request failed ({})",
            crate::http_diagnostic::transport_category(&error)
        )
    })?;
    if !(200..300).contains(&response.status_code) {
        return Err(format!(
            "Codex OAuth usage returned HTTP {}",
            response.status_code
        ));
    }
    let payload = response
        .json::<Value>()
        .map_err(|_| "Codex OAuth usage returned an invalid payload".to_owned())?;
    oauth_snapshot_from_payload(&payload)
}

fn read_oauth_context(path: &Path) -> Result<CodexOauthContext, String> {
    let auth = serde_json::from_slice::<StoredCodexAuth>(
        &fs::read(path).map_err(|_| "Codex auth.json is unavailable".to_owned())?,
    )
    .map_err(|_| "Codex auth.json has an unexpected shape".to_owned())?;
    let tokens = auth.tokens.as_ref();
    let access_token = tokens
        .and_then(|value| value.access_token.clone())
        .or(auth.access_token)
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Codex access token is unavailable".to_owned())?;
    let id_token = tokens
        .and_then(|value| value.id_token.as_deref())
        .or(auth.id_token.as_deref());
    let claims = id_token.and_then(jwt_payload);
    let (claimed_account, claimed_fedramp) = claims
        .as_ref()
        .map(codex_claim_identity)
        .unwrap_or((None, None));
    let stored_account = tokens
        .and_then(|value| value.account_id.clone())
        .or(auth.account_id)
        .and_then(|value| clean_identity(Some(value)));
    let account_id = stored_account.or(claimed_account.clone());
    let fedramp =
        account_id.is_some() && account_id == claimed_account && claimed_fedramp == Some(true);
    Ok(CodexOauthContext {
        access_token,
        account_id,
        fedramp,
    })
}

fn codex_claim_identity(claims: &Value) -> (Option<String>, Option<bool>) {
    let nested = claims
        .get("https://api.openai.com/auth")
        .or_else(|| claims.get("https://api.openai.com/profile"));
    let account_id = claims
        .get("chatgpt_account_id")
        .and_then(Value::as_str)
        .or_else(|| {
            nested
                .and_then(|value| value.get("chatgpt_account_id"))
                .and_then(Value::as_str)
        })
        .and_then(|value| normalize_identity(Some(value)));
    let fedramp = nested
        .and_then(|value| value.get("chatgpt_account_is_fedramp"))
        .and_then(Value::as_bool)
        .or_else(|| {
            claims
                .get("chatgpt_account_is_fedramp")
                .and_then(Value::as_bool)
        });
    (account_id, fedramp)
}

fn read_chatgpt_base_url(path: &Path) -> String {
    let Ok(text) = fs::read_to_string(path) else {
        return DEFAULT_CODEX_BASE_URL.to_owned();
    };
    for raw_line in text.lines() {
        let line = raw_line.split('#').next().unwrap_or("").trim();
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() != "chatgpt_base_url" {
            continue;
        }
        let value = value.trim().trim_matches(['\'', '"']);
        if value.starts_with("https://") {
            return normalize_chatgpt_base_url(value);
        }
    }
    DEFAULT_CODEX_BASE_URL.to_owned()
}

fn normalize_chatgpt_base_url(value: &str) -> String {
    let mut base = value.trim().trim_end_matches('/').to_owned();
    if matches!(
        base.to_ascii_lowercase().as_str(),
        "https://chatgpt.openai.com" | "https://chat.openai.com" | "https://chatgpt.com"
    ) {
        base.push_str("/backend-api");
    }
    base
}

fn codex_usage_endpoint(base_url: &str) -> String {
    let base = normalize_chatgpt_base_url(base_url);
    if base.to_ascii_lowercase().contains("/backend-api") {
        format!("{base}/wham/usage")
    } else {
        format!("{base}/api/codex/usage")
    }
}

fn oauth_snapshot_from_payload(payload: &Value) -> Result<AppServerSnapshot, String> {
    let rate_limit = payload
        .get("rateLimit")
        .or_else(|| payload.get("rate_limit"))
        .ok_or_else(|| "Codex OAuth usage did not include rate limits".to_owned())?;
    let primary = rate_limit
        .get("primaryWindow")
        .or_else(|| rate_limit.get("primary_window"))
        .and_then(oauth_rate_window);
    let secondary = rate_limit
        .get("secondaryWindow")
        .or_else(|| rate_limit.get("secondary_window"))
        .and_then(oauth_rate_window);
    if primary.is_none() && secondary.is_none() {
        return Err("Codex OAuth usage did not include usable quota windows".to_owned());
    }
    Ok(AppServerSnapshot {
        rate_plan: string_alias(payload, &["planType", "plan_type"]),
        primary,
        secondary,
        ..AppServerSnapshot::default()
    })
}

fn oauth_rate_window(raw: &Value) -> Option<RateLimitWindow> {
    let used_percent = number_alias(raw, &["usedPercent", "used_percent"])?;
    let minutes = number_alias(raw, &["limitWindowSeconds", "limit_window_seconds"])
        .filter(|value| *value >= 0.0)
        .map(|seconds| (seconds / 60.0).round() as u64);
    let resets_at = raw
        .get("resetsAt")
        .or_else(|| raw.get("resetAt"))
        .or_else(|| raw.get("reset_at"))
        .cloned()
        .or_else(|| reset_after_value(raw));
    Some(RateLimitWindow {
        used_percent: Some(used_percent),
        resets_at,
        window_duration_mins: minutes,
    })
}

fn string_alias(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| value.get(*key))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn number_alias(value: &Value, keys: &[&str]) -> Option<f64> {
    let value = keys.iter().find_map(|key| value.get(*key))?;
    match value {
        Value::Number(value) => value.as_f64().filter(|value| value.is_finite()),
        Value::String(value) => value.parse::<f64>().ok().filter(|value| value.is_finite()),
        _ => None,
    }
}

fn reset_after_value(raw: &Value) -> Option<Value> {
    let seconds = number_alias(raw, &["resetAfterSeconds", "reset_after_seconds"])?;
    if !seconds.is_finite() || seconds < 0.0 {
        return None;
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_secs_f64();
    Some(Value::from((now + seconds).round() as i64))
}

fn jwt_payload(token: &str) -> Option<Value> {
    let payload = token.split('.').nth(1)?;
    let bytes = decode_base64url(payload)?;
    serde_json::from_slice(&bytes).ok()
}

fn decode_base64url(value: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(value.len() * 3 / 4);
    let mut buffer = 0u32;
    let mut bits = 0u8;
    for byte in value.bytes() {
        if byte == b'=' {
            break;
        }
        let digit = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'-' => 62,
            b'_' => 63,
            _ => return None,
        } as u32;
        buffer = (buffer << 6) | digit;
        bits += 6;
        while bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
            buffer &= (1u32 << bits).saturating_sub(1);
        }
    }
    Some(out)
}

async fn read_app_server_snapshot() -> Result<AppServerSnapshot, String> {
    let candidates = codex_command_candidates();
    let mut last_error = None;
    for candidate in candidates {
        match read_app_server_with_command(&candidate).await {
            Ok(snapshot) => return Ok(snapshot),
            Err(error) => last_error = Some(error),
        }
    }
    Err(last_error.unwrap_or_else(|| "Codex CLI not found".to_owned()))
}

async fn read_app_server_with_command(command: &Path) -> Result<AppServerSnapshot, String> {
    let mut child = spawn_app_server(command)?;
    let stderr_task = child
        .stderr
        .take()
        .map(|stderr| tokio::spawn(read_app_server_stderr_hint(stderr)));
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| "Codex App Server stdin unavailable".to_owned())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Codex App Server stdout unavailable".to_owned())?;
    let mut reader = BufReader::new(stdout);

    let result = async {
        let _: Value = rpc_call(
            &mut stdin,
            &mut reader,
            1,
            "initialize",
            Some(json!({
                "clientInfo": {
                    "name": "token-lens",
                    "title": "Token Lens",
                    "version": env!("CARGO_PKG_VERSION")
                }
            })),
        )
        .await?;
        rpc_notify(&mut stdin, "initialized", Some(json!({}))).await?;

        // Preserve the proven v1 order: quota first, account identity second. The
        // Business monthly limit can be available before account/read settles.
        let mut rates = rpc_call::<RateLimitResponse>(
            &mut stdin,
            &mut reader,
            2,
            "account/rateLimits/read",
            None,
        )
        .await?;

        let account = rpc_call::<AccountReadResponse>(
            &mut stdin,
            &mut reader,
            3,
            "account/read",
            Some(json!({ "refreshToken": false })),
        )
        .await
        .ok()
        .and_then(|response| response.account);

        let mut selected = canonical_rate_snapshot(&rates).unwrap_or_default();
        let plan_hint = selected.plan_type.as_deref().or(account
            .as_ref()
            .and_then(|value| value.plan_type.as_deref()));
        let empty_base = selected.primary.is_none() && selected.secondary.is_none();
        let missing_business_limit =
            selected.individual_limit.is_none() && plan_hint.is_some_and(business_like_plan);
        if empty_base || missing_business_limit {
            sleep(EMPTY_LIMIT_RETRY_DELAY).await;
            if let Ok(retry) = rpc_call::<RateLimitResponse>(
                &mut stdin,
                &mut reader,
                4,
                "account/rateLimits/read",
                None,
            )
            .await
            {
                rates = retry;
                selected = canonical_rate_snapshot(&rates).unwrap_or_default();
            }
        }

        Ok(AppServerSnapshot {
            account_email: account.as_ref().and_then(|value| value.email.clone()),
            account_plan: account.and_then(|value| value.plan_type),
            rate_plan: selected.plan_type,
            primary: selected.primary,
            secondary: selected.secondary,
            individual_limit: selected.individual_limit,
        })
    }
    .await;

    drop(stdin);
    drop(reader);
    terminate_child(&mut child).await;
    let hint = match stderr_task {
        Some(task) => task.await.ok().flatten(),
        None => None,
    };
    match result {
        Ok(snapshot) => Ok(snapshot),
        Err(error) => Err(match hint {
            Some(hint) => format!("{error} · {hint}"),
            None => error,
        }),
    }
}

async fn read_app_server_stderr_hint(mut stderr: ChildStderr) -> Option<&'static str> {
    let mut sample = Vec::with_capacity(MAX_APP_SERVER_STDERR_BYTES);
    let mut buffer = [0_u8; 4096];
    loop {
        let bytes = stderr.read(&mut buffer).await.ok()?;
        if bytes == 0 {
            break;
        }
        if sample.len() < MAX_APP_SERVER_STDERR_BYTES {
            let remaining = MAX_APP_SERVER_STDERR_BYTES - sample.len();
            sample.extend_from_slice(&buffer[..bytes.min(remaining)]);
        }
    }
    classify_app_server_stderr(&String::from_utf8_lossy(&sample))
}

fn classify_app_server_stderr(stderr: &str) -> Option<&'static str> {
    let stderr = stderr.to_ascii_lowercase();
    if stderr.contains("--ask-for-approval")
        && (stderr.contains("invalid value") || stderr.contains("possible values"))
    {
        return Some("Codex App Server launch policy was rejected by the installed Codex CLI");
    }
    if stderr.contains("unknown argument")
        || stderr.contains("unexpected argument")
        || stderr.contains("unrecognized option")
    {
        return Some("Codex App Server launch arguments were rejected by the installed Codex CLI");
    }
    None
}

fn canonical_rate_snapshot(response: &RateLimitResponse) -> Option<RateLimitSnapshot> {
    if let Some(by_id) = response.rate_limits_by_limit_id.as_ref() {
        if let Some(codex) = by_id.get("codex").or_else(|| {
            by_id.values().find(|snapshot| {
                snapshot
                    .limit_id
                    .as_deref()
                    .is_some_and(|value| value.eq_ignore_ascii_case("codex"))
            })
        }) {
            // An explicit canonical bucket is authoritative even when empty.
            return Some(codex.clone());
        }
    }

    if rate_snapshot_has_quota_data(&response.rate_limits) {
        return Some(response.rate_limits.clone());
    }

    alternate_rate_snapshot_consensus(response).or_else(|| Some(response.rate_limits.clone()))
}

fn rate_snapshot_has_quota_data(snapshot: &RateLimitSnapshot) -> bool {
    snapshot.primary.is_some()
        || snapshot.secondary.is_some()
        || snapshot.individual_limit.is_some()
}

fn normalized_plan(value: Option<&str>) -> Option<String> {
    let value = value?.trim().to_ascii_lowercase();
    (!value.is_empty()).then_some(value)
}

fn alternate_rate_snapshot_consensus(response: &RateLimitResponse) -> Option<RateLimitSnapshot> {
    let by_id = response.rate_limits_by_limit_id.as_ref()?;
    let candidates = by_id
        .iter()
        .filter(|(id, snapshot)| {
            !id.eq_ignore_ascii_case("codex")
                && !snapshot
                    .limit_id
                    .as_deref()
                    .is_some_and(|value| value.eq_ignore_ascii_case("codex"))
                && (snapshot.primary.is_some() || snapshot.secondary.is_some())
        })
        .map(|(_, snapshot)| snapshot)
        .collect::<Vec<_>>();
    let first = *candidates.first()?;
    if !candidates
        .iter()
        .all(|snapshot| snapshot.primary == first.primary && snapshot.secondary == first.secondary)
    {
        return None;
    }

    let plan_type = candidates
        .iter()
        .all(|snapshot| {
            normalized_plan(snapshot.plan_type.as_deref())
                == normalized_plan(first.plan_type.as_deref())
        })
        .then(|| first.plan_type.clone())
        .flatten();
    let individual_limit = candidates
        .iter()
        .all(|snapshot| snapshot.individual_limit == first.individual_limit)
        .then(|| first.individual_limit.clone())
        .flatten();
    Some(RateLimitSnapshot {
        limit_id: None,
        plan_type,
        primary: first.primary.clone(),
        secondary: first.secondary.clone(),
        individual_limit,
    })
}

fn spawn_app_server(command: &Path) -> Result<Child, String> {
    let mut process = app_server_command(command);
    process
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    crate::background_process::configure_tokio(&mut process);
    process
        .spawn()
        .map_err(|error| format!("Codex App Server launch failed: {error}"))
}

#[cfg(not(target_os = "windows"))]
fn app_server_command(command: &Path) -> Command {
    let mut process = Command::new(command);
    process.args(CODEX_APP_SERVER_ARGS);
    process
}

#[cfg(target_os = "windows")]
fn app_server_command(command: &Path) -> Command {
    let is_script = command
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| {
            value.eq_ignore_ascii_case("cmd") || value.eq_ignore_ascii_case("bat")
        });
    if !is_script {
        let mut process = Command::new(command);
        process.args(CODEX_APP_SERVER_ARGS);
        return process;
    }

    let command = quote_windows_cmd_arg(&command.to_string_lossy());
    let command_line = format!("{command} {}", CODEX_APP_SERVER_ARGS.join(" "));
    let mut process = Command::new("cmd.exe");
    process.args(["/d", "/s", "/c"]).arg(command_line);
    process
}

#[cfg(target_os = "windows")]
fn quote_windows_cmd_arg(value: &str) -> String {
    if value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || b"_./:=\\-".contains(&byte))
    {
        return value.to_owned();
    }
    format!("\"{}\"", value.replace('"', "\\\""))
}

async fn terminate_child(child: &mut Child) {
    let _ = child.kill().await;
    let _ = child.wait().await;
}

async fn rpc_notify(
    stdin: &mut ChildStdin,
    method: &str,
    params: Option<Value>,
) -> Result<(), String> {
    let message = match params {
        Some(params) => json!({ "method": method, "params": params }),
        None => json!({ "method": method }),
    };
    write_rpc_line(stdin, &message).await
}

async fn rpc_call<T: DeserializeOwned>(
    stdin: &mut ChildStdin,
    reader: &mut BufReader<ChildStdout>,
    id: u64,
    method: &str,
    params: Option<Value>,
) -> Result<T, String> {
    let request = match params {
        Some(params) => json!({ "id": id, "method": method, "params": params }),
        None => json!({ "id": id, "method": method }),
    };
    write_rpc_line(stdin, &request).await?;
    let result = timeout(RPC_TIMEOUT, read_rpc_result(reader, id))
        .await
        .map_err(|_| format!("Codex App Server {method} timed out"))??;
    serde_json::from_value(result)
        .map_err(|error| format!("Codex App Server {method} returned an unexpected shape: {error}"))
}

async fn write_rpc_line(stdin: &mut ChildStdin, value: &Value) -> Result<(), String> {
    let mut bytes = serde_json::to_vec(value)
        .map_err(|error| format!("failed to serialize Codex App Server request: {error}"))?;
    bytes.push(b'\n');
    stdin
        .write_all(&bytes)
        .await
        .map_err(|error| format!("failed to write Codex App Server request: {error}"))?;
    stdin
        .flush()
        .await
        .map_err(|error| format!("failed to flush Codex App Server request: {error}"))
}

async fn read_rpc_result(reader: &mut BufReader<ChildStdout>, id: u64) -> Result<Value, String> {
    loop {
        let mut line = String::new();
        let bytes = reader
            .read_line(&mut line)
            .await
            .map_err(|error| format!("failed to read Codex App Server response: {error}"))?;
        if bytes == 0 {
            return Err("Codex App Server exited before returning a response".to_owned());
        }
        if line.len() > MAX_RPC_LINE_BYTES {
            return Err("Codex App Server response exceeded the size limit".to_owned());
        }
        let Ok(message) = serde_json::from_str::<RpcEnvelope>(line.trim()) else {
            continue;
        };
        if message.id != Some(id) {
            continue;
        }
        if let Some(error) = message.error {
            return Err(error
                .message
                .unwrap_or_else(|| "Codex App Server returned an RPC error".to_owned()));
        }
        return message
            .result
            .ok_or_else(|| "Codex App Server response did not include a result".to_owned());
    }
}

fn codex_command_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(path) = env::var_os("TOKEN_LENS_CODEX_BIN") {
        push_candidate(&mut candidates, PathBuf::from(path));
    }

    #[cfg(target_os = "macos")]
    {
        push_candidate(
            &mut candidates,
            PathBuf::from("/Applications/Codex.app/Contents/Resources/codex"),
        );
        push_candidate(
            &mut candidates,
            PathBuf::from("/Applications/ChatGPT.app/Contents/Resources/codex"),
        );
    }

    #[cfg(target_os = "windows")]
    {
        add_windows_candidates(&mut candidates);
        push_candidate(&mut candidates, PathBuf::from("codex.cmd"));
        push_candidate(&mut candidates, PathBuf::from("codex.exe"));
    }

    push_candidate(&mut candidates, PathBuf::from("codex"));
    candidates
        .into_iter()
        .filter(|path| !path.is_absolute() || path.is_file())
        .collect()
}

fn push_candidate(candidates: &mut Vec<PathBuf>, candidate: PathBuf) {
    if candidate.as_os_str().is_empty() || candidates.contains(&candidate) {
        return;
    }
    candidates.push(candidate);
}

#[cfg(target_os = "windows")]
fn add_windows_candidates(candidates: &mut Vec<PathBuf>) {
    if let Some(local) = env::var_os("LOCALAPPDATA").map(PathBuf::from) {
        push_candidate(candidates, local.join("Programs/Codex/resources/codex.exe"));
        push_candidate(candidates, local.join("Programs/Codex/Codex.exe"));
        let bin = local.join("OpenAI/Codex/bin");
        add_codex_bin_candidates(candidates, &bin);
        let packages = local.join("Packages");
        if let Ok(entries) = fs::read_dir(packages) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with("OpenAI.Codex_")
                    && entry.file_type().is_ok_and(|kind| kind.is_dir())
                {
                    add_codex_bin_candidates(
                        candidates,
                        &entry.path().join("LocalCache/Local/OpenAI/Codex/bin"),
                    );
                }
            }
        }
        push_candidate(candidates, local.join("Microsoft/WindowsApps/codex.exe"));
        push_candidate(candidates, local.join("Microsoft/WindowsApps/Codex.exe"));
    }
    if let Some(app_data) = env::var_os("APPDATA").map(PathBuf::from) {
        push_candidate(candidates, app_data.join("npm/codex.cmd"));
    }
    for root in ["ProgramFiles", "ProgramW6432", "ProgramFiles(x86)"] {
        if let Some(program_files) = env::var_os(root).map(PathBuf::from) {
            push_candidate(candidates, program_files.join("Codex/resources/codex.exe"));
            let windows_apps = program_files.join("WindowsApps");
            if let Ok(entries) = fs::read_dir(windows_apps) {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if !name.starts_with("OpenAI.Codex_")
                        || !entry.file_type().is_ok_and(|kind| kind.is_dir())
                    {
                        continue;
                    }
                    push_candidate(candidates, entry.path().join("app/resources/codex.exe"));
                    push_candidate(candidates, entry.path().join("app/Codex.exe"));
                }
            }
        }
    }
}

#[cfg(target_os = "windows")]
fn add_codex_bin_candidates(candidates: &mut Vec<PathBuf>, bin: &Path) {
    push_candidate(candidates, bin.join("codex.exe"));
    if let Ok(entries) = fs::read_dir(bin) {
        for entry in entries.flatten() {
            if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                push_candidate(candidates, entry.path().join("codex.exe"));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{QuotaWindow, ResetCredits};

    fn provider(plan: &str) -> QuotaProvider {
        QuotaProvider {
            provider: SupportedProvider::Codex,
            plan: Some(plan.to_owned()),
            account_email: Some("user@example.test".to_owned()),
            diagnostic: None,
            windows: vec![
                QuotaWindow {
                    kind: QuotaWindowKind::Session,
                    label: "5h".to_owned(),
                    metric: "quota",
                    additional: false,
                    used: None,
                    limit: None,
                    remaining: None,
                    used_percent: Some(15.0),
                    remaining_percent: Some(85.0),
                    remaining_label: None,
                    resets_at: Some("2026-09-05T06:00:00Z".to_owned()),
                    currency: None,
                    show_meter: true,
                    source: "tokscale",
                },
                QuotaWindow {
                    kind: QuotaWindowKind::Billing,
                    label: "Monthly".to_owned(),
                    metric: "quota",
                    additional: false,
                    used: None,
                    limit: None,
                    remaining: None,
                    used_percent: Some(40.0),
                    remaining_percent: Some(60.0),
                    remaining_label: None,
                    resets_at: Some("2026-10-01T00:00:00Z".to_owned()),
                    currency: None,
                    show_meter: true,
                    source: "tokscale",
                },
                QuotaWindow {
                    kind: QuotaWindowKind::Weekly,
                    label: "GPT Reserve weekly".to_owned(),
                    metric: "quota",
                    additional: true,
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
                },
            ],
            reset_credits: Some(ResetCredits {
                available_count: 1,
                next_expires_at: None,
                expirations: Vec::new(),
            }),
            credit_status: None,
            spend_control: None,
        }
    }

    fn limit() -> IndividualLimit {
        IndividualLimit {
            limit: NumberLike::Text("750".to_owned()),
            used: NumberLike::Text("432.762320022503".to_owned()),
            remaining_percent: Some(NumberLike::Text("42".to_owned())),
            resets_at: Some(json!("1790812800")),
        }
    }

    #[test]
    fn app_server_launch_uses_supported_noninteractive_approval_policy() {
        assert_eq!(
            CODEX_APP_SERVER_ARGS,
            ["-s", "read-only", "-a", "never", "app-server"]
        );
        assert!(!CODEX_APP_SERVER_ARGS.contains(&"untrusted"));
    }

    #[test]
    fn app_server_stderr_is_classified_without_echoing_raw_content() {
        let retired_policy =
            "error: invalid value 'untrusted' for '--ask-for-approval <APPROVAL_POLICY>'";
        assert_eq!(
            classify_app_server_stderr(retired_policy),
            Some("Codex App Server launch policy was rejected by the installed Codex CLI")
        );
        assert_eq!(
            classify_app_server_stderr("Authorization: Bearer secret-value"),
            None
        );
    }

    #[test]
    fn business_plan_gate_is_narrow_but_future_business_labels_remain_compatible() {
        assert!(business_like_plan("Business"));
        assert!(business_like_plan("self_serve_business_usage_based"));
        assert!(business_like_plan("enterprise_cbp_usage_based"));
        assert!(business_like_plan("team"));
        assert!(!business_like_plan("plus"));
        assert!(!business_like_plan("pro"));
    }

    #[test]
    fn individual_limit_replaces_only_the_canonical_monthly_lane() {
        let mut provider = provider("Business");
        assert!(apply_individual_limit(&mut provider, &limit()));
        assert_eq!(provider.windows.len(), 3);
        assert_eq!(provider.windows[0].kind, QuotaWindowKind::Session);
        let monthly = &provider.windows[1];
        assert_eq!(monthly.kind, QuotaWindowKind::Billing);
        assert_eq!(monthly.metric, "credits");
        assert_eq!(monthly.currency.as_deref(), Some("CREDITS"));
        assert_eq!(monthly.used, Some(432.762320022503));
        assert_eq!(monthly.limit, Some(750.0));
        assert_eq!(monthly.remaining, Some(317.237679977497));
        assert_eq!(monthly.used_percent, Some(58.0));
        assert_eq!(monthly.remaining_percent, Some(42.0));
        assert_eq!(monthly.resets_at.as_deref(), Some("2026-10-01T00:00:00Z"));
        assert_eq!(monthly.source, CODEX_APP_SERVER_SOURCE);
        assert!(provider.windows[2].additional);
    }

    #[test]
    fn individual_limit_without_remaining_percent_keeps_absolute_business_credits() {
        let mut provider = provider("Business");
        let raw = IndividualLimit {
            limit: NumberLike::Text("1000".to_owned()),
            used: NumberLike::Text("250".to_owned()),
            remaining_percent: None,
            resets_at: Some(json!(1_790_812_800_i64)),
        };
        assert!(apply_individual_limit(&mut provider, &raw));
        let monthly = provider
            .windows
            .iter()
            .find(|window| window.metric == "credits")
            .expect("monthly credit window");
        assert_eq!(monthly.used, Some(250.0));
        assert_eq!(monthly.limit, Some(1000.0));
        assert_eq!(monthly.remaining, Some(750.0));
        assert_eq!(monthly.remaining_percent, Some(75.0));
        assert_eq!(monthly.used_percent, Some(25.0));
    }

    #[test]
    fn existing_structured_credit_window_skips_app_server_enrichment() {
        let mut business = provider("Business");
        assert!(apply_individual_limit(&mut business, &limit()));
        assert!(!provider_needs_enrichment(&business));
        assert!(!provider_needs_enrichment(&provider("Plus")));
    }

    #[test]
    fn app_server_base_windows_fill_empty_tokscale_quota_without_replacing_existing_data() {
        let mut empty = provider("Business");
        empty.windows.clear();
        let snapshot = AppServerSnapshot {
            primary: Some(RateLimitWindow {
                used_percent: Some(20.0),
                resets_at: Some(json!("2026-09-08T01:00:00Z")),
                window_duration_mins: Some(5 * 60),
            }),
            secondary: Some(RateLimitWindow {
                used_percent: Some(40.0),
                resets_at: Some(json!(1_789_171_200_i64)),
                window_duration_mins: Some(7 * 24 * 60),
            }),
            ..AppServerSnapshot::default()
        };
        assert_eq!(
            apply_base_rate_limits(&mut empty, &snapshot, CODEX_APP_SERVER_SOURCE),
            2
        );
        assert_eq!(empty.windows[0].kind, QuotaWindowKind::Session);
        assert_eq!(empty.windows[0].remaining_percent, Some(80.0));
        assert_eq!(empty.windows[0].source, CODEX_APP_SERVER_SOURCE);
        assert_eq!(empty.windows[1].kind, QuotaWindowKind::Weekly);
        assert_eq!(empty.windows[1].remaining_percent, Some(60.0));
        assert!(empty.windows[1].resets_at.is_some());

        let mut existing = provider("Business");
        assert_eq!(
            apply_base_rate_limits(&mut existing, &snapshot, CODEX_APP_SERVER_SOURCE),
            1
        );
        assert_eq!(
            existing
                .windows
                .iter()
                .filter(|window| window.kind == QuotaWindowKind::Session)
                .count(),
            1
        );
    }

    #[test]
    fn oauth_claim_context_accepts_v1_top_level_and_profile_shapes() {
        let top_level = json!({
            "chatgpt_account_id": "Workspace-A",
            "chatgpt_account_is_fedramp": true
        });
        assert_eq!(
            codex_claim_identity(&top_level),
            (Some("workspace-a".to_owned()), Some(true))
        );

        let profile = json!({
            "https://api.openai.com/profile": {
                "chatgpt_account_id": "Workspace-B",
                "chatgpt_account_is_fedramp": false
            }
        });
        assert_eq!(
            codex_claim_identity(&profile),
            (Some("workspace-b".to_owned()), Some(false))
        );
    }

    #[test]
    fn oauth_usage_shape_restores_v1_primary_and_secondary_windows() {
        let payload = json!({
            "plan_type": "Business",
            "rate_limit": {
                "primary_window": {
                    "used_percent": 23,
                    "limit_window_seconds": 18000,
                    "reset_at": 1_789_171_200_i64
                },
                "secondary_window": {
                    "used_percent": 41,
                    "limit_window_seconds": 604800,
                    "reset_at": "2026-09-12T00:00:00Z"
                }
            }
        });
        let snapshot = oauth_snapshot_from_payload(&payload).expect("OAuth usage snapshot");
        assert_eq!(snapshot.rate_plan.as_deref(), Some("Business"));
        let mut target = provider("Business");
        target.windows.clear();
        assert_eq!(
            apply_base_rate_limits(&mut target, &snapshot, CODEX_OAUTH_SOURCE),
            2
        );
        assert_eq!(target.windows[0].kind, QuotaWindowKind::Session);
        assert_eq!(target.windows[0].remaining_percent, Some(77.0));
        assert_eq!(target.windows[0].source, CODEX_OAUTH_SOURCE);
        assert_eq!(target.windows[1].kind, QuotaWindowKind::Weekly);
        assert_eq!(target.windows[1].remaining_percent, Some(59.0));
        assert_eq!(
            codex_usage_endpoint(DEFAULT_CODEX_BASE_URL),
            "https://chatgpt.com/backend-api/wham/usage"
        );
        assert_eq!(
            codex_usage_endpoint("https://chatgpt.com"),
            "https://chatgpt.com/backend-api/wham/usage"
        );
        assert_eq!(
            codex_usage_endpoint("https://chat.openai.com/"),
            "https://chat.openai.com/backend-api/wham/usage"
        );
        assert_eq!(
            codex_usage_endpoint("https://example.test/custom"),
            "https://example.test/custom/api/codex/usage"
        );
    }

    #[test]
    #[ignore = "requires a local Codex OAuth credential and live usage endpoint"]
    fn live_codex_oauth_usage_smoke() {
        let home = env::var_os("HOME").map(PathBuf::from).expect("HOME");
        let snapshot = read_oauth_usage_snapshot(&home).expect("read live Codex OAuth usage");
        assert!(snapshot.primary.is_some() || snapshot.secondary.is_some());
    }

    #[test]
    fn workspace_and_email_guards_reject_cross_account_merges() {
        assert!(enrichment_context_matches(
            Some("workspace-a"),
            Some("workspace-a"),
            Some("workspace-a"),
            Some("USER@example.test"),
            Some("user@example.test"),
        ));
        assert!(!enrichment_context_matches(
            Some("workspace-a"),
            Some("workspace-a"),
            Some("workspace-b"),
            Some("user@example.test"),
            Some("user@example.test"),
        ));
        assert!(!enrichment_context_matches(
            Some("workspace-a"),
            Some("workspace-a"),
            Some("workspace-a"),
            Some("first@example.test"),
            Some("second@example.test"),
        ));
    }

    #[test]
    fn canonical_rate_limit_prefers_codex_bucket() {
        let response: RateLimitResponse = serde_json::from_value(json!({
            "rateLimits": { "planType": "plus" },
            "rateLimitsByLimitId": {
                "codex": {
                    "limitId": "codex",
                    "planType": "business",
                    "individualLimit": {
                        "limit": "25000",
                        "used": "8000",
                        "remainingPercent": 68,
                        "resetsAt": 1798761600
                    }
                }
            }
        }))
        .expect("rate-limit fixture");
        let selected = canonical_rate_snapshot(&response).expect("canonical bucket");
        assert_eq!(selected.plan_type.as_deref(), Some("business"));
        assert!(selected.individual_limit.is_some());

        let snake: RateLimitResponse = serde_json::from_value(json!({
            "rate_limits": {
                "limit_id": "codex",
                "plan_type": "business",
                "individual_limit": {
                    "limit": 750,
                    "used": 300,
                    "remaining_percent": 60,
                    "resets_at": 1790812800
                }
            }
        }))
        .expect("snake-case compatibility fixture");
        assert!(canonical_rate_snapshot(&snake)
            .and_then(|snapshot| snapshot.individual_limit)
            .is_some());
    }

    #[test]
    fn canonical_rate_limit_uses_only_unambiguous_alternate_window_consensus() {
        let consensus: RateLimitResponse = serde_json::from_value(json!({
            "rateLimits": { "planType": "business" },
            "rateLimitsByLimitId": {
                "feature-a": {
                    "planType": "business",
                    "primary": { "usedPercent": 20, "windowDurationMins": 300 },
                    "secondary": { "usedPercent": 30, "windowDurationMins": 10080 }
                },
                "feature-b": {
                    "planType": "business",
                    "primary": { "usedPercent": 20, "windowDurationMins": 300 },
                    "secondary": { "usedPercent": 30, "windowDurationMins": 10080 }
                }
            }
        }))
        .expect("alternate consensus fixture");
        let selected = canonical_rate_snapshot(&consensus).expect("alternate consensus");
        assert_eq!(
            selected.primary.and_then(|window| window.used_percent),
            Some(20.0)
        );
        assert_eq!(
            selected.secondary.and_then(|window| window.used_percent),
            Some(30.0)
        );

        let divergent: RateLimitResponse = serde_json::from_value(json!({
            "rateLimits": { "planType": "business" },
            "rateLimitsByLimitId": {
                "feature-a": { "primary": { "usedPercent": 20, "windowDurationMins": 300 } },
                "feature-b": { "primary": { "usedPercent": 21, "windowDurationMins": 300 } }
            }
        }))
        .expect("divergent alternate fixture");
        let selected = canonical_rate_snapshot(&divergent).expect("direct fallback");
        assert!(selected.primary.is_none());
        assert!(selected.secondary.is_none());
    }

    #[test]
    fn explicit_empty_codex_bucket_is_not_replaced_by_alternate_limits() {
        let response: RateLimitResponse = serde_json::from_value(json!({
            "rateLimitsByLimitId": {
                "codex": { "limitId": "codex", "planType": "business" },
                "feature-a": { "primary": { "usedPercent": 20, "windowDurationMins": 300 } }
            }
        }))
        .expect("explicit canonical fixture");
        let selected = canonical_rate_snapshot(&response).expect("canonical bucket");
        assert!(selected.primary.is_none());
        assert_eq!(selected.plan_type.as_deref(), Some("business"));
    }

    #[tokio::test]
    #[ignore = "requires a local Codex App Server and account"]
    async fn live_codex_app_server_smoke() {
        let snapshot = read_app_server_snapshot()
            .await
            .expect("local Codex App Server should answer read-only RPCs");
        assert!(
            snapshot.account_email.is_some()
                || snapshot.account_plan.is_some()
                || snapshot.rate_plan.is_some()
                || snapshot.individual_limit.is_some()
        );
    }
}
