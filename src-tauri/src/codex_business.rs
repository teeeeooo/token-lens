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
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::time::{sleep, timeout};

const CODEX_APP_SERVER_SOURCE: &str = "codex-app-server";
const RPC_TIMEOUT: Duration = Duration::from_secs(20);
const EMPTY_LIMIT_RETRY_DELAY: Duration = Duration::from_millis(300);
const MAX_RPC_LINE_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RateLimitSnapshot {
    #[serde(alias = "limit_id")]
    limit_id: Option<String>,
    #[serde(alias = "plan_type")]
    plan_type: Option<String>,
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

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IndividualLimit {
    limit: NumberLike,
    used: NumberLike,
    #[serde(alias = "remaining_percent")]
    remaining_percent: f64,
    #[serde(alias = "resets_at")]
    resets_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
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
    individual_limit: Option<IndividualLimit>,
}

#[derive(Debug, Default, Deserialize)]
struct StoredCodexAuth {
    tokens: Option<StoredCodexTokens>,
    #[serde(alias = "accountId")]
    account_id: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct StoredCodexTokens {
    #[serde(alias = "accountId")]
    account_id: Option<String>,
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

    let expected_email = report.providers[index].account_email.clone();
    let snapshot = match read_app_server_snapshot().await {
        Ok(snapshot) => snapshot,
        Err(_) => return report,
    };

    let after_workspace = selected_workspace_id(home);
    if !enrichment_context_matches(
        expected_workspace_id.as_deref(),
        before_workspace.as_deref(),
        after_workspace.as_deref(),
        expected_email.as_deref(),
        snapshot.account_email.as_deref(),
    ) {
        return report;
    }

    if snapshot
        .rate_plan
        .as_deref()
        .or(snapshot.account_plan.as_deref())
        .is_some_and(|plan| !business_like_plan(plan))
    {
        return report;
    }

    if let Some(individual_limit) = snapshot.individual_limit.as_ref() {
        apply_individual_limit(&mut report.providers[index], individual_limit);
    }
    report
}

fn provider_needs_enrichment(provider: &QuotaProvider) -> bool {
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

    let remaining_percent = raw.remaining_percent.clamp(0.0, 100.0);
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
        resets_at: epoch_seconds_to_rfc3339(raw.resets_at),
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

        let account = rpc_call::<AccountReadResponse>(
            &mut stdin,
            &mut reader,
            2,
            "account/read",
            Some(json!({ "refreshToken": false })),
        )
        .await
        .ok()
        .and_then(|response| response.account);

        let mut rates = rpc_call::<RateLimitResponse>(
            &mut stdin,
            &mut reader,
            3,
            "account/rateLimits/read",
            None,
        )
        .await?;

        let mut selected = canonical_rate_snapshot(&rates).cloned().unwrap_or_default();
        let plan_hint = selected.plan_type.as_deref().or(account
            .as_ref()
            .and_then(|value| value.plan_type.as_deref()));
        if selected.individual_limit.is_none() && plan_hint.is_some_and(business_like_plan) {
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
                selected = canonical_rate_snapshot(&rates).cloned().unwrap_or_default();
            }
        }

        Ok(AppServerSnapshot {
            account_email: account.as_ref().and_then(|value| value.email.clone()),
            account_plan: account.and_then(|value| value.plan_type),
            rate_plan: selected.plan_type,
            individual_limit: selected.individual_limit,
        })
    }
    .await;

    drop(stdin);
    drop(reader);
    terminate_child(&mut child).await;
    result
}

fn canonical_rate_snapshot(response: &RateLimitResponse) -> Option<&RateLimitSnapshot> {
    response
        .rate_limits_by_limit_id
        .as_ref()
        .and_then(|by_id| {
            by_id.get("codex").or_else(|| {
                by_id.values().find(|snapshot| {
                    snapshot
                        .limit_id
                        .as_deref()
                        .is_some_and(|value| value.eq_ignore_ascii_case("codex"))
                })
            })
        })
        .or(Some(&response.rate_limits))
}

fn spawn_app_server(command: &Path) -> Result<Child, String> {
    let mut process = Command::new(command);
    process
        .args(["-s", "read-only", "-a", "untrusted", "app-server"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    process.spawn().map_err(|error| {
        format!(
            "failed to start Codex App Server from {}: {error}",
            command.display()
        )
    })
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
    add_windows_candidates(&mut candidates);

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
        let bin = local.join("OpenAI/Codex/bin");
        push_candidate(candidates, bin.join("codex.exe"));
        if let Ok(entries) = fs::read_dir(&bin) {
            for entry in entries.flatten() {
                if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                    push_candidate(candidates, entry.path().join("codex.exe"));
                }
            }
        }
        push_candidate(candidates, local.join("Microsoft/WindowsApps/codex.exe"));
    }
    if let Some(program_files) = env::var_os("ProgramFiles").map(PathBuf::from) {
        push_candidate(candidates, program_files.join("Codex/resources/codex.exe"));
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
            remaining_percent: 42.0,
            resets_at: 1_790_812_800,
        }
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
    fn existing_structured_credit_window_skips_app_server_enrichment() {
        let mut business = provider("Business");
        assert!(apply_individual_limit(&mut business, &limit()));
        assert!(!provider_needs_enrichment(&business));
        assert!(!provider_needs_enrichment(&provider("Plus")));
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
            .and_then(|snapshot| snapshot.individual_limit.as_ref())
            .is_some());
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
