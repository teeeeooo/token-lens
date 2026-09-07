use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsagePeriod {
    Today,
    Week,
    Month,
    AllTime,
    Custom,
}

impl UsagePeriod {
    pub(crate) fn tokscale_args(self) -> &'static [&'static str] {
        match self {
            Self::Today => &["--today"],
            Self::Week => &["--week"],
            Self::Month => &["--month"],
            Self::AllTime | Self::Custom => &[],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageGrouping {
    Model,
    ClientModel,
    ClientSessionModel,
}

impl UsageGrouping {
    pub(crate) fn tokscale_value(self) -> &'static str {
        match self {
            Self::Model => "model",
            Self::ClientModel => "client,model",
            Self::ClientSessionModel => "client,session,model",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageReport {
    pub period: UsagePeriod,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since: Option<String>,
    pub grouping: UsageGrouping,
    pub generated_at_ms: u64,
    pub entries: Vec<UsageEntry>,
    pub totals: UsageTotals,
    pub source: &'static str,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageEntry {
    pub client: String,
    pub provider: String,
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub reasoning: u64,
    pub message_count: u64,
    pub cost: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageTotals {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub reasoning: u64,
    pub message_count: u64,
    pub cost: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryReport {
    pub generated_at_ms: u64,
    pub start_date: String,
    pub end_date: String,
    pub daily: Vec<HistoryDay>,
    pub summary: HistorySummary,
    pub source: &'static str,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryDay {
    pub date: String,
    pub tokens: u64,
    pub cost: f64,
    pub messages: u64,
    pub active_time_ms: u64,
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub reasoning: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistorySummary {
    pub total_tokens: u64,
    pub total_cost: f64,
    pub active_days: u64,
    pub peak_day_tokens: u64,
    pub active_time_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMetadataRef {
    pub client: String,
    pub session_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMetadata {
    pub client: String,
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMetadataReport {
    pub generated_at_ms: u64,
    pub sessions: Vec<SessionMetadata>,
    pub source: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionTokenBreakdown {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub reasoning: u64,
    pub total: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionTurnDetail {
    pub timestamp: String,
    pub tokens: SessionTokenBreakdown,
    pub tools: Vec<String>,
    pub cost_estimate: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionExchangeDetail {
    pub started_at: String,
    pub ended_at: String,
    pub turn_count: u64,
    pub tools: Vec<String>,
    pub tokens: SessionTokenBreakdown,
    pub cost_estimate: f64,
    pub turns: Vec<SessionTurnDetail>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionDetailTotals {
    pub total_tokens: u64,
    pub cost_usd: f64,
    pub exchange_count: u64,
    pub turn_count: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionDetailReport {
    pub found: bool,
    pub client: String,
    pub session_id: String,
    pub exchanges: Vec<SessionExchangeDetail>,
    pub totals: SessionDetailTotals,
    pub source: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SupportedProvider {
    Codex,
    Claude,
    Gemini,
    Antigravity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum QuotaWindowKind {
    Session,
    Daily,
    Weekly,
    Billing,
    Other,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaReport {
    pub generated_at_ms: u64,
    pub providers: Vec<QuotaProvider>,
    pub source: &'static str,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaProvider {
    pub provider: SupportedProvider,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<String>,
    pub windows: Vec<QuotaWindow>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reset_credits: Option<ResetCredits>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credit_status: Option<CreditStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spend_control: Option<SpendControl>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaWindow {
    pub kind: QuotaWindowKind,
    pub label: String,
    pub metric: &'static str,
    pub additional: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub used: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remaining: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub used_percent: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remaining_percent: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remaining_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resets_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
    pub show_meter: bool,
    pub source: &'static str,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResetCredits {
    pub available_count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_expires_at: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub expirations: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreditStatus {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub balance: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_credits: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unlimited: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overage_limit_reached: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpendControl {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub individual_limit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reached: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokscaleStatus {
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub source: String,
}
