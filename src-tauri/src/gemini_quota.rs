use crate::domain::{QuotaProvider, QuotaReport, QuotaWindow, QuotaWindowKind, SupportedProvider};
use crate::google_code_assist::{self, LoadSnapshot, QuotaBucket};
#[cfg(any(target_os = "windows", test))]
use aes_gcm::{
    aead::{consts::U16, AeadInPlace, KeyInit},
    aes::Aes256,
    AesGcm, Nonce, Tag,
};
#[cfg(any(target_os = "windows", test))]
use scrypt::{scrypt, Params as ScryptParams};
use serde::Deserialize;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const SOURCE: &str = "gemini-code-assist";
const TOKEN_EXPIRY_SAFETY_MS: u64 = 30_000;

#[derive(Debug, Deserialize)]
struct StoredGeminiCredential {
    access_token: Option<String>,
    expiry_date: Option<u64>,
}

#[cfg(any(target_os = "windows", test))]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredGeminiKeychainCredential {
    token: Option<StoredGeminiKeychainToken>,
}

#[cfg(any(target_os = "windows", test))]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredGeminiKeychainToken {
    access_token: Option<String>,
    expires_at: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct StoredGeminiAccounts {
    active: Option<String>,
}

pub(crate) async fn enrich_quota_report(home: &Path, report: QuotaReport) -> QuotaReport {
    let fallback = report.clone();
    let home = home.to_path_buf();
    tokio::task::spawn_blocking(move || enrich_quota_report_sync(&home, report))
        .await
        .unwrap_or(fallback)
}

fn enrich_quota_report_sync(home: &Path, mut report: QuotaReport) -> QuotaReport {
    if report
        .providers
        .iter()
        .find(|provider| provider.provider == SupportedProvider::Gemini)
        .is_some_and(provider_has_usable_quota)
    {
        return report;
    }

    let Some(credential) = read_valid_credential(home) else {
        return report;
    };
    let requested_project = configured_project();
    let load = match google_code_assist::load_code_assist(
        &credential.access_token,
        requested_project.as_deref(),
    ) {
        Ok(load) => load,
        Err(_) => return report,
    };
    let Some(project_id) = load.project_id.as_deref() else {
        // Token Lens is a monitor. Never call onboardUser to create/attach a project.
        return report;
    };
    let buckets =
        match google_code_assist::retrieve_user_quota(&credential.access_token, project_id) {
            Ok(buckets) => buckets,
            Err(_) => return report,
        };
    let windows = normalize_buckets(buckets);
    if windows.is_empty() {
        return report;
    }

    let provider = QuotaProvider {
        provider: SupportedProvider::Gemini,
        plan: normalize_plan(load),
        account_email: read_active_account(home),
        windows,
        reset_credits: None,
        credit_status: None,
        spend_control: None,
    };
    if let Some(existing) = report
        .providers
        .iter_mut()
        .find(|item| item.provider == SupportedProvider::Gemini)
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

struct ValidCredential {
    access_token: String,
}

fn read_valid_credential(home: &Path) -> Option<ValidCredential> {
    #[cfg(target_os = "windows")]
    {
        if let Some(credential) = read_windows_keychain_credential() {
            return Some(credential);
        }
        if let Some(credential) = read_windows_file_keychain_credential(home) {
            return Some(credential);
        }
    }

    let path = gemini_home(home).join("oauth_creds.json");
    let bytes = fs::read(path).ok()?;
    let raw = serde_json::from_slice::<StoredGeminiCredential>(&bytes).ok()?;
    valid_credential(raw.access_token, raw.expiry_date)
}

fn valid_credential(access_token: Option<String>, expiry: Option<u64>) -> Option<ValidCredential> {
    let access_token = access_token?.trim().to_owned();
    if access_token.is_empty() {
        return None;
    }
    if expiry.is_some_and(|value| value <= now_ms().saturating_add(TOKEN_EXPIRY_SAFETY_MS)) {
        return None;
    }
    Some(ValidCredential { access_token })
}

#[cfg(any(target_os = "windows", test))]
fn parse_keychain_credential(bytes: &[u8]) -> Option<ValidCredential> {
    let raw = serde_json::from_slice::<StoredGeminiKeychainCredential>(bytes).ok()?;
    let token = raw.token?;
    valid_credential(token.access_token, token.expires_at)
}

#[cfg(target_os = "windows")]
fn read_windows_keychain_credential() -> Option<ValidCredential> {
    use std::ffi::OsStr;
    use std::iter::once;
    use std::os::windows::ffi::OsStrExt;
    use std::ptr;
    use windows_sys::Win32::Security::Credentials::{
        CredFree, CredReadW, CREDENTIALW, CRED_TYPE_GENERIC,
    };

    // Current Gemini CLI (OAuthCredentialStorage + keytar) stores the main
    // account as service/account => `gemini-cli-oauth/main-account`. Read only:
    // Token Lens never asks Gemini CLI or Google to refresh the credential.
    let target = OsStr::new("gemini-cli-oauth/main-account")
        .encode_wide()
        .chain(once(0))
        .collect::<Vec<_>>();
    let mut credential: *mut CREDENTIALW = ptr::null_mut();
    let ok = unsafe { CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut credential) } != 0;
    if !ok || credential.is_null() {
        return None;
    }
    let bytes = unsafe {
        let item = &*credential;
        std::slice::from_raw_parts(item.CredentialBlob, item.CredentialBlobSize as usize).to_vec()
    };
    unsafe { CredFree(credential.cast()) };
    parse_keychain_credential(&bytes)
}

#[cfg(target_os = "windows")]
fn read_windows_file_keychain_credential(home: &Path) -> Option<ValidCredential> {
    let hostname = env::var("COMPUTERNAME").ok()?;
    let username = env::var("USERNAME")
        .ok()
        .or_else(|| env::var("USER").ok())?;
    let text = fs::read_to_string(gemini_home(home).join("gemini-credentials.json")).ok()?;
    parse_file_keychain_credential(&text, &hostname, &username)
}

#[cfg(any(target_os = "windows", test))]
fn parse_file_keychain_credential(
    encrypted: &str,
    hostname: &str,
    username: &str,
) -> Option<ValidCredential> {
    let plaintext = decrypt_file_keychain(encrypted, hostname, username)?;
    let store = serde_json::from_slice::<serde_json::Value>(&plaintext).ok()?;
    let secret = store
        .get("gemini-cli-oauth")?
        .get("main-account")?
        .as_str()?;
    parse_keychain_credential(secret.as_bytes())
}

#[cfg(any(target_os = "windows", test))]
fn decrypt_file_keychain(encrypted: &str, hostname: &str, username: &str) -> Option<Vec<u8>> {
    type Aes256Gcm16 = AesGcm<Aes256, U16>;
    let mut parts = encrypted.trim().split(':');
    let iv = decode_hex(parts.next()?)?;
    let tag = decode_hex(parts.next()?)?;
    let mut ciphertext = decode_hex(parts.next()?)?;
    if parts.next().is_some() || iv.len() != 16 || tag.len() != 16 {
        return None;
    }

    let salt = format!("{hostname}-{username}-gemini-cli");
    let params = ScryptParams::new(14, 8, 1, 32).ok()?;
    let mut key = [0u8; 32];
    scrypt(b"gemini-cli-oauth", salt.as_bytes(), &params, &mut key).ok()?;
    let cipher = Aes256Gcm16::new_from_slice(&key).ok()?;
    cipher
        .decrypt_in_place_detached(
            Nonce::<U16>::from_slice(&iv),
            b"",
            &mut ciphertext,
            Tag::from_slice(&tag),
        )
        .ok()?;
    Some(ciphertext)
}

#[cfg(any(target_os = "windows", test))]
fn decode_hex(value: &str) -> Option<Vec<u8>> {
    if value.len() % 2 != 0 {
        return None;
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let high = hex_digit(pair[0])?;
            let low = hex_digit(pair[1])?;
            Some((high << 4) | low)
        })
        .collect()
}

#[cfg(any(target_os = "windows", test))]
fn hex_digit(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn read_active_account(home: &Path) -> Option<String> {
    let path = gemini_home(home).join("google_accounts.json");
    let raw = serde_json::from_slice::<StoredGeminiAccounts>(&fs::read(path).ok()?).ok()?;
    let active = raw.active?.trim().to_ascii_lowercase();
    (!active.is_empty()).then_some(active)
}

fn gemini_home(home: &Path) -> PathBuf {
    env::var_os("GEMINI_CLI_HOME")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| home.join(".gemini"))
}

fn configured_project() -> Option<String> {
    env::var("GOOGLE_CLOUD_PROJECT")
        .ok()
        .or_else(|| env::var("GOOGLE_CLOUD_PROJECT_ID").ok())
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn normalize_plan(load: LoadSnapshot) -> Option<String> {
    load.plan
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn normalize_buckets(buckets: Vec<QuotaBucket>) -> Vec<QuotaWindow> {
    let mut by_model: HashMap<String, QuotaWindow> = HashMap::new();
    for bucket in buckets {
        let Some(model_id) = bucket
            .model_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        let Some(fraction) = bucket.remaining_fraction.filter(|value| value.is_finite()) else {
            continue;
        };
        let fraction = fraction.clamp(0.0, 1.0);
        let remaining_percent = fraction * 100.0;
        let used_percent = (100.0 - remaining_percent).clamp(0.0, 100.0);
        let remaining = bucket
            .remaining_amount
            .as_deref()
            .and_then(|value| value.parse::<f64>().ok())
            .filter(|value| value.is_finite() && *value >= 0.0);
        let limit =
            remaining.and_then(|value| (fraction > 0.0).then_some((value / fraction).round()));
        let used = limit
            .zip(remaining)
            .map(|(limit, remaining)| (limit - remaining).max(0.0));
        let window = QuotaWindow {
            kind: QuotaWindowKind::Other,
            label: model_id.to_owned(),
            metric: "quota",
            additional: true,
            used,
            limit,
            remaining,
            used_percent: Some(used_percent),
            remaining_percent: Some(remaining_percent),
            remaining_label: None,
            resets_at: bucket.reset_time.and_then(clean_string),
            currency: None,
            show_meter: true,
            source: SOURCE,
        };
        let replace = by_model
            .get(model_id)
            .and_then(|current| current.remaining_percent)
            .map_or(true, |current| remaining_percent < current);
        if replace {
            by_model.insert(model_id.to_owned(), window);
        }
    }
    let mut windows = by_model.into_values().collect::<Vec<_>>();
    windows.sort_by(|a, b| {
        a.remaining_percent
            .partial_cmp(&b.remaining_percent)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.label.cmp(&b.label))
    });
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

    fn bucket(model: &str, fraction: f64, amount: Option<&str>) -> QuotaBucket {
        QuotaBucket {
            model_id: Some(model.into()),
            remaining_fraction: Some(fraction),
            remaining_amount: amount.map(str::to_owned),
            reset_time: Some("2026-09-06T00:00:00Z".into()),
        }
    }

    #[test]
    fn model_buckets_are_allowlist_free_and_keep_most_constrained_duplicate() {
        let windows = normalize_buckets(vec![
            bucket("gemini-future-pro", 0.8, None),
            bucket("gemini-future-pro", 0.4, None),
            bucket("gemini-new-flash", 0.9, None),
        ]);
        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0].label, "gemini-future-pro");
        assert_eq!(windows[0].remaining_percent, Some(40.0));
        assert!(windows.iter().all(|window| window.additional));
    }

    #[test]
    fn remaining_amount_is_preserved_without_inventing_a_currency() {
        let [window] = normalize_buckets(vec![bucket("gemini-pro", 0.25, Some("250"))])
            .try_into()
            .expect("one window");
        assert_eq!(window.remaining, Some(250.0));
        assert_eq!(window.limit, Some(1000.0));
        assert_eq!(window.used, Some(750.0));
        assert_eq!(window.currency, None);
    }

    #[test]
    fn existing_tokscale_gemini_quota_remains_authoritative() {
        let provider = QuotaProvider {
            provider: SupportedProvider::Gemini,
            plan: Some("Future".into()),
            account_email: None,
            windows: vec![QuotaWindow {
                kind: QuotaWindowKind::Daily,
                label: "Daily".into(),
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

    #[tokio::test]
    #[ignore = "requires a local Gemini CLI credential; live API runs only while its access token is already valid"]
    async fn live_gemini_quota_smoke() {
        let home = env::var_os("HOME").map(PathBuf::from).expect("HOME");
        let Some(credential) = read_valid_credential(&home) else {
            return;
        };
        let load = google_code_assist::load_code_assist(
            &credential.access_token,
            configured_project().as_deref(),
        )
        .expect("load live Gemini Code Assist state");
        let Some(project) = load.project_id.as_deref() else {
            return;
        };
        let windows = normalize_buckets(
            google_code_assist::retrieve_user_quota(&credential.access_token, project)
                .expect("read live Gemini quota"),
        );
        assert!(!windows.is_empty(), "expected live Gemini quota buckets");
        assert!(windows.iter().all(|window| window.source == SOURCE));
    }

    #[test]
    fn current_gemini_keychain_shape_reads_only_access_token_and_expiry() {
        let future = now_ms() + 60_000;
        let fixture = serde_json::json!({
            "serverName": "main-account",
            "token": {
                "accessToken": "keychain-access",
                "refreshToken": "DO_NOT_READ_OR_USE",
                "tokenType": "Bearer",
                "expiresAt": future
            },
            "updatedAt": now_ms()
        });
        let bytes = serde_json::to_vec(&fixture).expect("keychain fixture");
        let parsed = parse_keychain_credential(&bytes).expect("valid current credential");
        assert_eq!(parsed.access_token, "keychain-access");
    }

    #[test]
    fn encrypted_file_keychain_matches_gemini_cli_node_crypto_format() {
        let fixture = "000102030405060708090a0b0c0d0e0f:4cce9d5c5d3fdf79b6d3b1034e119489:434b76d776b0e43555058fd747d6d635c228ccf9a0744385826972d4160076ee21c3c84fe1dca16614c0e5efb7e20428329a400d289b7e679483a07452ff96c1b16825655477f940a77b40b3177ae6424b45c25314706bc8288c19da4ac5ff53bc421b36ac7562458482d9a83520908139c74a87e54d2b2edc6b800de14dd863ea8dc4749232cfb7b818d25eabe3774350719b6d573412ec1ae06cc54817920e";
        let parsed = parse_file_keychain_credential(fixture, "TEST-HOST", "tester")
            .expect("Gemini CLI encrypted file credential should decrypt");
        assert_eq!(parsed.access_token, "test-access-token");
    }

    #[test]
    fn credential_without_expiry_can_be_validated_by_the_read_only_api_call() {
        let parsed = valid_credential(Some("existing-access-token".into()), None)
            .expect("unknown expiry should not discard an existing access token");
        assert_eq!(parsed.access_token, "existing-access-token");
    }

    #[test]
    fn expired_credentials_are_rejected_without_refreshing_or_deserializing_refresh_token() {
        let raw = serde_json::from_str::<StoredGeminiCredential>(
            r#"{"access_token":"token","expiry_date":1,"refresh_token":"DO_NOT_USE"}"#,
        )
        .expect("credential fixture");
        assert_eq!(raw.access_token.as_deref(), Some("token"));
        assert_eq!(raw.expiry_date, Some(1));
    }
}
