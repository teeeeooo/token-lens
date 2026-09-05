use serde::Deserialize;
use serde_json::{json, Value};

const API_BASE: &str = "https://cloudcode-pa.googleapis.com/v1internal";
const HTTP_TIMEOUT_SECONDS: u64 = 12;
const USER_AGENT: &str = "Token-Lens/2";

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

pub(crate) fn load_code_assist(
    access_token: &str,
    requested_project: Option<&str>,
) -> Result<LoadSnapshot, String> {
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
    let response: LoadResponse = post_json("loadCodeAssist", access_token, &body)?;
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
    let project =
        clean(Some(project_id)).ok_or_else(|| "Code Assist project is unavailable".to_owned())?;
    let response: QuotaResponse = post_json(
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
    let token = access_token.trim();
    if token.is_empty() {
        return Err("Google access token is unavailable".to_owned());
    }
    let request = minreq::post(format!("{API_BASE}:{method}"))
        .with_header("authorization", format!("Bearer {token}"))
        .with_header("accept", "*/*")
        .with_header("content-type", "application/json")
        .with_header("user-agent", USER_AGENT)
        .with_timeout(HTTP_TIMEOUT_SECONDS)
        .with_follow_redirects(false)
        .with_json(body)
        .map_err(|_| format!("Google Code Assist {method} request could not be encoded"))?;
    let response = request
        .send()
        .map_err(|_| format!("Google Code Assist {method} request failed"))?;
    if !(200..300).contains(&response.status_code) {
        return Err(format!(
            "Google Code Assist {method} returned HTTP {}",
            response.status_code
        ));
    }
    response
        .json::<T>()
        .map_err(|_| format!("Google Code Assist {method} returned an invalid payload"))
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
