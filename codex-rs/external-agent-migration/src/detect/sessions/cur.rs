use super::common::SessionFileCandidate;
use super::common::detect_recent_sessions;
use crate::model::ExternalAgentSessionImportLimits;
use crate::sessions::ExternalAgentSessionMigration;
use crate::sessions::SessionRecordFormat;
use std::fs;
use std::io;
use std::path::Path;
use std::path::PathBuf;

const MAX_CUR_PROJECT_PATH_PROBES: usize = 128;
const MAX_CUR_PROJECT_DIRECTORY_ENTRIES: usize = 4096;

pub fn detect_recent_cur_sessions(
    external_agent_home: &Path,
    codex_home: &Path,
) -> io::Result<Vec<ExternalAgentSessionMigration>> {
    detect_recent_cur_sessions_with_limits(
        external_agent_home,
        codex_home,
        ExternalAgentSessionImportLimits::default(),
    )
}

pub(crate) fn detect_recent_cur_sessions_with_limits(
    external_agent_home: &Path,
    codex_home: &Path,
    limits: ExternalAgentSessionImportLimits,
) -> io::Result<Vec<ExternalAgentSessionMigration>> {
    let projects_root = external_agent_home.join("projects");
    if !projects_root.is_dir() {
        return Ok(Vec::new());
    }

    let mut candidates = Vec::new();
    for project_entry in fs::read_dir(projects_root)? {
        let Ok(project_entry) = project_entry else {
            continue;
        };
        let project_storage = project_entry.path();
        if !project_storage.is_dir() {
            continue;
        }
        let fallback_cwd = cur_project_cwd(&project_storage, external_agent_home);
        for path in cur_transcript_files(&project_storage.join("agent-transcripts")) {
            candidates.push(SessionFileCandidate {
                path,
                fallback_cwd: fallback_cwd.clone(),
                record_format: SessionRecordFormat::Cur,
            });
        }
    }
    detect_recent_sessions(
        codex_home, candidates, /*require_existing_cwd*/ false, limits,
    )
}

fn cur_transcript_files(transcripts_root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut pending = vec![transcripts_root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let Ok(entries) = fs::read_dir(directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                if entry.file_name() != "subagents" {
                    pending.push(path);
                }
            } else if file_type.is_file()
                && path.extension().and_then(|extension| extension.to_str()) == Some("jsonl")
            {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

fn cur_project_cwd(project_storage: &Path, external_agent_home: &Path) -> Option<PathBuf> {
    let encoded = project_storage.file_name()?.to_str()?;
    // Cursor stores projectless chats under this reserved project name.
    if encoded == "empty-window" {
        let external_agent_home = if external_agent_home.is_absolute() {
            external_agent_home.to_path_buf()
        } else {
            std::env::current_dir().ok()?.join(external_agent_home)
        };
        return external_agent_home.parent().map(Path::to_path_buf);
    }
    decode_cur_project_path(encoded)
}

fn decode_cur_project_path(encoded: &str) -> Option<PathBuf> {
    #[cfg(not(windows))]
    let path = PathBuf::from("/");

    #[cfg(windows)]
    let (encoded, path) = {
        let (drive, encoded) = decode_cur_windows_project_drive(encoded)?;
        (encoded, PathBuf::from(format!("{drive}:\\")))
    };

    let encoded = encoded.strip_prefix('-').unwrap_or(encoded);
    if encoded.split('-').any(|component| {
        component.is_empty()
            || matches!(component, "." | "..")
            || component.contains(['/', '\\', ':'])
    }) {
        return None;
    }

    let mut pending = vec![(path, encoded)];
    let mut matched_path = None;
    let mut probes = 0;
    let mut entries = 0;
    while let Some((parent, remaining)) = pending.pop() {
        for entry in fs::read_dir(parent).ok()? {
            entries += 1;
            if entries > MAX_CUR_PROJECT_DIRECTORY_ENTRIES {
                return None;
            }
            let entry = entry.ok()?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            let normalized = name
                .split(['-', '_', '.', ' ', '+', '@', '&'])
                .filter(|component| !component.is_empty())
                .collect::<Vec<_>>()
                .join("-");
            let mut matched_lengths = std::collections::BTreeSet::new();
            for candidate in [name, normalized.as_str()] {
                if candidate.is_empty() {
                    continue;
                }
                let Some(trailing) = remaining.strip_prefix(candidate) else {
                    continue;
                };
                if !trailing.is_empty() && !trailing.starts_with('-') {
                    continue;
                }
                if !matched_lengths.insert(candidate.len()) {
                    continue;
                }
                probes += 1;
                if probes > MAX_CUR_PROJECT_PATH_PROBES {
                    return None;
                }
                let candidate_path = entry.path();
                if !candidate_path.is_dir() {
                    continue;
                }
                if trailing.is_empty() {
                    if matched_path
                        .as_ref()
                        .is_some_and(|matched| matched != &candidate_path)
                    {
                        return None;
                    }
                    matched_path = Some(candidate_path);
                } else {
                    pending.push((candidate_path, &trailing[1..]));
                }
            }
        }
    }
    matched_path
}

#[cfg(any(windows, test))]
fn decode_cur_windows_project_drive(encoded: &str) -> Option<(char, &str)> {
    let drive = encoded.as_bytes().first().copied()?;
    if !drive.is_ascii_alphabetic() || encoded.as_bytes().get(1) != Some(&b'-') {
        return None;
    }

    Some((char::from(drive), encoded.get(2..)?))
}

#[cfg(test)]
#[path = "cur_tests.rs"]
mod tests;
