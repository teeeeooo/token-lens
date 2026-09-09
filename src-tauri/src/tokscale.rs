use crate::domain::{
    CreditStatus, HistoryDay, HistoryReport, HistorySummary, QuotaProvider, QuotaReport,
    QuotaWindow, QuotaWindowKind, ResetCredits, SpendControl, SupportedProvider, TokscaleStatus,
    UsageEntry, UsageGrouping, UsagePeriod, UsageReport, UsageTotals,
};
use crate::portable_sidecar::PortableSidecar;
use serde::Deserialize;
use serde_json::Value;
use std::env;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::process::Command;
use tokio::time::timeout;

const TOKSCALE_TIMEOUT: Duration = Duration::from_secs(30);
const TOKSCALE_CLIENTS: &str = "codex,claude,gemini,antigravity";
const TOKSCALE_SOURCE: &str = "tokscale";

#[derive(Debug, Clone)]
pub struct TokscaleAdapter {
    binary: PathBuf,
    binary_source: String,
    _portable_sidecar: Option<PortableSidecar>,
}

impl TokscaleAdapter {
    pub fn discover() -> Result<Self, String> {
        if let Ok(path) = env::var("TOKEN_LENS_TOKSCALE_BIN") {
            let candidate = PathBuf::from(path);
            if is_executable_candidate(&candidate) {
                return Ok(Self::new(candidate, "env"));
            }
        }

        if let Some(portable) = PortableSidecar::prepare(binary_name())? {
            return Ok(Self::new_portable(portable));
        }

        if let Some(candidate) = bundled_binary_candidate() {
            if is_executable_candidate(&candidate) {
                return Ok(Self::new(candidate, "bundled-sidecar"));
            }
        }

        if let Some(candidate) = project_binary_candidate() {
            if is_executable_candidate(&candidate) {
                return Ok(Self::new(candidate, "project-package"));
            }
        }

        Ok(Self::new(PathBuf::from(binary_name()), "path"))
    }

    fn new(binary: PathBuf, source: &str) -> Self {
        Self {
            binary,
            binary_source: source.to_owned(),
            _portable_sidecar: None,
        }
    }

    fn new_portable(portable: PortableSidecar) -> Self {
        Self {
            binary: portable.path().to_path_buf(),
            binary_source: "embedded-portable".to_owned(),
            _portable_sidecar: Some(portable),
        }
    }

    pub(crate) fn source(&self) -> &str {
        &self.binary_source
    }

    pub async fn status(&self) -> TokscaleStatus {
        match self.run(&["--version"]).await {
            Ok(output) => TokscaleStatus {
                available: true,
                version: parse_version(&output),
                source: self.binary_source.clone(),
            },
            Err(_) => TokscaleStatus {
                available: false,
                version: None,
                source: self.binary_source.clone(),
            },
        }
    }

    pub async fn usage_report(
        &self,
        period: UsagePeriod,
        grouping: UsageGrouping,
    ) -> Result<UsageReport, String> {
        if period == UsagePeriod::Custom {
            return Err("custom usage ranges require an explicit since date".to_owned());
        }
        let mut args = vec![
            "--json",
            "--client",
            TOKSCALE_CLIENTS,
            "--group-by",
            grouping.tokscale_value(),
            "--no-spinner",
        ];
        args.extend_from_slice(period.tokscale_args());
        let output = self.run(&args).await?;
        parse_usage_report(&output, period, None, grouping)
    }

    pub async fn usage_since_report(
        &self,
        since: &str,
        grouping: UsageGrouping,
    ) -> Result<UsageReport, String> {
        validate_date_key(since)?;
        let args = [
            "--json",
            "--client",
            TOKSCALE_CLIENTS,
            "--group-by",
            grouping.tokscale_value(),
            "--no-spinner",
            "--since",
            since,
        ];
        let output = self.run(&args).await?;
        parse_usage_report(
            &output,
            UsagePeriod::Custom,
            Some(since.to_owned()),
            grouping,
        )
    }

    pub async fn history_report(&self, since: &str) -> Result<HistoryReport, String> {
        validate_date_key(since)?;
        let args = [
            "graph",
            "--client",
            TOKSCALE_CLIENTS,
            "--since",
            since,
            "--no-spinner",
        ];
        let output = self.run(&args).await?;
        parse_history_report(&output, since)
    }

    pub async fn quota_report(&self) -> Result<QuotaReport, String> {
        let output = self.run(&["usage", "--json"]).await?;
        parse_quota_report(&output)
    }

    async fn run(&self, args: &[&str]) -> Result<String, String> {
        let mut command = Command::new(&self.binary);
        command
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        crate::background_process::configure_tokio(&mut command);

        let child = command.spawn().map_err(|error| {
            format!(
                "failed to start tokScale from {}: {error}",
                self.binary.display()
            )
        })?;
        let output = timeout(TOKSCALE_TIMEOUT, child.wait_with_output())
            .await
            .map_err(|_| "tokScale command timed out after 30 seconds".to_owned())?
            .map_err(|error| format!("tokScale command failed: {error}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!(
                "tokScale exited with {}: {}",
                output.status,
                stderr.trim()
            ));
        }

        String::from_utf8(output.stdout)
            .map_err(|error| format!("tokScale stdout was not valid UTF-8: {error}"))
    }
}

fn binary_name() -> &'static str {
    if cfg!(windows) {
        "tokscale.exe"
    } else {
        "tokscale"
    }
}

fn is_executable_candidate(path: &Path) -> bool {
    path.is_file()
}

fn bundled_binary_candidate() -> Option<PathBuf> {
    bundled_binary_candidate_from(&env::current_exe().ok()?)
}

fn bundled_binary_candidate_from(executable: &Path) -> Option<PathBuf> {
    Some(executable.parent()?.join(binary_name()))
}

fn project_binary_candidate() -> Option<PathBuf> {
    let relative = platform_package_relative_bin()?;
    let mut directory = env::current_dir().ok()?;
    loop {
        let candidate = directory.join(relative);
        if candidate.is_file() {
            return Some(candidate);
        }
        directory = directory.parent()?.to_path_buf();
    }
}
fn platform_package_relative_bin() -> Option<&'static str> {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    return Some("node_modules/@tokscale/cli-darwin-arm64/bin/tokscale");
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    return Some("node_modules/@tokscale/cli-darwin-x64/bin/tokscale");
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    return Some("node_modules/@tokscale/cli-win32-x64-msvc/bin/tokscale.exe");
    #[cfg(all(target_os = "windows", target_arch = "aarch64"))]
    return Some("node_modules/@tokscale/cli-win32-arm64-msvc/bin/tokscale.exe");
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    return Some("node_modules/@tokscale/cli-linux-x64-gnu/bin/tokscale");
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    return Some("node_modules/@tokscale/cli-linux-arm64-gnu/bin/tokscale");
    #[allow(unreachable_code)]
    None
}

fn parse_version(output: &str) -> Option<String> {
    output
        .split_whitespace()
        .find(|part| {
            part.chars()
                .next()
                .is_some_and(|value| value.is_ascii_digit())
        })
        .map(str::to_owned)
}

fn validate_date_key(value: &str) -> Result<(), String> {
    let bytes = value.as_bytes();
    let shape_ok = bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit());
    if !shape_ok {
        return Err("usage since date must use YYYY-MM-DD".to_owned());
    }
    let year = value[0..4].parse::<u16>().unwrap_or_default();
    let month = value[5..7].parse::<u8>().unwrap_or_default();
    let day = value[8..10].parse::<u8>().unwrap_or_default();
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => 0,
    };
    if year == 0 || day == 0 || day > max_day {
        return Err("usage since date is outside the supported calendar range".to_owned());
    }
    Ok(())
}

fn generated_at_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawUsageReport {
    #[serde(default)]
    entries: Vec<RawUsageEntry>,
    #[serde(default)]
    total_input: u64,
    #[serde(default)]
    total_output: u64,
    #[serde(default)]
    total_cache_read: u64,
    #[serde(default)]
    total_cache_write: u64,
    #[serde(default)]
    total_messages: u64,
    #[serde(default)]
    total_cost: f64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawUsageEntry {
    #[serde(default)]
    client: String,
    #[serde(default)]
    provider: String,
    #[serde(default)]
    model: String,
    session_id: Option<String>,
    #[serde(default)]
    input: u64,
    #[serde(default)]
    output: u64,
    #[serde(default)]
    cache_read: u64,
    #[serde(default)]
    cache_write: u64,
    #[serde(default)]
    reasoning: u64,
    #[serde(default)]
    message_count: u64,
    #[serde(default)]
    cost: f64,
}

fn parse_usage_report(
    output: &str,
    period: UsagePeriod,
    since: Option<String>,
    grouping: UsageGrouping,
) -> Result<UsageReport, String> {
    let raw: RawUsageReport = parse_json(output)?;
    let reasoning = raw.entries.iter().map(|entry| entry.reasoning).sum();
    let entries = raw
        .entries
        .into_iter()
        .map(|entry| UsageEntry {
            client: entry.client,
            provider: entry.provider,
            model: entry.model,
            session_id: entry.session_id,
            input: entry.input,
            output: entry.output,
            cache_read: entry.cache_read,
            cache_write: entry.cache_write,
            reasoning: entry.reasoning,
            message_count: entry.message_count,
            cost: entry.cost,
        })
        .collect();

    Ok(UsageReport {
        period,
        since,
        grouping,
        generated_at_ms: generated_at_ms(),
        entries,
        totals: UsageTotals {
            input: raw.total_input,
            output: raw.total_output,
            cache_read: raw.total_cache_read,
            cache_write: raw.total_cache_write,
            reasoning,
            message_count: raw.total_messages,
            cost: raw.total_cost,
        },
        source: TOKSCALE_SOURCE,
    })
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawGraphReport {
    #[serde(default)]
    meta: RawGraphMeta,
    #[serde(default)]
    summary: RawGraphSummary,
    #[serde(default)]
    contributions: Vec<RawGraphContribution>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawGraphMeta {
    #[serde(default)]
    date_range: RawGraphDateRange,
}

#[derive(Debug, Default, Deserialize)]
struct RawGraphDateRange {
    #[serde(default)]
    start: String,
    #[serde(default)]
    end: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawGraphSummary {
    #[serde(default)]
    total_tokens: u64,
    #[serde(default)]
    total_cost: f64,
    #[serde(default)]
    active_days: u64,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawGraphContribution {
    #[serde(default)]
    date: String,
    #[serde(default)]
    totals: RawGraphTotals,
    #[serde(default)]
    token_breakdown: RawGraphTokenBreakdown,
    #[serde(default)]
    active_time_ms: u64,
}

#[derive(Debug, Default, Deserialize)]
struct RawGraphTotals {
    #[serde(default)]
    tokens: u64,
    #[serde(default)]
    cost: f64,
    #[serde(default)]
    messages: u64,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawGraphTokenBreakdown {
    #[serde(default)]
    input: u64,
    #[serde(default)]
    output: u64,
    #[serde(default)]
    cache_read: u64,
    #[serde(default)]
    cache_write: u64,
    #[serde(default)]
    reasoning: u64,
}

fn parse_history_report(output: &str, requested_since: &str) -> Result<HistoryReport, String> {
    let raw: RawGraphReport = parse_json(output)?;
    let mut daily: Vec<HistoryDay> = raw
        .contributions
        .into_iter()
        .filter(|day| validate_date_key(&day.date).is_ok())
        .map(|day| HistoryDay {
            date: day.date,
            tokens: day.totals.tokens,
            cost: day.totals.cost,
            messages: day.totals.messages,
            active_time_ms: day.active_time_ms,
            input: day.token_breakdown.input,
            output: day.token_breakdown.output,
            cache_read: day.token_breakdown.cache_read,
            cache_write: day.token_breakdown.cache_write,
            reasoning: day.token_breakdown.reasoning,
        })
        .collect();
    daily.sort_by(|a, b| a.date.cmp(&b.date));

    let peak_day_tokens = daily.iter().map(|day| day.tokens).max().unwrap_or(0);
    let active_time_ms = daily.iter().map(|day| day.active_time_ms).sum();
    let active_days = if raw.summary.active_days > 0 {
        raw.summary.active_days
    } else {
        daily.iter().filter(|day| day.tokens > 0).count() as u64
    };
    let total_tokens = if raw.summary.total_tokens > 0 {
        raw.summary.total_tokens
    } else {
        daily.iter().map(|day| day.tokens).sum()
    };
    let total_cost = if raw.summary.total_cost > 0.0 {
        raw.summary.total_cost
    } else {
        daily.iter().map(|day| day.cost).sum()
    };
    let start_date = if raw.meta.date_range.start.is_empty() {
        requested_since.to_owned()
    } else {
        raw.meta.date_range.start
    };
    let end_date = if raw.meta.date_range.end.is_empty() {
        daily
            .last()
            .map(|day| day.date.clone())
            .unwrap_or_else(|| start_date.clone())
    } else {
        raw.meta.date_range.end
    };

    Ok(HistoryReport {
        generated_at_ms: generated_at_ms(),
        start_date,
        end_date,
        daily,
        summary: HistorySummary {
            total_tokens,
            total_cost,
            active_days,
            peak_day_tokens,
            active_time_ms,
        },
        source: TOKSCALE_SOURCE,
    })
}

#[derive(Debug, Deserialize)]
struct RawQuotaProvider {
    #[serde(default)]
    provider: String,
    plan: Option<String>,
    email: Option<String>,
    #[serde(default)]
    metrics: Vec<RawQuotaMetric>,
    reset_credits: Option<RawResetCredits>,
    credit_status: Option<RawCreditStatus>,
    spend_control: Option<RawSpendControl>,
}

#[derive(Debug, Deserialize)]
struct RawQuotaMetric {
    #[serde(default)]
    label: String,
    used_percent: Option<f64>,
    remaining_percent: Option<f64>,
    remaining_label: Option<String>,
    resets_at: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct RawResetCredits {
    available_count: Option<u64>,
    #[serde(default)]
    credits: Vec<RawResetCredit>,
}

#[derive(Debug, Deserialize)]
struct RawResetCredit {
    status: Option<String>,
    expires_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawCreditStatus {
    balance: Option<Value>,
    has_credits: Option<bool>,
    unlimited: Option<bool>,
    overage_limit_reached: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct RawSpendControl {
    individual_limit: Option<Value>,
    reached: Option<bool>,
}

fn parse_quota_report(output: &str) -> Result<QuotaReport, String> {
    let raw: Vec<RawQuotaProvider> = parse_json(output)?;
    let providers = raw
        .into_iter()
        .filter_map(normalize_quota_provider)
        .collect();

    Ok(QuotaReport {
        generated_at_ms: generated_at_ms(),
        providers,
        source: TOKSCALE_SOURCE,
    })
}

fn normalize_quota_provider(raw: RawQuotaProvider) -> Option<QuotaProvider> {
    let provider = supported_provider(&raw.provider)?;
    let windows = raw
        .metrics
        .into_iter()
        .map(|metric| {
            let (kind, additional) = quota_window_shape(&metric.label);
            QuotaWindow {
                kind,
                label: metric.label,
                metric: "quota",
                additional,
                used: None,
                limit: None,
                remaining: None,
                used_percent: metric.used_percent,
                remaining_percent: metric.remaining_percent,
                remaining_label: metric.remaining_label,
                resets_at: metric.resets_at.and_then(json_scalar_string),
                currency: None,
                show_meter: true,
                source: TOKSCALE_SOURCE,
            }
        })
        .collect();

    Some(QuotaProvider {
        provider,
        plan: raw.plan,
        account_email: raw.email,
        diagnostic: None,
        windows,
        reset_credits: raw.reset_credits.map(normalize_reset_credits),
        credit_status: raw.credit_status.map(normalize_credit_status),
        spend_control: raw.spend_control.map(normalize_spend_control),
    })
}

fn supported_provider(value: &str) -> Option<SupportedProvider> {
    match value.trim().to_ascii_lowercase().as_str() {
        "codex" => Some(SupportedProvider::Codex),
        "claude" => Some(SupportedProvider::Claude),
        "gemini" => Some(SupportedProvider::Gemini),
        "antigravity" => Some(SupportedProvider::Antigravity),
        _ => None,
    }
}

fn quota_window_shape(label: &str) -> (QuotaWindowKind, bool) {
    let normalized = label.trim().to_ascii_lowercase();
    let canonical = match normalized.as_str() {
        "5h" | "5 hr" | "5hr" | "session" => Some(QuotaWindowKind::Session),
        "daily" | "day" => Some(QuotaWindowKind::Daily),
        "weekly" | "week" => Some(QuotaWindowKind::Weekly),
        "monthly" | "month" => Some(QuotaWindowKind::Billing),
        _ => None,
    };
    if let Some(kind) = canonical {
        return (kind, false);
    }

    let inferred = if normalized.contains("weekly") || normalized.contains("week") {
        QuotaWindowKind::Weekly
    } else if normalized.contains("5h") || normalized.contains("session") {
        QuotaWindowKind::Session
    } else if normalized.contains("daily") || normalized.contains("day") {
        QuotaWindowKind::Daily
    } else if normalized.contains("monthly") || normalized.contains("month") {
        QuotaWindowKind::Billing
    } else {
        QuotaWindowKind::Other
    };
    (inferred, true)
}

fn normalize_reset_credits(raw: RawResetCredits) -> ResetCredits {
    let available_count = raw.available_count;
    let mut expirations: Vec<String> = raw
        .credits
        .into_iter()
        .filter(|credit| credit.status.as_deref().unwrap_or("available") == "available")
        .filter_map(|credit| credit.expires_at)
        .collect();
    expirations.sort();
    let next_expires_at = expirations.first().cloned();

    ResetCredits {
        available_count: available_count.unwrap_or(expirations.len() as u64),
        next_expires_at,
        expirations,
    }
}

fn normalize_credit_status(raw: RawCreditStatus) -> CreditStatus {
    CreditStatus {
        balance: raw.balance.and_then(json_scalar_string),
        has_credits: raw.has_credits,
        unlimited: raw.unlimited,
        overage_limit_reached: raw.overage_limit_reached,
    }
}

fn normalize_spend_control(raw: RawSpendControl) -> SpendControl {
    SpendControl {
        individual_limit: raw.individual_limit.and_then(json_scalar_string),
        reached: raw.reached,
    }
}

fn json_scalar_string(value: Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::String(value) => Some(value),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        Value::Array(_) | Value::Object(_) => None,
    }
}

fn parse_json<T>(output: &str) -> Result<T, String>
where
    T: for<'de> Deserialize<'de>,
{
    let trimmed = output.trim();
    if trimmed.is_empty() {
        return Err("tokScale produced empty stdout".to_owned());
    }
    if let Ok(value) = serde_json::from_str(trimmed) {
        return Ok(value);
    }

    for marker in ['{', '['] {
        if let Some(index) = trimmed.find(marker) {
            if let Ok(value) = serde_json::from_str(&trimmed[index..]) {
                return Ok(value);
            }
        }
    }

    Err(format!(
        "could not parse tokScale JSON output: {}",
        trimmed.chars().take(240).collect::<String>()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_candidate_is_resolved_next_to_the_app_executable() {
        let executable = if cfg!(windows) {
            PathBuf::from(r"C:\Program Files\Token Lens\token-lens.exe")
        } else {
            PathBuf::from("/Applications/Token Lens.app/Contents/MacOS/token-lens")
        };
        let candidate = bundled_binary_candidate_from(&executable).expect("candidate");
        assert_eq!(
            candidate.file_name().and_then(|value| value.to_str()),
            Some(binary_name())
        );
        assert_eq!(candidate.parent(), executable.parent());
    }

    const USAGE_FIXTURE: &str = r#"{
      "groupBy":"client,session,model",
      "entries":[{
        "client":"codex","sessionId":"rollout-1","model":"gpt-5.6-sol",
        "provider":"openai","input":100,"output":20,"cacheRead":300,
        "cacheWrite":4,"reasoning":5,"messageCount":2,"cost":0.42
      }],
      "totalInput":100,"totalOutput":20,"totalCacheRead":300,
      "totalCacheWrite":4,"totalMessages":2,"totalCost":0.42
    }"#;

    const HISTORY_FIXTURE: &str = r#"{
      "meta":{"dateRange":{"start":"2026-09-01","end":"2026-09-03"}},
      "summary":{"totalTokens":600,"totalCost":1.5,"activeDays":2},
      "contributions":[
        {"date":"2026-09-01","totals":{"tokens":100,"cost":0.25,"messages":2},
         "tokenBreakdown":{"input":10,"output":20,"cacheRead":60,"cacheWrite":5,"reasoning":5},
         "activeTimeMs":1000},
        {"date":"2026-09-03","totals":{"tokens":500,"cost":1.25,"messages":4},
         "tokenBreakdown":{"input":50,"output":40,"cacheRead":380,"cacheWrite":10,"reasoning":20},
         "activeTimeMs":3000}
      ]
    }"#;

    const QUOTA_FIXTURE: &str = r#"[
      {"provider":"Codex","plan":"Plus","email":"user@example.test","metrics":[
        {"label":"5h","used_percent":65.0,"remaining_percent":35.0,
         "remaining_label":null,"resets_at":"2026-09-04T16:59:41+00:00"},
        {"label":"Weekly","used_percent":100.0,"remaining_percent":0.0,
         "remaining_label":null,"resets_at":"2026-09-07T02:31:33+00:00"},
        {"label":"Gpt-reserve weekly","used_percent":10.0,"remaining_percent":90.0,
         "remaining_label":null,"resets_at":"2026-09-07T02:31:33+00:00"}
      ],
      "reset_credits":{"available_count":1,"credits":[
        {"status":"available","expires_at":"2026-10-04T01:03:08Z"}
      ]},
      "credit_status":{"balance":"0","has_credits":false,"unlimited":false,
        "overage_limit_reached":false},
      "spend_control":{"reached":false}
      },
      {"provider":"Copilot","plan":"Individual","email":"other@example.test",
       "metrics":[{"label":"Premium","used_percent":1.0,"remaining_percent":99.0}]}
    ]"#;

    #[test]
    fn parses_usage_into_stable_domain() {
        let report = parse_usage_report(
            USAGE_FIXTURE,
            UsagePeriod::Today,
            None,
            UsageGrouping::ClientSessionModel,
        )
        .expect("usage fixture should parse");
        assert_eq!(report.entries.len(), 1);
        assert_eq!(report.entries[0].session_id.as_deref(), Some("rollout-1"));
        assert_eq!(report.entries[0].reasoning, 5);
        assert_eq!(report.totals.reasoning, 5);
        assert_eq!(report.totals.cache_read, 300);
    }

    #[test]
    fn parses_graph_into_normalized_history() {
        let report = parse_history_report(HISTORY_FIXTURE, "2026-09-01")
            .expect("history fixture should parse");
        assert_eq!(report.start_date, "2026-09-01");
        assert_eq!(report.end_date, "2026-09-03");
        assert_eq!(report.daily.len(), 2);
        assert_eq!(report.daily[0].tokens, 100);
        assert_eq!(report.daily[1].reasoning, 20);
        assert_eq!(report.summary.total_tokens, 600);
        assert_eq!(report.summary.active_days, 2);
        assert_eq!(report.summary.peak_day_tokens, 500);
        assert_eq!(report.summary.active_time_ms, 4000);
        assert_eq!(report.source, TOKSCALE_SOURCE);
    }

    #[test]
    fn filters_unsupported_quota_providers_and_classifies_windows() {
        let report = parse_quota_report(QUOTA_FIXTURE).expect("quota fixture should parse");
        assert_eq!(report.providers.len(), 1);
        let codex = &report.providers[0];
        assert_eq!(codex.provider, SupportedProvider::Codex);
        assert_eq!(codex.windows.len(), 3);
        assert_eq!(codex.windows[0].kind, QuotaWindowKind::Session);
        assert_eq!(codex.windows[1].kind, QuotaWindowKind::Weekly);
        assert_eq!(codex.windows[2].kind, QuotaWindowKind::Weekly);
        assert!(codex.windows[2].additional);
        assert!(!codex.windows[1].additional);
        assert_eq!(
            codex
                .reset_credits
                .as_ref()
                .map(|value| value.available_count),
            Some(1)
        );
    }

    #[test]
    fn reset_credit_count_falls_back_to_available_credit_entries() {
        let raw = r#"[{"provider":"Codex","metrics":[],"reset_credits":{"credits":[
          {"status":"available","expires_at":"2026-10-04T01:03:08Z"},
          {"status":"used","expires_at":"2026-10-05T01:03:08Z"}
        ]}}]"#;
        let report = parse_quota_report(raw).expect("quota fixture should parse");
        let credits = report.providers[0].reset_credits.as_ref().unwrap();
        assert_eq!(credits.available_count, 1);
        assert_eq!(credits.expirations.len(), 1);
    }

    #[test]
    fn structured_spend_control_limit_is_not_misrepresented_as_scalar() {
        let raw = r#"[{"provider":"Codex","metrics":[],"spend_control":{
          "individual_limit":{"limit":"750","used":"432"},"reached":false
        }}]"#;
        let report = parse_quota_report(raw).expect("quota fixture should parse");
        let spend = report.providers[0].spend_control.as_ref().unwrap();
        assert_eq!(spend.individual_limit, None);
        assert_eq!(spend.reached, Some(false));
    }

    #[test]
    fn usage_since_date_validation_rejects_unsafe_or_invalid_shapes() {
        assert!(validate_date_key("2026-08-30").is_ok());
        assert!(validate_date_key("2024-02-29").is_ok());
        assert!(validate_date_key("2026-02-29").is_err());
        assert!(validate_date_key("2026-04-31").is_err());
        assert!(validate_date_key("2026-13-30").is_err());
        assert!(validate_date_key("2026-08-00").is_err());
        assert!(validate_date_key("--debug").is_err());
    }

    #[tokio::test]
    #[ignore = "requires local tokScale binary and provider credentials"]
    async fn live_tokscale_smoke() {
        let adapter = TokscaleAdapter::discover().expect("tokScale should resolve");
        let status = adapter.status().await;
        assert!(status.available, "tokScale status should be available");

        let usage = adapter
            .usage_report(UsagePeriod::Today, UsageGrouping::ClientSessionModel)
            .await
            .expect("live usage should normalize");
        assert_eq!(usage.source, TOKSCALE_SOURCE);

        let all_time = adapter
            .usage_report(UsagePeriod::AllTime, UsageGrouping::ClientSessionModel)
            .await
            .expect("live all-time usage should normalize");
        assert_eq!(all_time.source, TOKSCALE_SOURCE);

        let custom = adapter
            .usage_since_report("2026-01-01", UsageGrouping::ClientSessionModel)
            .await
            .expect("live custom-range usage should normalize");
        assert_eq!(custom.period, UsagePeriod::Custom);
        assert_eq!(custom.since.as_deref(), Some("2026-01-01"));
        assert_eq!(custom.source, TOKSCALE_SOURCE);

        let history = adapter
            .history_report("2026-01-01")
            .await
            .expect("live history should normalize");
        assert_eq!(history.source, TOKSCALE_SOURCE);
        assert!(history
            .daily
            .iter()
            .all(|day| validate_date_key(&day.date).is_ok()));

        let quota = adapter
            .quota_report()
            .await
            .expect("live quota should normalize");
        assert!(quota.providers.iter().all(|provider| matches!(
            provider.provider,
            SupportedProvider::Codex
                | SupportedProvider::Claude
                | SupportedProvider::Gemini
                | SupportedProvider::Antigravity
        )));
    }
}
