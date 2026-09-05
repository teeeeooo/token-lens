use crate::domain::{SessionMetadata, SessionMetadataRef, SessionMetadataReport};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_SESSION_REFS: usize = 5_000;
const MAX_TITLE_CHARS: usize = 240;
const MAX_PROJECT_LABEL_CHARS: usize = 120;

#[derive(Debug, Clone, Default)]
struct ProviderMetadata {
    title: Option<String>,
    project_label: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ClaudeMetadataLine {
    #[serde(rename = "type")]
    event_type: Option<String>,
    #[serde(rename = "aiTitle")]
    ai_title: Option<String>,
    cwd: Option<String>,
}
pub fn collect(
    home: &Path,
    refs: Vec<SessionMetadataRef>,
) -> Result<SessionMetadataReport, String> {
    if refs.len() > MAX_SESSION_REFS {
        return Err(format!(
            "too many session metadata references: {}",
            refs.len()
        ));
    }

    let refs = normalized_refs(refs);
    let codex_refs = refs
        .iter()
        .filter(|item| item.client == "codex")
        .cloned()
        .collect::<Vec<_>>();
    let claude_refs = refs
        .iter()
        .filter(|item| item.client == "claude")
        .cloned()
        .collect::<Vec<_>>();

    let codex = collect_codex(home, &codex_refs);
    let claude = collect_claude(home, &claude_refs);
    let mut sessions = Vec::with_capacity(refs.len());

    for item in refs {
        let metadata = match item.client.as_str() {
            "codex" => codex.get(&item.session_id),
            "claude" => claude.get(&item.session_id),
            _ => None,
        };
        if let Some(metadata) = metadata {
            sessions.push(SessionMetadata {
                client: item.client,
                session_id: item.session_id,
                session_title: metadata.title.clone(),
                project_label: metadata.project_label.clone(),
            });
        }
    }
    Ok(SessionMetadataReport {
        generated_at_ms: now_ms(),
        sessions,
        source: "provider-metadata",
    })
}

fn normalized_refs(refs: Vec<SessionMetadataRef>) -> Vec<SessionMetadataRef> {
    let mut seen = HashSet::new();
    let mut normalized = Vec::new();
    for item in refs {
        let client = item.client.trim().to_ascii_lowercase();
        let session_id = item.session_id.trim().to_owned();
        if session_id.is_empty() || !matches!(client.as_str(), "codex" | "claude") {
            continue;
        }
        let key = format!("{client}\0{session_id}");
        if seen.insert(key) {
            normalized.push(SessionMetadataRef { client, session_id });
        }
    }
    normalized
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn clean_text(value: Option<String>, max_chars: usize) -> Option<String> {
    let value = value?.trim().chars().take(max_chars).collect::<String>();
    (!value.is_empty()).then_some(value)
}
fn project_label_from_path(value: Option<String>) -> Option<String> {
    let value = value?.trim().replace('\\', "/");
    let trimmed = value.trim_end_matches('/');
    let label = trimmed
        .rsplit('/')
        .find(|part| !part.is_empty())
        .unwrap_or(trimmed);
    clean_text(Some(label.to_owned()), MAX_PROJECT_LABEL_CHARS)
}

fn codex_thread_id(session_id: &str) -> Option<String> {
    let session_id = session_id.trim();
    if uuid_shape(session_id) {
        return Some(session_id.to_owned());
    }
    if session_id.len() >= 36 {
        let tail = &session_id[session_id.len() - 36..];
        if uuid_shape(tail) {
            return Some(tail.to_owned());
        }
    }
    None
}

fn uuid_shape(value: &str) -> bool {
    if value.len() != 36 {
        return false;
    }
    value.bytes().enumerate().all(|(index, byte)| match index {
        8 | 13 | 18 | 23 => byte == b'-',
        _ => byte.is_ascii_hexdigit(),
    })
}

fn readonly_connection(path: &Path) -> Option<Connection> {
    Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .ok()
}
fn codex_state_databases(home: &Path) -> Vec<PathBuf> {
    let root = home.join(".codex");
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    let mut databases = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let number = name
                .strip_prefix("state_")?
                .strip_suffix(".sqlite")?
                .parse::<u64>()
                .ok()?;
            Some((number, entry.path()))
        })
        .collect::<Vec<_>>();
    databases.sort_by_key(|item| std::cmp::Reverse(item.0));
    databases.into_iter().map(|(_, path)| path).collect()
}

fn collect_codex(home: &Path, refs: &[SessionMetadataRef]) -> HashMap<String, ProviderMetadata> {
    if refs.is_empty() {
        return HashMap::new();
    }
    let thread_ids = refs
        .iter()
        .filter_map(|item| {
            codex_thread_id(&item.session_id).map(|id| (item.session_id.clone(), id))
        })
        .collect::<HashMap<_, _>>();
    let mut state = HashMap::new();
    for database in codex_state_databases(home) {
        query_codex_state(&database, &thread_ids, &mut state);
        if state.len() == thread_ids.len() {
            break;
        }
    }
    let mut catalog = HashMap::new();
    query_codex_catalog(
        &home.join(".codex/sqlite/codex-dev.db"),
        &thread_ids,
        &mut catalog,
    );
    let mut merged = HashMap::new();
    for item in refs {
        let mut metadata = state.get(&item.session_id).cloned().unwrap_or_default();
        if let Some(candidate) = catalog.get(&item.session_id) {
            if candidate.title.is_some() {
                metadata.title = candidate.title.clone();
            }
            if candidate.project_label.is_some() {
                metadata.project_label = candidate.project_label.clone();
            }
        }
        if metadata.title.is_some() || metadata.project_label.is_some() {
            merged.insert(item.session_id.clone(), metadata);
        }
    }
    merged
}

fn query_codex_state(
    path: &Path,
    thread_ids: &HashMap<String, String>,
    output: &mut HashMap<String, ProviderMetadata>,
) {
    let Some(connection) = readonly_connection(path) else {
        return;
    };
    let Ok(mut statement) =
        connection.prepare("SELECT title, cwd FROM threads WHERE id = ?1 LIMIT 1")
    else {
        return;
    };
    for (session_id, thread_id) in thread_ids {
        if output.contains_key(session_id) {
            continue;
        }
        let row = statement
            .query_row(params![thread_id], |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                ))
            })
            .optional()
            .ok()
            .flatten();
        if let Some((title, cwd)) = row {
            output.insert(
                session_id.clone(),
                ProviderMetadata {
                    title: clean_text(title, MAX_TITLE_CHARS),
                    project_label: project_label_from_path(cwd),
                },
            );
        }
    }
}
fn query_codex_catalog(
    path: &Path,
    thread_ids: &HashMap<String, String>,
    output: &mut HashMap<String, ProviderMetadata>,
) {
    let Some(connection) = readonly_connection(path) else {
        return;
    };
    let Ok(mut statement) = connection.prepare(
        "SELECT display_title, cwd FROM local_thread_catalog WHERE thread_id = ?1 ORDER BY observation_sequence DESC LIMIT 1",
    ) else {
        return;
    };
    for (session_id, thread_id) in thread_ids {
        let row = statement
            .query_row(params![thread_id], |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                ))
            })
            .optional()
            .ok()
            .flatten();
        if let Some((title, cwd)) = row {
            output.insert(
                session_id.clone(),
                ProviderMetadata {
                    title: clean_text(title, MAX_TITLE_CHARS),
                    project_label: project_label_from_path(cwd),
                },
            );
        }
    }
}

fn claude_config_dir(home: &Path) -> PathBuf {
    env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .filter(|value| !value.as_os_str().is_empty())
        .unwrap_or_else(|| home.join(".claude"))
}
fn collect_claude(home: &Path, refs: &[SessionMetadataRef]) -> HashMap<String, ProviderMetadata> {
    if refs.is_empty() {
        return HashMap::new();
    }
    let wanted = refs
        .iter()
        .map(|item| item.session_id.clone())
        .collect::<HashSet<_>>();
    let config = claude_config_dir(home);
    let mut files = HashMap::new();
    find_session_files(&config.join("projects"), &wanted, &mut files);
    if files.len() < wanted.len() {
        find_session_files(&config.join("transcripts"), &wanted, &mut files);
    }

    files
        .into_iter()
        .filter_map(|(session_id, path)| {
            let metadata = read_claude_metadata(&path);
            (metadata.title.is_some() || metadata.project_label.is_some())
                .then_some((session_id, metadata))
        })
        .collect()
}

fn find_session_files(
    root: &Path,
    wanted: &HashSet<String>,
    output: &mut HashMap<String, PathBuf>,
) {
    if !root.is_dir() || output.len() >= wanted.len() {
        return;
    }
    let mut stack = vec![root.to_path_buf()];
    while let Some(directory) = stack.pop() {
        let Ok(entries) = fs::read_dir(directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                stack.push(entry.path());
                continue;
            }
            if !file_type.is_file() {
                continue;
            }
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("jsonl") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|value| value.to_str()) else {
                continue;
            };
            if wanted.contains(stem) {
                output.entry(stem.to_owned()).or_insert(path);
                if output.len() >= wanted.len() {
                    return;
                }
            }
        }
    }
}

fn read_claude_metadata(path: &Path) -> ProviderMetadata {
    let Ok(file) = File::open(path) else {
        return ProviderMetadata::default();
    };
    let mut metadata = ProviderMetadata::default();
    for line in BufReader::new(file).lines().map_while(Result::ok) {
        let Ok(event) = serde_json::from_str::<ClaudeMetadataLine>(&line) else {
            continue;
        };
        if metadata.project_label.is_none() {
            metadata.project_label = project_label_from_path(event.cwd);
        }
        if event.event_type.as_deref() == Some("ai-title") && metadata.title.is_none() {
            metadata.title = clean_text(event.ai_title, MAX_TITLE_CHARS);
        }
        if metadata.title.is_some() && metadata.project_label.is_some() {
            break;
        }
    }
    metadata
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEST_DIR: AtomicU64 = AtomicU64::new(1);

    fn assert_renderer_metadata_shape(report: &SessionMetadataReport) {
        let value = serde_json::to_value(report).expect("serialize metadata report");
        let root = value.as_object().expect("metadata report object");
        assert!(root
            .keys()
            .all(|key| matches!(key.as_str(), "generatedAtMs" | "sessions" | "source")));
        for session in root["sessions"].as_array().expect("session metadata array") {
            let fields = session.as_object().expect("session metadata object");
            assert!(
                fields.keys().all(|key| matches!(
                    key.as_str(),
                    "client" | "sessionId" | "sessionTitle" | "projectLabel"
                )),
                "unexpected renderer session metadata field: {fields:?}"
            );
        }
    }
    fn test_home(label: &str) -> PathBuf {
        let id = NEXT_TEST_DIR.fetch_add(1, Ordering::Relaxed);
        let path = env::temp_dir().join(format!(
            "token-lens-session-metadata-{label}-{}-{id}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("create test home");
        path
    }

    #[test]
    fn codex_metadata_uses_provider_title_without_exposing_transcript_fields() {
        let home = test_home("codex");
        let codex_dir = home.join(".codex");
        fs::create_dir_all(codex_dir.join("sqlite")).expect("create codex dirs");
        let thread_id = "01234567-89ab-cdef-0123-456789abcdef";
        let session_id = format!("rollout-2026-09-05T12-00-00-{thread_id}");

        let state = Connection::open(codex_dir.join("state_9.sqlite")).expect("open state db");
        state.execute(
            "CREATE TABLE threads (id TEXT PRIMARY KEY, title TEXT, cwd TEXT, first_user_message TEXT, preview TEXT)",
            [],
        ).expect("create threads");
        state
            .execute(
                "INSERT INTO threads VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    thread_id,
                    "State title",
                    "/Users/test/workspace",
                    "SECRET PROMPT",
                    "SECRET PREVIEW"
                ],
            )
            .expect("insert state row");
        let catalog =
            Connection::open(codex_dir.join("sqlite/codex-dev.db")).expect("open catalog db");
        catalog.execute(
            "CREATE TABLE local_thread_catalog (thread_id TEXT, display_title TEXT, cwd TEXT, observation_sequence INTEGER)",
            [],
        ).expect("create catalog");
        catalog
            .execute(
                "INSERT INTO local_thread_catalog VALUES (?1, ?2, ?3, 2)",
                params![thread_id, "Catalog title", "/Users/test/workspace"],
            )
            .expect("insert catalog row");

        let report = collect(
            &home,
            vec![SessionMetadataRef {
                client: "codex".into(),
                session_id: session_id.clone(),
            }],
        )
        .expect("collect metadata");
        assert_eq!(report.sessions.len(), 1);
        assert_eq!(
            report.sessions[0].session_title.as_deref(),
            Some("Catalog title")
        );
        assert_eq!(
            report.sessions[0].project_label.as_deref(),
            Some("workspace")
        );

        assert_renderer_metadata_shape(&report);
        assert_ne!(
            report.sessions[0].session_title.as_deref(),
            Some("SECRET PROMPT")
        );
        assert_ne!(
            report.sessions[0].session_title.as_deref(),
            Some("SECRET PREVIEW")
        );
        drop(catalog);
        drop(state);
        fs::remove_dir_all(home).expect("remove test home");
    }

    #[test]
    fn claude_parser_reads_ai_title_and_ignores_prompt_content() {
        let home = test_home("claude");
        let file_path = home.join("session.jsonl");
        let mut file = File::create(&file_path).expect("create claude fixture");
        writeln!(file, r#"{{"type":"user","cwd":"C:\\work\\predictor","lastPrompt":"SECRET PROMPT","message":{{"content":"SECRET RESPONSE"}}}}"#).unwrap();
        writeln!(file, r#"{{"type":"ai-title","aiTitle":"Provider title"}}"#).unwrap();
        let metadata = read_claude_metadata(&file_path);
        assert_eq!(metadata.title.as_deref(), Some("Provider title"));
        assert_eq!(metadata.project_label.as_deref(), Some("predictor"));
        drop(file);
        fs::remove_dir_all(home).expect("remove claude fixture");
    }

    #[test]
    fn metadata_reference_normalization_keeps_only_supported_detail_clients() {
        let refs = normalized_refs(vec![
            SessionMetadataRef {
                client: " CODEX ".into(),
                session_id: " s1 ".into(),
            },
            SessionMetadataRef {
                client: "codex".into(),
                session_id: "s1".into(),
            },
            SessionMetadataRef {
                client: "claude".into(),
                session_id: "s2".into(),
            },
            SessionMetadataRef {
                client: "antigravity".into(),
                session_id: "s3".into(),
            },
            SessionMetadataRef {
                client: "future".into(),
                session_id: "s4".into(),
            },
        ]);
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].client, "codex");
        assert_eq!(refs[0].session_id, "s1");
        assert_eq!(refs[1].client, "claude");
    }

    #[tokio::test]
    #[ignore = "requires local tokScale data and provider session metadata"]
    async fn live_session_metadata_smoke() {
        use crate::domain::{UsageGrouping, UsagePeriod};
        use crate::tokscale::TokscaleAdapter;

        let adapter = TokscaleAdapter::discover().expect("discover tokScale");
        let usage = adapter
            .usage_report(UsagePeriod::AllTime, UsageGrouping::ClientSessionModel)
            .await
            .expect("read live usage");
        let refs = usage
            .entries
            .iter()
            .filter_map(|entry| {
                entry
                    .session_id
                    .as_ref()
                    .map(|session_id| SessionMetadataRef {
                        client: entry.client.clone(),
                        session_id: session_id.clone(),
                    })
            })
            .collect::<Vec<_>>();
        let home = env::var_os("HOME").map(PathBuf::from).expect("HOME");
        let report = collect(&home, refs).expect("collect live metadata");
        assert_renderer_metadata_shape(&report);
        assert!(
            !report.sessions.is_empty(),
            "expected at least one local session metadata match"
        );
    }

    #[test]
    fn codex_rollout_ids_resolve_to_thread_uuid_tail() {
        let thread = "01234567-89ab-cdef-0123-456789abcdef";
        assert_eq!(
            codex_thread_id(&format!("rollout-2026-09-05T12-00-00-{thread}")),
            Some(thread.to_owned())
        );
        assert_eq!(codex_thread_id(thread), Some(thread.to_owned()));
        assert_eq!(codex_thread_id("not-a-session"), None);
    }
}
