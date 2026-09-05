use crate::domain::{
    SessionDetailReport, SessionDetailTotals, SessionExchangeDetail, SessionTokenBreakdown,
    SessionTurnDetail,
};
use crate::session_metadata::{resolve_session_file, valid_session_id};
use chrono::DateTime;
use serde::de::{IgnoredAny, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

const SESSION_DETAIL_SOURCE: &str = "provider-transcript";
const SUPPORTED_CLIENTS: [&str; 2] = ["codex", "claude"];
const MAX_TOOL_NAME_CHARS: usize = 120;
const MAX_TOOLS_PER_TURN: usize = 64;

#[derive(Debug, Clone)]
struct TurnBuilder {
    timestamp: String,
    tokens: SessionTokenBreakdown,
    tools: Vec<String>,
}

#[derive(Debug, Clone, Default)]
struct ExchangeBuilder {
    boundary_timestamp: String,
    turns: Vec<TurnBuilder>,
}

#[derive(Debug, Default)]
struct ClaudeContentSummary {
    has_tool_result: bool,
    tools: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ClaudeBlockMeta {
    #[serde(rename = "type")]
    block_type: Option<String>,
    name: Option<String>,
}

fn deserialize_claude_content<'de, D>(deserializer: D) -> Result<ClaudeContentSummary, D::Error>
where
    D: Deserializer<'de>,
{
    struct ContentVisitor;
    impl<'de> Visitor<'de> for ContentVisitor {
        type Value = ClaudeContentSummary;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("Claude message content")
        }

        fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let mut summary = ClaudeContentSummary::default();
            while let Some(block) = sequence.next_element::<ClaudeBlockMeta>()? {
                match block.block_type.as_deref() {
                    Some("tool_result") => summary.has_tool_result = true,
                    Some("tool_use") => push_unique(&mut summary.tools, block.name),
                    _ => {}
                }
            }
            Ok(summary)
        }

        fn visit_str<E>(self, _value: &str) -> Result<Self::Value, E> {
            Ok(ClaudeContentSummary::default())
        }

        fn visit_string<E>(self, _value: String) -> Result<Self::Value, E> {
            Ok(ClaudeContentSummary::default())
        }

        fn visit_none<E>(self) -> Result<Self::Value, E> {
            Ok(ClaudeContentSummary::default())
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E> {
            Ok(ClaudeContentSummary::default())
        }

        fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
        where
            A: MapAccess<'de>,
        {
            while map.next_entry::<String, IgnoredAny>()?.is_some() {}
            Ok(ClaudeContentSummary::default())
        }
    }

    deserializer.deserialize_any(ContentVisitor)
}

#[derive(Debug, Deserialize)]
struct ClaudeLineMeta {
    #[serde(rename = "type")]
    event_type: Option<String>,
    timestamp: Option<String>,
    uuid: Option<String>,
    #[serde(rename = "isMeta", default)]
    is_meta: bool,
    message: Option<ClaudeMessageMeta>,
}

#[derive(Debug, Deserialize)]
struct ClaudeMessageMeta {
    id: Option<String>,
    usage: Option<ClaudeUsage>,
    #[serde(default, deserialize_with = "deserialize_claude_content")]
    content: ClaudeContentSummary,
}

#[derive(Debug, Default, Deserialize)]
struct ClaudeUsage {
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
    #[serde(default)]
    cache_read_input_tokens: u64,
    #[serde(default)]
    cache_creation_input_tokens: u64,
}

#[derive(Debug, Deserialize)]
struct CodexLineMeta {
    #[serde(rename = "type")]
    outer_type: Option<String>,
    timestamp: Option<String>,
    payload: Option<CodexPayloadMeta>,
}

#[derive(Debug, Deserialize)]
struct CodexPayloadMeta {
    #[serde(rename = "type")]
    event_type: Option<String>,
    name: Option<String>,
    tool_name: Option<String>,
    tool: Option<String>,
    info: Option<CodexTokenInfo>,
    invocation: Option<CodexInvocation>,
}

#[derive(Debug, Deserialize)]
struct CodexTokenInfo {
    last_token_usage: Option<CodexUsage>,
}

#[derive(Debug, Default, Deserialize)]
struct CodexUsage {
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
    #[serde(default)]
    cached_input_tokens: u64,
    #[serde(default)]
    cache_write_input_tokens: u64,
    #[serde(default)]
    reasoning_output_tokens: u64,
}

#[derive(Debug, Deserialize)]
struct CodexInvocation {
    server: Option<String>,
    tool: Option<String>,
}

pub fn read(
    home: &Path,
    client: &str,
    session_id: &str,
    start_time_ms: Option<i64>,
    session_cost: f64,
) -> SessionDetailReport {
    let client = client.trim().to_ascii_lowercase();
    let session_id = session_id.trim().to_owned();
    if !SUPPORTED_CLIENTS.contains(&client.as_str()) || !valid_session_id(&session_id) {
        return empty_report(client, session_id, false);
    }
    let Some(path) = resolve_session_file(home, &client, &session_id) else {
        return empty_report(client, session_id, false);
    };
    let Ok(file) = File::open(path) else {
        return empty_report(client, session_id, false);
    };
    let reader = BufReader::new(file);
    let builders = match client.as_str() {
        "claude" => parse_claude(reader),
        "codex" => parse_codex(reader),
        _ => Vec::new(),
    };
    build_report(
        client,
        session_id,
        builders,
        start_time_ms,
        sanitize_cost(session_cost),
    )
}

fn empty_report(client: String, session_id: String, found: bool) -> SessionDetailReport {
    SessionDetailReport {
        found,
        client,
        session_id,
        exchanges: Vec::new(),
        totals: SessionDetailTotals {
            total_tokens: 0,
            cost_usd: 0.0,
            exchange_count: 0,
            turn_count: 0,
        },
        source: SESSION_DETAIL_SOURCE,
    }
}

fn parse_claude<R: BufRead>(reader: R) -> Vec<ExchangeBuilder> {
    let mut exchanges = Vec::<ExchangeBuilder>::new();
    let mut seen_uuids = HashSet::new();
    let mut message_turns = HashMap::<String, (usize, usize)>::new();

    for line in reader.lines().map_while(Result::ok) {
        let Ok(event) = serde_json::from_str::<ClaudeLineMeta>(&line) else {
            continue;
        };
        if let Some(uuid) = event
            .uuid
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
        {
            if !seen_uuids.insert(uuid.to_owned()) {
                continue;
            }
        }
        let timestamp = event.timestamp.unwrap_or_default();
        match event.event_type.as_deref() {
            Some("user") => {
                if event.is_meta {
                    continue;
                }
                let content = event
                    .message
                    .map(|message| message.content)
                    .unwrap_or_default();
                if !content.has_tool_result {
                    exchanges.push(ExchangeBuilder {
                        boundary_timestamp: timestamp,
                        turns: Vec::new(),
                    });
                }
            }
            Some("assistant") => {
                let Some(message) = event.message else {
                    continue;
                };
                let Some(usage) = message.usage else {
                    continue;
                };
                if let Some(id) = message
                    .id
                    .as_deref()
                    .map(str::trim)
                    .filter(|v| !v.is_empty())
                {
                    if let Some(&(exchange_index, turn_index)) = message_turns.get(id) {
                        if let Some(turn) = exchanges
                            .get_mut(exchange_index)
                            .and_then(|exchange| exchange.turns.get_mut(turn_index))
                        {
                            merge_unique(&mut turn.tools, message.content.tools);
                        }
                        continue;
                    }
                }
                let tokens = claude_tokens(usage);
                let exchange_index = ensure_exchange(&mut exchanges, &timestamp);
                let turn_index = exchanges[exchange_index].turns.len();
                exchanges[exchange_index].turns.push(TurnBuilder {
                    timestamp,
                    tokens,
                    tools: message.content.tools,
                });
                if let Some(id) = message
                    .id
                    .map(|value| value.trim().to_owned())
                    .filter(|v| !v.is_empty())
                {
                    message_turns.insert(id, (exchange_index, turn_index));
                }
            }
            _ => {}
        }
    }
    exchanges
}

fn parse_codex<R: BufRead>(reader: R) -> Vec<ExchangeBuilder> {
    let mut exchanges = Vec::<ExchangeBuilder>::new();
    let mut pending_tools = Vec::<String>::new();

    for line in reader.lines().map_while(Result::ok) {
        let Ok(event) = serde_json::from_str::<CodexLineMeta>(&line) else {
            continue;
        };
        let outer = event.outer_type.as_deref().unwrap_or_default();
        let Some(payload) = event.payload else {
            continue;
        };
        let kind = payload.event_type.as_deref().unwrap_or_default();
        if outer == "response_item"
            && matches!(
                kind,
                "function_call" | "custom_tool_call" | "tool_search_call"
            )
        {
            push_unique(&mut pending_tools, codex_tool_name(&payload));
            continue;
        }
        if outer == "event_msg" && kind == "mcp_tool_call_end" {
            push_unique(&mut pending_tools, codex_mcp_tool_name(&payload));
            continue;
        }
        let timestamp = event.timestamp.unwrap_or_default();
        if outer == "event_msg" && kind == "user_message" {
            exchanges.push(ExchangeBuilder {
                boundary_timestamp: timestamp,
                turns: Vec::new(),
            });
            continue;
        }
        if outer != "event_msg" || kind != "token_count" {
            continue;
        }
        let Some(usage) = payload.info.and_then(|info| info.last_token_usage) else {
            continue;
        };
        let tokens = codex_tokens(usage);
        if tokens.total == 0 {
            pending_tools.clear();
            continue;
        }
        let exchange_index = ensure_exchange(&mut exchanges, &timestamp);
        exchanges[exchange_index].turns.push(TurnBuilder {
            timestamp,
            tokens,
            tools: std::mem::take(&mut pending_tools),
        });
    }
    exchanges
}

fn ensure_exchange(exchanges: &mut Vec<ExchangeBuilder>, timestamp: &str) -> usize {
    if exchanges.is_empty() {
        exchanges.push(ExchangeBuilder {
            boundary_timestamp: timestamp.to_owned(),
            turns: Vec::new(),
        });
    }
    exchanges.len() - 1
}

fn claude_tokens(usage: ClaudeUsage) -> SessionTokenBreakdown {
    let total = usage
        .input_tokens
        .saturating_add(usage.output_tokens)
        .saturating_add(usage.cache_read_input_tokens)
        .saturating_add(usage.cache_creation_input_tokens);
    SessionTokenBreakdown {
        input: usage.input_tokens,
        output: usage.output_tokens,
        cache_read: usage.cache_read_input_tokens,
        cache_write: usage.cache_creation_input_tokens,
        reasoning: 0,
        total,
    }
}

fn codex_tokens(usage: CodexUsage) -> SessionTokenBreakdown {
    let cached = usage.cached_input_tokens;
    let cache_write = usage.cache_write_input_tokens;
    let input = usage
        .input_tokens
        .saturating_sub(cached.saturating_add(cache_write));
    // Raw Codex output_tokens includes reasoning_output_tokens. v2 normalizes reasoning as a
    // disjoint additive bucket, matching tokScale's renderer-facing usage contract.
    let reasoning = usage.reasoning_output_tokens;
    let output = usage.output_tokens.saturating_sub(reasoning);
    let total = input
        .saturating_add(output)
        .saturating_add(cached)
        .saturating_add(cache_write)
        .saturating_add(reasoning);
    SessionTokenBreakdown {
        input,
        output,
        cache_read: cached,
        cache_write,
        reasoning,
        total,
    }
}

fn codex_tool_name(payload: &CodexPayloadMeta) -> Option<String> {
    [
        payload.name.as_deref(),
        payload.tool_name.as_deref(),
        payload.tool.as_deref(),
    ]
    .into_iter()
    .flatten()
    .map(str::trim)
    .find(|value| !value.is_empty())
    .map(str::to_owned)
}

fn codex_mcp_tool_name(payload: &CodexPayloadMeta) -> Option<String> {
    if let Some(invocation) = payload.invocation.as_ref() {
        let server = invocation.server.as_deref().unwrap_or_default().trim();
        let tool = invocation.tool.as_deref().unwrap_or_default().trim();
        if !tool.is_empty() {
            return Some(if server.is_empty() {
                tool.to_owned()
            } else {
                format!("{server}/{tool}")
            });
        }
    }
    codex_tool_name(payload)
}

fn build_report(
    client: String,
    session_id: String,
    builders: Vec<ExchangeBuilder>,
    start_time_ms: Option<i64>,
    session_cost: f64,
) -> SessionDetailReport {
    let mut exchanges = Vec::<SessionExchangeDetail>::new();
    for builder in builders {
        let turns = builder
            .turns
            .into_iter()
            .filter(|turn| timestamp_allowed(&turn.timestamp, start_time_ms))
            .collect::<Vec<_>>();
        if turns.is_empty() {
            continue;
        }
        let mut tokens = zero_tokens();
        let mut tools = Vec::new();
        let mut detail_turns = Vec::with_capacity(turns.len());
        for turn in turns {
            add_tokens(&mut tokens, &turn.tokens);
            merge_unique(&mut tools, turn.tools.clone());
            detail_turns.push(SessionTurnDetail {
                timestamp: turn.timestamp,
                tokens: turn.tokens,
                tools: turn.tools,
                cost_estimate: 0.0,
            });
        }
        let first_turn_timestamp = detail_turns
            .first()
            .map(|turn| turn.timestamp.clone())
            .unwrap_or_default();
        let boundary_in_range = !builder.boundary_timestamp.is_empty()
            && timestamp_allowed(&builder.boundary_timestamp, start_time_ms);
        let started_at = if boundary_in_range {
            builder.boundary_timestamp
        } else {
            first_turn_timestamp
        };
        let ended_at = detail_turns
            .last()
            .map(|turn| turn.timestamp.clone())
            .unwrap_or_else(|| started_at.clone());
        exchanges.push(SessionExchangeDetail {
            started_at,
            ended_at,
            turn_count: detail_turns.len() as u64,
            tools,
            tokens,
            cost_estimate: 0.0,
            turns: detail_turns,
        });
    }

    let total_tokens = exchanges
        .iter()
        .map(|exchange| exchange.tokens.total)
        .sum::<u64>();
    if total_tokens > 0 && session_cost > 0.0 {
        for exchange in &mut exchanges {
            let mut exchange_cost = 0.0;
            for turn in &mut exchange.turns {
                turn.cost_estimate = session_cost * turn.tokens.total as f64 / total_tokens as f64;
                exchange_cost += turn.cost_estimate;
            }
            exchange.cost_estimate = exchange_cost;
        }
    }
    let turn_count = exchanges
        .iter()
        .map(|exchange| exchange.turn_count)
        .sum::<u64>();
    let exchange_count = exchanges.len() as u64;
    SessionDetailReport {
        found: true,
        client,
        session_id,
        exchanges,
        totals: SessionDetailTotals {
            total_tokens,
            cost_usd: if total_tokens > 0 { session_cost } else { 0.0 },
            exchange_count,
            turn_count,
        },
        source: SESSION_DETAIL_SOURCE,
    }
}

fn timestamp_allowed(timestamp: &str, start_time_ms: Option<i64>) -> bool {
    let Some(start) = start_time_ms else {
        return true;
    };
    DateTime::parse_from_rfc3339(timestamp)
        .map(|value| value.timestamp_millis() >= start)
        .unwrap_or(false)
}

fn sanitize_cost(value: f64) -> f64 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        0.0
    }
}

fn zero_tokens() -> SessionTokenBreakdown {
    SessionTokenBreakdown {
        input: 0,
        output: 0,
        cache_read: 0,
        cache_write: 0,
        reasoning: 0,
        total: 0,
    }
}

fn add_tokens(target: &mut SessionTokenBreakdown, value: &SessionTokenBreakdown) {
    target.input = target.input.saturating_add(value.input);
    target.output = target.output.saturating_add(value.output);
    target.cache_read = target.cache_read.saturating_add(value.cache_read);
    target.cache_write = target.cache_write.saturating_add(value.cache_write);
    target.reasoning = target.reasoning.saturating_add(value.reasoning);
    target.total = target.total.saturating_add(value.total);
}

fn push_unique(target: &mut Vec<String>, value: Option<String>) {
    if target.len() >= MAX_TOOLS_PER_TURN {
        return;
    }
    let Some(value) = value
        .map(|value| {
            value
                .trim()
                .chars()
                .take(MAX_TOOL_NAME_CHARS)
                .collect::<String>()
        })
        .filter(|value| !value.is_empty())
    else {
        return;
    };
    if !target.iter().any(|existing| existing == &value) {
        target.push(value);
    }
}

fn merge_unique(target: &mut Vec<String>, values: Vec<String>) {
    for value in values {
        push_unique(target, Some(value));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn assert_renderer_shape(report: &SessionDetailReport) {
        let value = serde_json::to_value(report).expect("serialize detail");
        let root = value.as_object().expect("root object");
        assert!(root.keys().all(|key| matches!(
            key.as_str(),
            "found" | "client" | "sessionId" | "exchanges" | "totals" | "source"
        )));
        for exchange in root["exchanges"].as_array().expect("exchange array") {
            let exchange = exchange.as_object().expect("exchange object");
            assert!(exchange.keys().all(|key| matches!(
                key.as_str(),
                "startedAt"
                    | "endedAt"
                    | "turnCount"
                    | "tools"
                    | "tokens"
                    | "costEstimate"
                    | "turns"
            )));
            assert_token_shape(&exchange["tokens"]);
            for turn in exchange["turns"].as_array().expect("turn array") {
                let turn = turn.as_object().expect("turn object");
                assert!(turn.keys().all(|key| matches!(
                    key.as_str(),
                    "timestamp" | "tokens" | "tools" | "costEstimate"
                )));
                assert_token_shape(&turn["tokens"]);
            }
        }
        let totals = root["totals"].as_object().expect("totals object");
        assert!(totals.keys().all(|key| matches!(
            key.as_str(),
            "totalTokens" | "costUsd" | "exchangeCount" | "turnCount"
        )));
    }

    fn assert_token_shape(value: &serde_json::Value) {
        let tokens = value.as_object().expect("tokens object");
        assert!(tokens.keys().all(|key| matches!(
            key.as_str(),
            "input" | "output" | "cacheRead" | "cacheWrite" | "reasoning" | "total"
        )));
    }

    #[test]
    fn claude_detail_keeps_usage_and_tools_without_prompt_or_response_content() {
        let transcript = [
            r#"{"type":"user","uuid":"u1","timestamp":"2026-09-05T00:00:00Z","message":{"content":"SUPER_SECRET_PROMPT"}}"#,
            r#"{"type":"assistant","uuid":"a1","timestamp":"2026-09-05T00:00:01Z","message":{"id":"m1","usage":{"input_tokens":10,"output_tokens":4,"cache_read_input_tokens":2,"cache_creation_input_tokens":1},"content":[{"type":"text","text":"SUPER_SECRET_RESPONSE"},{"type":"tool_use","name":"Bash","input":{"command":"secret"}}]}}"#,
            r#"{"type":"assistant","uuid":"a2","timestamp":"2026-09-05T00:00:02Z","message":{"id":"m1","usage":{"input_tokens":10,"output_tokens":4,"cache_read_input_tokens":2,"cache_creation_input_tokens":1},"content":[{"type":"tool_use","name":"Read","input":{"file_path":"secret"}}]}}"#,
            r#"{"type":"user","uuid":"meta","isMeta":true,"timestamp":"2026-09-05T00:00:02Z","message":{"content":"SUPER_SECRET_META"}}"#,
            r#"{"type":"user","uuid":"u2","timestamp":"2026-09-05T00:00:03Z","message":{"content":[{"type":"tool_result","content":"SUPER_SECRET_TOOL_RESULT"}]}}"#,
            r#"{"type":"assistant","uuid":"a3","timestamp":"2026-09-05T00:00:04Z","message":{"id":"m2","usage":{"input_tokens":3,"output_tokens":2,"cache_read_input_tokens":0,"cache_creation_input_tokens":0},"content":[{"type":"text","text":"SUPER_SECRET_RESPONSE_2"}]}}"#,
        ]
        .join("\n");
        let builders = parse_claude(Cursor::new(transcript));
        let report = build_report("claude".into(), "s1".into(), builders, None, 1.7);
        assert_eq!(report.exchanges.len(), 1);
        assert_eq!(report.exchanges[0].turn_count, 2);
        assert_eq!(report.exchanges[0].tokens.total, 22);
        assert_eq!(report.exchanges[0].tools, vec!["Bash", "Read"]);
        assert_renderer_shape(&report);
        let serialized = serde_json::to_string(&report).expect("serialize");
        assert!(!serialized.contains("SUPER_SECRET"));
    }

    #[test]
    fn codex_detail_uses_disjoint_cache_and_reasoning_subset_semantics() {
        let transcript = [
            r#"{"type":"event_msg","timestamp":"2026-09-05T00:00:00Z","payload":{"type":"user_message","message":"SUPER_SECRET_PROMPT"}}"#,
            r#"{"type":"response_item","timestamp":"2026-09-05T00:00:00Z","payload":{"type":"function_call","name":"shell","arguments":"SUPER_SECRET_ARGS"}}"#,
            r#"{"type":"event_msg","timestamp":"2026-09-05T00:00:01Z","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":20,"cached_input_tokens":5,"cache_write_input_tokens":0,"output_tokens":6,"reasoning_output_tokens":2}}}}"#,
        ]
        .join("\n");
        let builders = parse_codex(Cursor::new(transcript));
        let report = build_report("codex".into(), "s1".into(), builders, None, 2.6);
        let turn = &report.exchanges[0].turns[0];
        assert_eq!(turn.tokens.input, 15);
        assert_eq!(turn.tokens.cache_read, 5);
        assert_eq!(turn.tokens.output, 4);
        assert_eq!(turn.tokens.reasoning, 2);
        assert_eq!(turn.tokens.total, 26);
        assert_eq!(turn.tools, vec!["shell"]);
        assert_renderer_shape(&report);
        let serialized = serde_json::to_string(&report).expect("serialize");
        assert!(!serialized.contains("SUPER_SECRET"));
    }

    #[test]
    fn detail_filters_by_absolute_range_start_and_redistributes_period_cost() {
        let transcript = [
            r#"{"type":"event_msg","timestamp":"2026-09-04T00:00:00Z","payload":{"type":"user_message","message":"old"}}"#,
            r#"{"type":"event_msg","timestamp":"2026-09-04T00:00:01Z","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":10,"cached_input_tokens":0,"output_tokens":2,"reasoning_output_tokens":0}}}}"#,
            r#"{"type":"event_msg","timestamp":"2026-09-05T00:00:00Z","payload":{"type":"user_message","message":"new"}}"#,
            r#"{"type":"event_msg","timestamp":"2026-09-05T00:00:01Z","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":20,"cached_input_tokens":5,"output_tokens":4,"reasoning_output_tokens":1}}}}"#,
        ]
        .join("\n");
        let start = DateTime::parse_from_rfc3339("2026-09-05T00:00:00Z")
            .unwrap()
            .timestamp_millis();
        let report = build_report(
            "codex".into(),
            "s1".into(),
            parse_codex(Cursor::new(transcript)),
            Some(start),
            3.25,
        );
        assert_eq!(report.exchanges.len(), 1);
        assert_eq!(report.totals.total_tokens, 24);
        assert_eq!(report.totals.cost_usd, 3.25);
        assert!((report.exchanges[0].cost_estimate - 3.25).abs() < f64::EPSILON);
    }

    #[test]
    fn unsupported_detail_clients_are_rejected_before_file_resolution() {
        let report = read(
            Path::new("/does/not/matter"),
            "antigravity",
            "s1",
            None,
            1.0,
        );
        assert!(!report.found);
        assert!(report.exchanges.is_empty());
        let traversal = read(
            Path::new("/does/not/matter"),
            "codex",
            "rollout-2026-09-05T../../secret",
            None,
            1.0,
        );
        assert!(!traversal.found);
    }

    #[tokio::test]
    #[ignore = "requires local tokScale data and provider session transcripts"]
    async fn live_session_detail_smoke() {
        use crate::domain::{UsageGrouping, UsagePeriod};
        use crate::tokscale::TokscaleAdapter;
        use std::env;
        use std::path::PathBuf;

        let adapter = TokscaleAdapter::discover().expect("discover tokScale");
        let usage = adapter
            .usage_report(UsagePeriod::AllTime, UsageGrouping::ClientSessionModel)
            .await
            .expect("read live usage");
        let home = env::var_os("HOME").map(PathBuf::from).expect("HOME");
        for entry in usage.entries.iter().filter(|entry| entry.client == "codex") {
            let Some(session_id) = entry.session_id.as_deref() else {
                continue;
            };
            let report = read(&home, "codex", session_id, None, entry.cost);
            if report.found && !report.exchanges.is_empty() {
                assert_renderer_shape(&report);
                assert!(report.totals.total_tokens > 0);
                return;
            }
        }
        panic!("expected at least one live Codex session detail");
    }
}
