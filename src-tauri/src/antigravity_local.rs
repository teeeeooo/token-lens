use crate::domain::{QuotaWindow, QuotaWindowKind};
use rustls::client::{ServerCertVerified, ServerCertVerifier};
use rustls::{
    Certificate, ClientConfig, ClientConnection, Error as RustlsError, ServerName, StreamOwned,
};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::process::{Command, Stdio};
use std::sync::{Arc, OnceLock};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

const LOCAL_SOURCE: &str = "antigravity-local";
const LS_SERVICE: &str = "exa.language_server_pb.LanguageServerService";
const LOCAL_PROBE_BUDGET: Duration = Duration::from_secs(8);
const LOCAL_CALL_MAX: Duration = Duration::from_millis(1_200);
const LOCAL_RESPONSE_MAX: u64 = 2 * 1024 * 1024;
const MAX_PROCESSES: usize = 6;
const MAX_PORTS: usize = 8;
const MAX_COMMAND_OUTPUT: u64 = 4 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum ProcessKind {
    App,
    Cli,
    Ide,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProcessInfo {
    pid: u32,
    kind: ProcessKind,
    csrf_token: String,
    extension_port: Option<u16>,
    extension_csrf_token: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum LocalScheme {
    Https,
    Http,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct LocalEndpoint {
    scheme: LocalScheme,
    port: u16,
    csrf_token: String,
}

#[derive(Debug)]
struct LocalCallError {
    reached_http: bool,
}

#[derive(Debug, Default)]
pub(crate) struct Snapshot {
    pub(crate) account_plan: Option<String>,
    pub(crate) account_email: Option<String>,
    pub(crate) windows: Vec<QuotaWindow>,
}

pub(crate) fn probe() -> Option<Snapshot> {
    let deadline = Instant::now() + LOCAL_PROBE_BUDGET;
    let infos = detect_process_infos(deadline).ok()?;
    for info in infos.into_iter().take(MAX_PROCESSES) {
        if Instant::now() >= deadline {
            break;
        }
        let ports = listening_ports(info.pid, deadline).unwrap_or_default();
        let candidates = endpoint_candidates(&info, &ports);
        for endpoint in candidates {
            if Instant::now() >= deadline {
                break;
            }
            match call_local(
                &endpoint,
                "RetrieveUserQuotaSummary",
                &json!({ "forceRefresh": true }),
                deadline,
            ) {
                Ok(value) => {
                    let windows = grouped_windows(&value, LOCAL_SOURCE);
                    if !windows.is_empty() {
                        let identity = call_local(
                            &endpoint,
                            "GetUserStatus",
                            &json!({ "metadata": probe_metadata() }),
                            deadline,
                        )
                        .ok()
                        .map(|value| identity_and_legacy_windows(&value))
                        .unwrap_or_default();
                        return Some(Snapshot {
                            account_plan: identity.account_plan,
                            account_email: identity.account_email,
                            windows,
                        });
                    }
                    if let Some(snapshot) = fallback_on_reached_endpoint(&endpoint, deadline) {
                        return Some(snapshot);
                    }
                }
                Err(error) if error.reached_http => {
                    if let Some(snapshot) = fallback_on_reached_endpoint(&endpoint, deadline) {
                        return Some(snapshot);
                    }
                }
                Err(_) => continue,
            }
        }
    }
    None
}

fn fallback_on_reached_endpoint(endpoint: &LocalEndpoint, deadline: Instant) -> Option<Snapshot> {
    if let Ok(value) = call_local(
        endpoint,
        "GetUserStatus",
        &json!({ "metadata": probe_metadata() }),
        deadline,
    ) {
        let snapshot = identity_and_legacy_windows(&value);
        if !snapshot.windows.is_empty() {
            return Some(snapshot);
        }
    }
    if let Ok(value) = call_local(
        endpoint,
        "GetCommandModelConfigs",
        &json!({ "metadata": probe_metadata() }),
        deadline,
    ) {
        let windows =
            legacy_windows_from_configs(value.get("clientModelConfigs").and_then(Value::as_array));
        if !windows.is_empty() {
            return Some(Snapshot {
                windows,
                ..Snapshot::default()
            });
        }
    }
    None
}

fn probe_metadata() -> Value {
    json!({
        "ideName": "antigravity",
        "extensionName": "antigravity",
        "ideVersion": "unknown",
        "locale": "en"
    })
}

fn detect_process_infos(deadline: Instant) -> Result<Vec<ProcessInfo>, String> {
    let text = if cfg!(windows) {
        let script = r#"Get-CimInstance Win32_Process | Where-Object { $_.Name -like 'language_server*' -or $_.Name -like 'language-server*' -or $_.Name -like 'agy*' -or $_.Name -like 'antigravity*' } | ForEach-Object { \"$($_.ProcessId) $($_.CommandLine)\" }"#;
        command_text(
            "powershell",
            &["-NoProfile", "-NonInteractive", "-Command", script],
            deadline,
            Duration::from_secs(4),
        )?
    } else {
        command_text(
            "ps",
            &["-ax", "-o", "pid=,command="],
            deadline,
            Duration::from_secs(4),
        )?
    };
    let mut infos = text
        .lines()
        .filter_map(parse_process_line)
        .collect::<Vec<_>>();
    infos.sort_by_key(|info| (info.kind, info.pid));
    infos.dedup_by_key(|info| info.pid);
    if infos.is_empty() {
        return Err("Antigravity language server is not running".to_owned());
    }
    Ok(infos)
}

fn parse_process_line(line: &str) -> Option<ProcessInfo> {
    let line = line.trim();
    let split = line.find(char::is_whitespace)?;
    let pid = line[..split].parse::<u32>().ok()?;
    let command = line[split..].trim();
    if command.is_empty() {
        return None;
    }
    let lower = command.to_ascii_lowercase();
    let language_server = lower.contains("language_server") || lower.contains("language-server");
    let ide = language_server
        && (lower.contains("antigravity ide.app")
            || lower.contains("antigravity-ide")
            || lower.contains("/extensions/antigravity/bin/language_server")
            || lower.contains("\\extensions\\antigravity\\bin\\language_server"));
    let cli = lower.contains("/antigravity-cli")
        || lower.contains("\\antigravity-cli")
        || lower.contains("/antigravity_cli")
        || lower.contains("\\antigravity_cli")
        || executable_name(&lower).is_some_and(|name| name == "agy" || name == "agy.exe");
    let app = language_server
        && !ide
        && (lower.contains("--app_data_dir antigravity")
            || lower.contains("--app_data_dir=antigravity")
            || lower.contains("/antigravity.app/")
            || lower.contains("\\antigravity.app\\")
            || lower.contains("/antigravity/"));
    let kind = if app {
        ProcessKind::App
    } else if cli {
        ProcessKind::Cli
    } else if ide {
        ProcessKind::Ide
    } else {
        return None;
    };
    let csrf_token = flag_value(command, "--csrf_token").unwrap_or_default();
    if kind != ProcessKind::Cli && csrf_token.is_empty() {
        return None;
    }
    Some(ProcessInfo {
        pid,
        kind,
        csrf_token,
        extension_port: flag_value(command, "--extension_server_port")
            .and_then(|value| value.parse::<u16>().ok())
            .filter(|port| *port > 0),
        extension_csrf_token: flag_value(command, "--extension_server_csrf_token"),
    })
}

fn executable_name(command: &str) -> Option<&str> {
    let command = command.trim_start();
    let executable = if let Some(rest) = command.strip_prefix('"') {
        rest.split_once('"').map(|(value, _)| value)?
    } else {
        command.split_whitespace().next()?
    };
    executable.rsplit(['/', '\\']).next()
}

fn flag_value(command: &str, flag: &str) -> Option<String> {
    let parts = command.split_whitespace().collect::<Vec<_>>();
    for (index, part) in parts.iter().enumerate() {
        let part = part.trim_matches('"');
        if let Some(value) = part.strip_prefix(&format!("{flag}=")) {
            return clean_string(value.to_owned());
        }
        if part == flag {
            return parts
                .get(index + 1)
                .and_then(|value| clean_string(value.trim_matches('"').to_owned()));
        }
    }
    None
}

fn command_text(
    command: &str,
    args: &[&str],
    deadline: Instant,
    maximum: Duration,
) -> Result<String, String> {
    let now = Instant::now();
    if now >= deadline {
        return Err(format!("{command} timed out"));
    }
    let timeout = (deadline - now).min(maximum);
    let mut child = Command::new(command)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| format!("failed to run {command}"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| format!("{command} has no stdout"))?;
    let reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stdout.take(MAX_COMMAND_OUTPUT + 1).read_to_end(&mut bytes);
        bytes
    });
    let command_deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let bytes = reader
                    .join()
                    .map_err(|_| format!("{command} output failed"))?;
                if !status.success() || bytes.len() as u64 > MAX_COMMAND_OUTPUT {
                    return Err(format!("{command} failed"));
                }
                return String::from_utf8(bytes)
                    .map_err(|_| format!("{command} returned invalid UTF-8"));
            }
            Ok(None) if Instant::now() < command_deadline => {
                thread::sleep(Duration::from_millis(10));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = reader.join();
                return Err(format!("{command} timed out"));
            }
        }
    }
}

fn listening_ports(pid: u32, deadline: Instant) -> Result<Vec<u16>, String> {
    let text = if cfg!(windows) {
        let script = format!("Get-NetTCPConnection -OwningProcess {pid} -State Listen -ErrorAction SilentlyContinue | Select-Object -ExpandProperty LocalPort");
        command_text(
            "powershell",
            &["-NoProfile", "-NonInteractive", "-Command", &script],
            deadline,
            Duration::from_secs(2),
        )?
    } else {
        command_text(
            "lsof",
            &["-nP", "-iTCP", "-sTCP:LISTEN", "-a", "-p", &pid.to_string()],
            deadline,
            Duration::from_secs(2),
        )?
    };
    let mut ports = HashSet::new();
    for line in text.lines() {
        if cfg!(windows) {
            if let Ok(port) = line.trim().parse::<u16>() {
                if port > 0 {
                    ports.insert(port);
                }
            }
            continue;
        }
        if !line.contains("(LISTEN)") {
            continue;
        }
        let before = line
            .split_whitespace()
            .find(|part| part.contains(':') && part.chars().any(|c| c.is_ascii_digit()));
        if let Some(part) = before {
            if let Some(raw) = part.rsplit(':').next() {
                if let Ok(port) = raw.parse::<u16>() {
                    if port > 0 {
                        ports.insert(port);
                    }
                }
            }
        }
    }
    let mut ports = ports.into_iter().collect::<Vec<_>>();
    ports.sort_unstable();
    ports.truncate(MAX_PORTS);
    Ok(ports)
}

fn endpoint_candidates(info: &ProcessInfo, ports: &[u16]) -> Vec<LocalEndpoint> {
    let mut result = Vec::new();
    let mut seen = HashSet::new();
    for port in ports.iter().copied().take(MAX_PORTS) {
        for scheme in [LocalScheme::Https, LocalScheme::Http] {
            let endpoint = LocalEndpoint {
                scheme,
                port,
                csrf_token: info.csrf_token.clone(),
            };
            if seen.insert((scheme, port, endpoint.csrf_token.clone())) {
                result.push(endpoint);
            }
        }
    }
    if let Some(port) = info.extension_port {
        let token = info
            .extension_csrf_token
            .clone()
            .unwrap_or_else(|| info.csrf_token.clone());
        let endpoint = LocalEndpoint {
            scheme: LocalScheme::Http,
            port,
            csrf_token: token,
        };
        if seen.insert((LocalScheme::Http, port, endpoint.csrf_token.clone())) {
            result.push(endpoint);
        }
    }
    result
}

struct LoopbackCertificateVerifier;

impl ServerCertVerifier for LoopbackCertificateVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &Certificate,
        _intermediates: &[Certificate],
        _server_name: &ServerName,
        _scts: &mut dyn Iterator<Item = &[u8]>,
        _ocsp_response: &[u8],
        _now: SystemTime,
    ) -> Result<ServerCertVerified, RustlsError> {
        Ok(ServerCertVerified::assertion())
    }
}

fn loopback_tls_config() -> Arc<ClientConfig> {
    static CONFIG: OnceLock<Arc<ClientConfig>> = OnceLock::new();
    CONFIG
        .get_or_init(|| {
            Arc::new(
                ClientConfig::builder()
                    .with_safe_defaults()
                    .with_custom_certificate_verifier(Arc::new(LoopbackCertificateVerifier))
                    .with_no_client_auth(),
            )
        })
        .clone()
}

fn call_local(
    endpoint: &LocalEndpoint,
    method: &str,
    body: &Value,
    deadline: Instant,
) -> Result<Value, LocalCallError> {
    let now = Instant::now();
    if now >= deadline {
        return Err(LocalCallError {
            reached_http: false,
        });
    }
    let timeout = (deadline - now).min(LOCAL_CALL_MAX);
    let address = SocketAddr::from(([127, 0, 0, 1], endpoint.port));
    let tcp = TcpStream::connect_timeout(&address, timeout).map_err(|_| LocalCallError {
        reached_http: false,
    })?;
    tcp.set_read_timeout(Some(timeout)).ok();
    tcp.set_write_timeout(Some(timeout)).ok();

    let payload = serde_json::to_vec(body).map_err(|_| LocalCallError {
        reached_http: false,
    })?;
    let request = format!(
        "POST /{LS_SERVICE}/{method} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnect-Protocol-Version: 1\r\nX-Codeium-Csrf-Token: {}\r\nUser-Agent: Token-Lens/2\r\nConnection: close\r\n\r\n",
        endpoint.port,
        payload.len(),
        endpoint.csrf_token
    );
    let mut bytes = request.into_bytes();
    bytes.extend_from_slice(&payload);

    let response = match endpoint.scheme {
        LocalScheme::Http => exchange_stream(tcp, &bytes),
        LocalScheme::Https => {
            let name = ServerName::try_from("localhost").map_err(|_| LocalCallError {
                reached_http: false,
            })?;
            let connection =
                ClientConnection::new(loopback_tls_config(), name).map_err(|_| LocalCallError {
                    reached_http: false,
                })?;
            exchange_stream(StreamOwned::new(connection, tcp), &bytes)
        }
    }?;
    parse_http_json(&response)
}

fn exchange_stream<S: Read + Write>(
    mut stream: S,
    request: &[u8],
) -> Result<Vec<u8>, LocalCallError> {
    stream.write_all(request).map_err(|_| LocalCallError {
        reached_http: false,
    })?;
    stream.flush().ok();
    let mut response = Vec::new();
    stream
        .take(LOCAL_RESPONSE_MAX + 1)
        .read_to_end(&mut response)
        .map_err(|_| LocalCallError {
            reached_http: false,
        })?;
    if response.len() as u64 > LOCAL_RESPONSE_MAX {
        return Err(LocalCallError { reached_http: true });
    }
    Ok(response)
}

fn parse_http_json(response: &[u8]) -> Result<Value, LocalCallError> {
    let Some(split) = response.windows(4).position(|value| value == b"\r\n\r\n") else {
        return Err(LocalCallError {
            reached_http: false,
        });
    };
    let headers = String::from_utf8_lossy(&response[..split + 4]);
    let status = headers
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|value| value.parse::<u16>().ok())
        .ok_or(LocalCallError {
            reached_http: false,
        })?;
    if status != 200 {
        return Err(LocalCallError { reached_http: true });
    }
    let mut body = response[split + 4..].to_vec();
    if headers
        .to_ascii_lowercase()
        .contains("transfer-encoding: chunked")
    {
        body = decode_chunked(&body).ok_or(LocalCallError { reached_http: true })?;
    }
    serde_json::from_slice(&body).map_err(|_| LocalCallError { reached_http: true })
}

fn decode_chunked(input: &[u8]) -> Option<Vec<u8>> {
    let mut cursor = 0usize;
    let mut output = Vec::new();
    while cursor < input.len() {
        let line_end = input[cursor..].windows(2).position(|v| v == b"\r\n")? + cursor;
        let size_text = std::str::from_utf8(&input[cursor..line_end])
            .ok()?
            .split(';')
            .next()?
            .trim();
        let size = usize::from_str_radix(size_text, 16).ok()?;
        cursor = line_end + 2;
        if size == 0 {
            return Some(output);
        }
        let end = cursor.checked_add(size)?;
        if end + 2 > input.len() || &input[end..end + 2] != b"\r\n" {
            return None;
        }
        output.extend_from_slice(&input[cursor..end]);
        if output.len() as u64 > LOCAL_RESPONSE_MAX {
            return None;
        }
        cursor = end + 2;
    }
    None
}

pub(crate) fn grouped_windows(payload: &Value, source: &'static str) -> Vec<QuotaWindow> {
    let summary = payload
        .get("response")
        .or_else(|| payload.get("summary"))
        .unwrap_or(payload);
    let groups = summary.get("groups").and_then(Value::as_array);
    let mut windows = Vec::new();
    for group in groups.into_iter().flatten() {
        let group_name = quota_group_name(group.get("displayName").and_then(Value::as_str));
        for bucket in group
            .get("buckets")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let Some(kind) = quota_bucket_kind(bucket) else {
                continue;
            };
            let disabled = bucket.get("disabled").and_then(Value::as_bool) == Some(true);
            let remaining = quota_remaining_fraction(bucket).map(|value| value.clamp(0.0, 1.0));
            let remaining_percent = if disabled {
                None
            } else {
                remaining.map(|value| value * 100.0)
            };
            let used_percent = remaining_percent.map(|value| 100.0 - value);
            let cadence = if kind == QuotaWindowKind::Session {
                "5-hour"
            } else {
                "Weekly"
            };
            windows.push(QuotaWindow {
                kind,
                label: format!("{group_name} {cadence}"),
                metric: "quota",
                additional: false,
                used: None,
                limit: None,
                remaining: None,
                used_percent,
                remaining_percent,
                remaining_label: None,
                resets_at: parse_reset_time(bucket.get("resetTime")),
                currency: None,
                show_meter: !disabled && remaining_percent.is_some(),
                source,
            });
        }
    }
    windows.sort_by_key(window_rank);
    windows
}

fn quota_group_name(value: Option<&str>) -> String {
    let name = value.unwrap_or("").trim();
    let lower = name.to_ascii_lowercase();
    if lower.contains("gemini") {
        "Gemini".to_owned()
    } else if lower.contains("claude") || lower.contains("gpt") {
        "Claude + GPT".to_owned()
    } else if name.is_empty() {
        "Quota".to_owned()
    } else {
        name.chars().take(80).collect()
    }
}

fn quota_bucket_kind(bucket: &Value) -> Option<QuotaWindowKind> {
    for key in ["window", "bucketId", "displayName"] {
        let Some(raw) = bucket.get(key).and_then(Value::as_str) else {
            continue;
        };
        let normalized = raw.trim().to_ascii_lowercase().replace('_', "-");
        if normalized == "weekly"
            || normalized.ends_with("-weekly")
            || normalized.contains("weekly")
        {
            return Some(QuotaWindowKind::Weekly);
        }
        if normalized == "session"
            || normalized == "5h"
            || normalized.contains("5-hour")
            || normalized.contains("five-hour")
            || normalized.contains("five hour")
        {
            return Some(QuotaWindowKind::Session);
        }
    }
    None
}

fn quota_remaining_fraction(bucket: &Value) -> Option<f64> {
    bucket
        .get("remainingFraction")
        .and_then(Value::as_f64)
        .or_else(|| {
            bucket
                .get("remaining")
                .and_then(|remaining| remaining.get("remainingFraction"))
                .and_then(Value::as_f64)
        })
        .or_else(|| {
            let remaining = bucket.get("remaining")?;
            (remaining.get("case").and_then(Value::as_str) == Some("remainingFraction"))
                .then(|| remaining.get("value").and_then(Value::as_f64))
                .flatten()
        })
        .filter(|value| value.is_finite())
}

fn identity_and_legacy_windows(payload: &Value) -> Snapshot {
    let status = payload.get("userStatus").unwrap_or(payload);
    let account_email = status
        .get("email")
        .and_then(Value::as_str)
        .and_then(|value| clean_string(value.to_ascii_lowercase()));
    let account_plan = status
        .pointer("/userTier/name")
        .and_then(Value::as_str)
        .or_else(|| {
            status
                .pointer("/planStatus/planInfo/planDisplayName")
                .and_then(Value::as_str)
        })
        .or_else(|| {
            status
                .pointer("/planStatus/planInfo/displayName")
                .and_then(Value::as_str)
        })
        .or_else(|| {
            status
                .pointer("/planStatus/planInfo/productName")
                .and_then(Value::as_str)
        })
        .and_then(|value| clean_string(value.to_owned()));
    let configs = status
        .pointer("/cascadeModelConfigData/clientModelConfigs")
        .and_then(Value::as_array);
    Snapshot {
        account_plan,
        account_email,
        windows: legacy_windows_from_configs(configs),
    }
}

fn legacy_windows_from_configs(configs: Option<&Vec<Value>>) -> Vec<QuotaWindow> {
    let mut families: HashMap<&'static str, (f64, Option<String>)> = HashMap::new();
    for config in configs.into_iter().flatten() {
        let model = config
            .pointer("/modelOrAlias/model")
            .and_then(Value::as_str)
            .unwrap_or("");
        let label = config.get("label").and_then(Value::as_str).unwrap_or(model);
        let Some(family) = model_family(label, model) else {
            continue;
        };
        let Some(fraction) = config
            .pointer("/quotaInfo/remainingFraction")
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite())
        else {
            continue;
        };
        let fraction = fraction.clamp(0.0, 1.0);
        let reset = parse_reset_time(config.pointer("/quotaInfo/resetTime"));
        let replace = families
            .get(family)
            .map_or(true, |(current, _)| fraction < *current);
        if replace {
            families.insert(family, (fraction, reset));
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
            source: LOCAL_SOURCE,
        })
        .collect::<Vec<_>>();
    windows.sort_by_key(window_rank);
    windows
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

fn parse_reset_time(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(value) => clean_string(value.clone()),
        Value::Number(value) => {
            let raw = value.as_i64()?;
            let seconds = if raw > 20_000_000_000 {
                raw / 1000
            } else {
                raw
            };
            chrono::DateTime::<chrono::Utc>::from_timestamp(seconds, 0)
                .map(|value| value.to_rfc3339())
        }
        _ => None,
    }
}

fn clean_string(value: String) -> Option<String> {
    let value = value.trim().to_owned();
    (!value.is_empty()).then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_parser_distinguishes_app_cli_and_ide_and_requires_desktop_csrf() {
        let app = parse_process_line("101 /Applications/Antigravity.app/Contents/MacOS/language_server --app_data_dir antigravity --csrf_token secret --extension_server_port 4010").unwrap();
        assert_eq!(app.kind, ProcessKind::App);
        assert_eq!(app.extension_port, Some(4010));
        assert!(parse_process_line("102 /Applications/Antigravity.app/Contents/MacOS/language_server --app_data_dir antigravity").is_none());

        let cli = parse_process_line("103 /opt/homebrew/bin/agy").unwrap();
        assert_eq!(cli.kind, ProcessKind::Cli);
        assert!(cli.csrf_token.is_empty());

        let windows_cli =
            parse_process_line(r#"105 "C:\Program Files\Antigravity\agy.exe""#).unwrap();
        assert_eq!(windows_cli.kind, ProcessKind::Cli);

        let ide = parse_process_line("104 /Applications/Antigravity IDE.app/Contents/extensions/antigravity/bin/language_server --app_data_dir=antigravity-ide --csrf_token ide-secret").unwrap();
        assert_eq!(ide.kind, ProcessKind::Ide);
    }

    #[test]
    fn grouped_summary_preserves_two_families_and_two_cadences() {
        let payload = json!({ "response": { "groups": [
            { "displayName": "Gemini Models", "buckets": [
                { "bucketId": "gemini_5-hour", "remaining": { "remainingFraction": 0.25 }, "resetTime": "2026-09-05T10:00:00Z" },
                { "bucketId": "gemini_weekly", "remainingFraction": 0.75, "resetTime": "2026-09-08T10:00:00Z" }
            ]},
            { "displayName": "Claude and GPT models", "buckets": [
                { "displayName": "5-hour limit", "remainingFraction": 0.40 },
                { "window": "weekly", "remainingFraction": 0.60 }
            ]}
        ]}});
        let windows = grouped_windows(&payload, LOCAL_SOURCE);
        assert_eq!(windows.len(), 4);
        assert_eq!(windows[0].label, "Gemini 5-hour");
        assert_eq!(windows[0].kind, QuotaWindowKind::Session);
        assert_eq!(windows[0].remaining_percent, Some(25.0));
        assert_eq!(windows[1].kind, QuotaWindowKind::Weekly);
        assert_eq!(windows[2].label, "Claude + GPT 5-hour");
        assert!(windows.iter().all(|window| window.source == LOCAL_SOURCE));
    }

    #[test]
    fn legacy_models_collapse_to_real_families_without_placeholder_guessing() {
        let configs = vec![
            json!({"label":"Gemini Pro","modelOrAlias":{"model":"gemini-3-pro"},"quotaInfo":{"remainingFraction":0.7}}),
            json!({"label":"Gemini Flash","modelOrAlias":{"model":"gemini-3-flash"},"quotaInfo":{"remainingFraction":0.3}}),
            json!({"label":"Claude Opus","modelOrAlias":{"model":"claude-opus"},"quotaInfo":{"remainingFraction":0.5}}),
            json!({"label":"Placeholder","modelOrAlias":{"model":"MODEL_CHAT_20706"},"quotaInfo":{"remainingFraction":0.0}}),
        ];
        let windows = legacy_windows_from_configs(Some(&configs));
        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0].label, "Gemini");
        assert_eq!(windows[0].remaining_percent, Some(30.0));
        assert_eq!(windows[1].label, "Claude + GPT");
        assert_eq!(windows[1].remaining_percent, Some(50.0));
    }

    #[test]
    fn loopback_http_transport_posts_connect_json_without_external_host_surface() {
        use std::net::TcpListener;
        use std::thread;

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind fixture");
        let port = listener.local_addr().unwrap().port();
        let server = thread::spawn(move || {
            let (mut socket, _) = listener.accept().expect("accept fixture");
            let mut request = [0u8; 4096];
            let size = socket.read(&mut request).expect("read request");
            let request = String::from_utf8_lossy(&request[..size]);
            assert!(request.starts_with(&format!(
                "POST /{LS_SERVICE}/RetrieveUserQuotaSummary HTTP/1.1"
            )));
            assert!(request.contains("Connect-Protocol-Version: 1"));
            let body = r#"{"response":{"groups":[]}}"#;
            write!(
                socket,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .expect("write response");
        });
        let endpoint = LocalEndpoint {
            scheme: LocalScheme::Http,
            port,
            csrf_token: "fixture".into(),
        };
        let value = call_local(
            &endpoint,
            "RetrieveUserQuotaSummary",
            &json!({"forceRefresh": true}),
            Instant::now() + Duration::from_secs(2),
        )
        .expect("local HTTP fixture");
        assert!(value.pointer("/response/groups").is_some());
        server.join().expect("join fixture");
    }

    #[test]
    #[ignore = "requires a running local Antigravity app, CLI, or IDE language server"]
    fn live_antigravity_local_quota_smoke() {
        if detect_process_infos(Instant::now() + LOCAL_PROBE_BUDGET).is_err() {
            return;
        }
        let snapshot = probe().expect("read live Antigravity local quota");
        assert!(
            !snapshot.windows.is_empty(),
            "expected at least one live Antigravity quota window"
        );
    }

    #[test]
    fn chunked_decoder_accepts_normal_http_chunks() {
        assert_eq!(
            decode_chunked(b"4\r\nWiki\r\n5\r\npedia\r\n0\r\n\r\n"),
            Some(b"Wikipedia".to_vec())
        );
    }
}
