use crate::domain::{QuotaProvider, QuotaReport, QuotaWindow, QuotaWindowKind, SupportedProvider};
use serde_json::Value;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const SOURCE: &str = "claude-oauth-usage";
const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const HTTP_TIMEOUT_SECONDS: u64 = 12;

pub(crate) async fn enrich_quota_report(home: &Path, report: QuotaReport) -> QuotaReport {
    let fallback = report.clone();
    let home = home.to_path_buf();
    tokio::task::spawn_blocking(move || enrich_quota_report_sync(&home, report))
        .await
        .unwrap_or(fallback)
}

fn enrich_quota_report_sync(home: &Path, mut report: QuotaReport) -> QuotaReport {
    let needs = report
        .providers
        .iter()
        .find(|provider| provider.provider == SupportedProvider::Claude)
        .map_or(true, provider_needs_enrichment);
    if !needs {
        return report;
    }
    let Some(access_token) = read_access_token(home) else {
        return report;
    };
    let Ok(usage) = fetch_usage(&access_token) else {
        return report;
    };
    let windows = windows_from_usage(&usage);
    if windows.is_empty() {
        return report;
    }

    if let Some(provider) = report
        .providers
        .iter_mut()
        .find(|provider| provider.provider == SupportedProvider::Claude)
    {
        merge_windows(provider, windows);
    } else {
        report.providers.push(QuotaProvider {
            provider: SupportedProvider::Claude,
            plan: None,
            account_email: None,
            windows,
            reset_credits: None,
            credit_status: None,
            spend_control: None,
        });
    }
    report
}

fn provider_needs_enrichment(provider: &QuotaProvider) -> bool {
    let has_spend = provider
        .windows
        .iter()
        .any(|window| window.metric == "spend");
    let has_base = provider.windows.iter().any(|window| {
        !window.additional
            && matches!(
                window.kind,
                QuotaWindowKind::Session | QuotaWindowKind::Weekly
            )
            && window.remaining_percent.is_some()
    });
    !has_spend || !has_base
}

fn merge_windows(provider: &mut QuotaProvider, incoming: Vec<QuotaWindow>) {
    for window in incoming {
        let duplicate = if window.metric == "spend" {
            provider
                .windows
                .iter()
                .any(|existing| existing.metric == "spend")
        } else {
            provider.windows.iter().any(|existing| {
                !existing.additional
                    && existing.kind == window.kind
                    && existing.remaining_percent.is_some()
            })
        };
        if duplicate {
            continue;
        }
        provider.windows.push(window);
    }
    provider.windows.sort_by_key(window_rank);
}

fn window_rank(window: &QuotaWindow) -> u8 {
    match window.kind {
        QuotaWindowKind::Session => 0,
        QuotaWindowKind::Daily => 1,
        QuotaWindowKind::Weekly => 2,
        QuotaWindowKind::Billing => 3,
        QuotaWindowKind::Other => 4,
    }
}

fn fetch_usage(access_token: &str) -> Result<Value, String> {
    let token = access_token.trim();
    if token.is_empty() {
        return Err("Claude access token is unavailable".to_owned());
    }
    let response = minreq::get(USAGE_URL)
        .with_header("accept", "application/json")
        .with_header("authorization", format!("Bearer {token}"))
        .with_header("anthropic-beta", "oauth-2025-04-20")
        .with_header("user-agent", "Token-Lens/2")
        .with_timeout(HTTP_TIMEOUT_SECONDS)
        .with_follow_redirects(false)
        .send()
        .map_err(|_| "Claude usage request failed".to_owned())?;
    if !(200..300).contains(&response.status_code) {
        return Err(format!(
            "Claude usage returned HTTP {}",
            response.status_code
        ));
    }
    response
        .json::<Value>()
        .map_err(|_| "Claude usage returned an invalid payload".to_owned())
}

fn windows_from_usage(usage: &Value) -> Vec<QuotaWindow> {
    let mut windows = Vec::new();
    if let Some(window) = usage_window(
        usage,
        &["five_hour", "fiveHour"],
        QuotaWindowKind::Session,
        "5h",
    ) {
        windows.push(window);
    }
    if let Some(window) = usage_window(
        usage,
        &["seven_day", "sevenDay"],
        QuotaWindowKind::Weekly,
        "Weekly",
    ) {
        windows.push(window);
    }
    if let Some(window) = usage_credits_window(usage) {
        windows.push(window);
    }
    windows
}

fn usage_window(
    usage: &Value,
    aliases: &[&str],
    kind: QuotaWindowKind,
    label: &str,
) -> Option<QuotaWindow> {
    let raw = value_from_aliases(usage, aliases)?;
    let used_percent = number_from_aliases(
        raw,
        &["usedPercent", "used_percent", "utilization", "percent"],
    )?
    .clamp(0.0, 100.0);
    Some(QuotaWindow {
        kind,
        label: label.to_owned(),
        metric: "quota",
        additional: false,
        used: None,
        limit: None,
        remaining: None,
        used_percent: Some(used_percent),
        remaining_percent: Some(100.0 - used_percent),
        remaining_label: None,
        resets_at: scalar_string(value_from_aliases(raw, &["resets_at", "resetsAt"])),
        currency: None,
        show_meter: true,
        source: SOURCE,
    })
}

fn usage_credits_window(usage: &Value) -> Option<QuotaWindow> {
    let spend = value_from_aliases(usage, &["spend"]);
    let extra = value_from_aliases(usage, &["extra_usage", "extraUsage"]);
    let enabled = spend
        .and_then(|value| value.get("enabled"))
        .and_then(Value::as_bool)
        == Some(true)
        || extra
            .and_then(|value| value_from_aliases(value, &["is_enabled", "isEnabled"]))
            .and_then(Value::as_bool)
            == Some(true);
    if !enabled {
        return None;
    }

    let spend_used = spend
        .and_then(|value| value.get("used"))
        .and_then(spend_money);
    let spend_limit = spend
        .and_then(|value| value.get("limit"))
        .and_then(spend_money);
    let used = spend_used
        .as_ref()
        .map(|money| money.0)
        .or_else(|| extra.and_then(|value| extra_usage_money(value, "used_credits")))?;
    let limit = spend_limit
        .as_ref()
        .map(|money| money.0)
        .or_else(|| extra.and_then(|value| extra_usage_money(value, "monthly_limit")));
    let currency = spend_used
        .as_ref()
        .and_then(|money| money.1.clone())
        .or_else(|| extra.and_then(|value| scalar_string(value.get("currency"))))
        .unwrap_or_else(|| "USD".to_owned())
        .to_ascii_uppercase();
    let remaining = limit.map(|limit| (limit - used).max(0.0));
    let used_percent = limit
        .filter(|limit| *limit > 0.0)
        .map(|limit| (used / limit * 100.0).clamp(0.0, 100.0));
    let remaining_percent = used_percent.map(|value| 100.0 - value);

    Some(QuotaWindow {
        kind: QuotaWindowKind::Billing,
        label: "Usage credits".to_owned(),
        metric: "spend",
        additional: false,
        used: Some(used),
        limit,
        remaining,
        used_percent,
        remaining_percent,
        remaining_label: None,
        resets_at: None,
        currency: Some(currency),
        show_meter: limit.is_some(),
        source: SOURCE,
    })
}

fn spend_money(value: &Value) -> Option<(f64, Option<String>)> {
    let minor = number_from_aliases(value, &["amount_minor", "amountMinor"])?;
    let exponent = number_from_aliases(value, &["exponent"]).unwrap_or(2.0);
    if exponent < 0.0 || exponent > 12.0 {
        return None;
    }
    let amount = minor / 10f64.powf(exponent);
    let currency = scalar_string(value_from_aliases(value, &["currency"]))
        .map(|value| value.to_ascii_uppercase());
    Some((amount, currency))
}

fn extra_usage_money(extra: &Value, key: &str) -> Option<f64> {
    let raw = number_from_aliases(extra, &[key])?;
    let places = number_from_aliases(extra, &["decimal_places", "decimalPlaces"])
        .unwrap_or(2.0)
        .clamp(0.0, 12.0);
    Some(raw / 10f64.powf(places))
}

fn number_from_aliases(value: &Value, aliases: &[&str]) -> Option<f64> {
    let value = value_from_aliases(value, aliases)?;
    match value {
        Value::Number(number) => number.as_f64().filter(|value| value.is_finite()),
        Value::String(value) => value.parse::<f64>().ok().filter(|value| value.is_finite()),
        _ => None,
    }
}

fn value_from_aliases<'a>(value: &'a Value, aliases: &[&str]) -> Option<&'a Value> {
    aliases
        .iter()
        .find_map(|key| value.get(*key))
        .filter(|value| !value.is_null())
}

fn scalar_string(value: Option<&Value>) -> Option<String> {
    let value = match value? {
        Value::String(value) => value.trim().to_owned(),
        Value::Number(value) => value.to_string(),
        Value::Bool(value) => value.to_string(),
        _ => return None,
    };
    (!value.is_empty()).then_some(value)
}

fn read_access_token(home: &Path) -> Option<String> {
    if let Ok(value) = env::var("CLAUDE_CODE_OAUTH_TOKEN") {
        let value = value.trim().to_owned();
        if !value.is_empty() {
            return Some(value);
        }
    }
    for path in claude_credential_paths(home) {
        let Ok(bytes) = fs::read(path) else {
            continue;
        };
        if let Some(token) = read_token_json(&bytes) {
            return Some(token);
        }
    }
    #[cfg(target_os = "windows")]
    if let Some(token) = read_windows_credential_token() {
        return Some(token);
    }
    None
}

fn claude_credential_paths(home: &Path) -> Vec<PathBuf> {
    if let Some(root) = env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
    {
        return vec![root.join(".credentials.json")];
    }
    vec![home.join(".claude/.credentials.json")]
}

fn read_token_json(bytes: &[u8]) -> Option<String> {
    let value = serde_json::from_slice::<Value>(bytes).ok()?;
    let oauth = value
        .get("claudeAiOauth")
        .or_else(|| value.get("oauth"))
        .unwrap_or(&value);
    scalar_string(value_from_aliases(oauth, &["accessToken", "access_token"]))
}

#[cfg(target_os = "windows")]
fn read_windows_credential_token() -> Option<String> {
    use std::ffi::OsStr;
    use std::iter::once;
    use std::os::windows::ffi::OsStrExt;
    use std::ptr;
    use windows_sys::Win32::Security::Credentials::{
        CredFree, CredReadW, CREDENTIALW, CRED_TYPE_GENERIC,
    };

    let mut targets = vec!["Claude Code-credentials".to_owned()];
    for key in ["USER", "USERNAME"] {
        if let Ok(user) = env::var(key) {
            let user = user.trim();
            if !user.is_empty() {
                targets.push(format!("Claude Code-credentials:{user}"));
                targets.push(format!("Claude Code-credentials/{user}"));
            }
        }
    }
    for target in targets {
        let wide = OsStr::new(&target)
            .encode_wide()
            .chain(once(0))
            .collect::<Vec<_>>();
        let mut credential: *mut CREDENTIALW = ptr::null_mut();
        let ok = unsafe { CredReadW(wide.as_ptr(), CRED_TYPE_GENERIC, 0, &mut credential) } != 0;
        if !ok || credential.is_null() {
            continue;
        }
        let bytes = unsafe {
            let item = &*credential;
            std::slice::from_raw_parts(item.CredentialBlob, item.CredentialBlobSize as usize)
                .to_vec()
        };
        unsafe { CredFree(credential.cast()) };
        if let Some(token) =
            decode_credential_blob(&bytes).and_then(|text| read_token_json(text.as_bytes()))
        {
            return Some(token);
        }
    }
    None
}

#[cfg(target_os = "windows")]
fn decode_credential_blob(bytes: &[u8]) -> Option<String> {
    let utf8 = String::from_utf8_lossy(bytes)
        .trim_matches('\0')
        .trim()
        .to_owned();
    if utf8.starts_with('{') || utf8.contains("accessToken") {
        return Some(utf8);
    }
    if bytes.len() % 2 == 0 {
        let units = bytes
            .chunks_exact(2)
            .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
            .collect::<Vec<_>>();
        let utf16 = String::from_utf16_lossy(&units)
            .trim_matches('\0')
            .trim()
            .to_owned();
        if utf16.starts_with('{') || utf16.contains("accessToken") {
            return Some(utf16);
        }
    }
    (!utf8.is_empty()).then_some(utf8)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_usage_credits_from_self_describing_spend_shape() {
        let usage = json!({
            "spend": {
                "enabled": true,
                "used": { "amount_minor": 23500, "currency": "usd", "exponent": 2 },
                "limit": { "amount_minor": 200000, "currency": "usd", "exponent": 2 }
            }
        });
        let [window] = windows_from_usage(&usage)
            .try_into()
            .expect("one billing window");
        assert_eq!(window.metric, "spend");
        assert_eq!(window.label, "Usage credits");
        assert_eq!(window.used, Some(235.0));
        assert_eq!(window.limit, Some(2000.0));
        assert_eq!(window.remaining, Some(1765.0));
        assert_eq!(window.currency.as_deref(), Some("USD"));
        assert_eq!(window.remaining_percent, Some(88.25));
    }

    #[test]
    fn parses_extra_usage_alias_without_inventing_a_disabled_credit_window() {
        let enabled = json!({
            "extra_usage": {
                "is_enabled": true,
                "used_credits": 23500,
                "monthly_limit": 200000,
                "decimal_places": 2,
                "currency": "USD"
            }
        });
        let [window] = windows_from_usage(&enabled)
            .try_into()
            .expect("one billing window");
        assert_eq!(window.used, Some(235.0));
        assert_eq!(window.limit, Some(2000.0));

        let disabled =
            json!({"extra_usage":{"is_enabled":false,"used_credits":0,"monthly_limit":200000}});
        assert!(windows_from_usage(&disabled).is_empty());
    }

    #[test]
    fn fills_missing_base_quota_but_does_not_replace_tokscale_windows() {
        let usage = json!({
            "five_hour": { "utilization": 25, "resets_at": "2026-09-08T01:00:00Z" },
            "seven_day": { "utilization": 40, "resets_at": "2026-09-12T01:00:00Z" }
        });
        let windows = windows_from_usage(&usage);
        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0].remaining_percent, Some(75.0));
        assert_eq!(windows[1].remaining_percent, Some(60.0));
    }

    #[test]
    fn credential_parser_reads_only_access_token_from_current_file_shapes() {
        let nested = br#"{"claudeAiOauth":{"accessToken":"token-a","refreshToken":"DO_NOT_USE"}}"#;
        let root = br#"{"accessToken":"token-b","refreshToken":"DO_NOT_USE"}"#;
        assert_eq!(read_token_json(nested).as_deref(), Some("token-a"));
        assert_eq!(read_token_json(root).as_deref(), Some("token-b"));
    }
}
