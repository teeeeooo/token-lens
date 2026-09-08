use chrono::{DateTime, NaiveDateTime, Utc};
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::SystemTime;

const API_BASE: &str = "https://cloudcode-pa.googleapis.com/v1internal";
const HTTP_TIMEOUT_SECONDS: u64 = 12;
const USER_AGENT: &str = "Token-Lens/2";
const DEFAULT_RATE_LIMIT_COOLDOWN_MS: u64 = 60_000;
const MAX_RATE_LIMIT_COOLDOWN_MS: u64 = 60 * 60 * 1000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CodeAssistError {
    Unauthorized,
    Forbidden,
    RateLimited { retry_after_ms: u64 },
    Transport,
    Http(i32),
    InvalidPayload,
    InvalidRequest,
}

impl CodeAssistError {
    pub(crate) fn diagnostic(&self, method: &str) -> String {
        match self {
            Self::Unauthorized => format!("Google Code Assist {method} returned HTTP 401"),
            Self::Forbidden => format!("Google Code Assist {method} returned HTTP 403"),
            Self::RateLimited { retry_after_ms } => format!(
                "Google Code Assist {method} returned HTTP 429; retry in about {}s",
                retry_after_ms.div_ceil(1000)
            ),
            Self::Transport => format!("Google Code Assist {method} request failed"),
            Self::Http(status) => format!("Google Code Assist {method} returned HTTP {status}"),
            Self::InvalidPayload => {
                format!("Google Code Assist {method} returned an invalid payload")
            }
            Self::InvalidRequest => {
                format!("Google Code Assist {method} request could not be encoded")
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LoadSnapshot {
    pub project_id: Option<String>,
    pub plan: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct QuotaBucket {
    pub model_id: Option<String>,
    pub remaining_fraction: Option<f64>,
    pub remaining_amount: Option<String>,
    pub reset_time: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QuotaResponse {
    #[serde(default)]
    buckets: Vec<QuotaBucket>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LoadResponse {
    cloudaicompanion_project: Option<Value>,
    current_tier: Option<Tier>,
    paid_tier: Option<Tier>,
}

#[derive(Debug, Deserialize)]
struct Tier {
    id: Option<String>,
    name: Option<String>,
}

pub(crate) fn load_antigravity_code_assist(
    access_token: &str,
    requested_project: Option<&str>,
) -> Result<LoadSnapshot, String> {
    let mut body = json!({ "metadata": { "ideType": "ANTIGRAVITY" } });
    if let Some(project) = clean(requested_project) {
        body["cloudaicompanionProject"] = Value::String(project);
    }
    let response: LoadResponse = post_json("loadCodeAssist", access_token, &body)?;
    Ok(LoadSnapshot {
        project_id: clean(requested_project)
            .or_else(|| project_from_value(response.cloudaicompanion_project.as_ref())),
        plan: plan_from_tiers(response.paid_tier.as_ref(), response.current_tier.as_ref()),
    })
}

#[cfg(test)]
pub(crate) fn load_code_assist(
    access_token: &str,
    requested_project: Option<&str>,
) -> Result<LoadSnapshot, String> {
    load_code_assist_typed(access_token, requested_project)
        .map_err(|error| error.diagnostic("loadCodeAssist"))
}

pub(crate) fn load_code_assist_typed(
    access_token: &str,
    requested_project: Option<&str>,
) -> Result<LoadSnapshot, CodeAssistError> {
    let mut metadata = json!({
        "ideType": "IDE_UNSPECIFIED",
        "platform": "PLATFORM_UNSPECIFIED",
        "pluginType": "GEMINI"
    });
    if let Some(project) = clean(requested_project) {
        metadata["duetProject"] = Value::String(project.clone());
    }
    let mut body = json!({ "metadata": metadata });
    if let Some(project) = clean(requested_project) {
        body["cloudaicompanionProject"] = Value::String(project);
    }
    let response: LoadResponse = post_json_typed("loadCodeAssist", access_token, &body)?;
    Ok(LoadSnapshot {
        project_id: clean(requested_project)
            .or_else(|| project_from_value(response.cloudaicompanion_project.as_ref())),
        plan: plan_from_tiers(response.paid_tier.as_ref(), response.current_tier.as_ref()),
    })
}

pub(crate) fn retrieve_user_quota_summary(
    access_token: &str,
    project_id: &str,
) -> Result<Value, String> {
    let project =
        clean(Some(project_id)).ok_or_else(|| "Code Assist project is unavailable".to_owned())?;
    post_json(
        "retrieveUserQuotaSummary",
        access_token,
        &json!({ "project": project }),
    )
}

pub(crate) fn retrieve_user_quota(
    access_token: &str,
    project_id: &str,
) -> Result<Vec<QuotaBucket>, String> {
    retrieve_user_quota_typed(access_token, project_id)
        .map_err(|error| error.diagnostic("retrieveUserQuota"))
}

pub(crate) fn retrieve_user_quota_typed(
    access_token: &str,
    project_id: &str,
) -> Result<Vec<QuotaBucket>, CodeAssistError> {
    let project = clean(Some(project_id)).ok_or(CodeAssistError::InvalidRequest)?;
    let response: QuotaResponse = post_json_typed(
        "retrieveUserQuota",
        access_token,
        &json!({ "project": project }),
    )?;
    Ok(response.buckets)
}

fn post_json<T>(method: &str, access_token: &str, body: &Value) -> Result<T, String>
where
    T: for<'de> Deserialize<'de>,
{
    post_json_typed(method, access_token, body).map_err(|error| error.diagnostic(method))
}

fn post_json_typed<T>(method: &str, access_token: &str, body: &Value) -> Result<T, CodeAssistError>
where
    T: for<'de> Deserialize<'de>,
{
    let token = access_token.trim();
    if token.is_empty() {
        return Err(CodeAssistError::Unauthorized);
    }
    let request = minreq::post(format!("{API_BASE}:{method}"))
        .with_header("authorization", format!("Bearer {token}"))
        .with_header("accept", "*/*")
        .with_header("content-type", "application/json")
        .with_header("user-agent", USER_AGENT)
        .with_timeout(HTTP_TIMEOUT_SECONDS)
        .with_follow_redirects(false)
        .with_json(body)
        .map_err(|_| CodeAssistError::InvalidRequest)?;
    let response = request.send().map_err(|_| CodeAssistError::Transport)?;
    match response.status_code {
        200..=299 => {}
        401 => return Err(CodeAssistError::Unauthorized),
        403 => return Err(CodeAssistError::Forbidden),
        429 => {
            return Err(CodeAssistError::RateLimited {
                retry_after_ms: retry_after_ms(&response),
            })
        }
        status => return Err(CodeAssistError::Http(status)),
    }
    response
        .json::<T>()
        .map_err(|_| CodeAssistError::InvalidPayload)
}

fn retry_after_ms(response: &minreq::Response) -> u64 {
    retry_after_header_ms(
        response.headers.get("retry-after").map(String::as_str),
        DateTime::<Utc>::from(SystemTime::now()),
    )
}

fn retry_after_header_ms(raw: Option<&str>, now: DateTime<Utc>) -> u64 {
    let Some(raw) = raw else {
        return DEFAULT_RATE_LIMIT_COOLDOWN_MS;
    };
    if let Ok(seconds) = raw.trim().parse::<u64>() {
        return seconds
            .saturating_mul(1000)
            .clamp(1_000, MAX_RATE_LIMIT_COOLDOWN_MS);
    }
    let when = DateTime::parse_from_rfc2822(raw.trim())
        .ok()
        .map(|value| value.with_timezone(&Utc))
        .or_else(|| {
            NaiveDateTime::parse_from_str(raw.trim(), "%a, %d %b %Y %H:%M:%S GMT")
                .ok()
                .map(|value| value.and_utc())
        });
    if let Some(when) = when {
        let millis = when.signed_duration_since(now).num_milliseconds();
        if millis > 0 {
            return (millis as u64).clamp(1_000, MAX_RATE_LIMIT_COOLDOWN_MS);
        }
    }
    DEFAULT_RATE_LIMIT_COOLDOWN_MS
}

fn clean(value: Option<&str>) -> Option<String> {
    let value = value?.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

fn project_from_value(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(value) => clean(Some(value)),
        Value::Object(value) => ["value", "id", "projectId"].into_iter().find_map(|key| {
            value
                .get(key)
                .and_then(Value::as_str)
                .and_then(|item| clean(Some(item)))
        }),
        _ => None,
    }
}

fn plan_from_tiers(paid: Option<&Tier>, current: Option<&Tier>) -> Option<String> {
    paid.and_then(|tier| clean(tier.name.as_deref()).or_else(|| clean(tier.id.as_deref())))
        .or_else(|| {
            current
                .and_then(|tier| clean(tier.name.as_deref()).or_else(|| clean(tier.id.as_deref())))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_extraction_accepts_current_wire_shapes_without_guessing() {
        assert_eq!(
            project_from_value(Some(&json!("project-a"))).as_deref(),
            Some("project-a")
        );
        assert_eq!(
            project_from_value(Some(&json!({"id":"project-b"}))).as_deref(),
            Some("project-b")
        );
        assert_eq!(project_from_value(Some(&json!({"unknown":"x"}))), None);
    }

    #[test]
    fn current_gemini_cli_wire_shapes_deserialize_without_model_allowlists() {
        let load: LoadResponse = serde_json::from_value(json!({
            "cloudaicompanionProject": "project-a",
            "currentTier": { "id": "free-tier", "name": "Free" }
        }))
        .expect("loadCodeAssist fixture");
        assert_eq!(
            project_from_value(load.cloudaicompanion_project.as_ref()).as_deref(),
            Some("project-a")
        );

        let quota: QuotaResponse = serde_json::from_value(json!({
            "buckets": [{
                "modelId": "gemini-future-model",
                "remainingFraction": 0.42,
                "remainingAmount": "420",
                "resetTime": "2026-09-06T00:00:00Z"
            }]
        }))
        .expect("retrieveUserQuota fixture");
        assert_eq!(quota.buckets.len(), 1);
        assert_eq!(
            quota.buckets[0].model_id.as_deref(),
            Some("gemini-future-model")
        );
        assert_eq!(quota.buckets[0].remaining_fraction, Some(0.42));
    }

    #[test]
    fn antigravity_load_shape_uses_existing_project_without_onboarding_contract() {
        let load: LoadResponse = serde_json::from_value(json!({
            "cloudaicompanionProject": { "id": "agy-project" },
            "currentTier": { "id": "standard-tier", "name": "Paid" }
        }))
        .expect("Antigravity load fixture");
        assert_eq!(
            project_from_value(load.cloudaicompanion_project.as_ref()).as_deref(),
            Some("agy-project")
        );
    }

    #[test]
    fn retry_after_seconds_and_http_date_are_bounded_and_parsed() {
        let now = DateTime::parse_from_rfc3339("2026-09-08T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(retry_after_header_ms(Some("120"), now), 120_000);
        assert_eq!(
            retry_after_header_ms(Some("Tue, 08 Sep 2026 00:02:00 GMT"), now),
            120_000
        );
        assert_eq!(
            retry_after_header_ms(Some("999999"), now),
            MAX_RATE_LIMIT_COOLDOWN_MS
        );
        assert_eq!(
            retry_after_header_ms(None, now),
            DEFAULT_RATE_LIMIT_COOLDOWN_MS
        );
    }

    #[test]
    fn plan_prefers_paid_tier_metadata() {
        let paid = Tier {
            id: Some("g1-pro-tier".into()),
            name: Some("Google AI Pro".into()),
        };
        let current = Tier {
            id: Some("free-tier".into()),
            name: Some("Free".into()),
        };
        assert_eq!(
            plan_from_tiers(Some(&paid), Some(&current)).as_deref(),
            Some("Google AI Pro")
        );
    }
}
