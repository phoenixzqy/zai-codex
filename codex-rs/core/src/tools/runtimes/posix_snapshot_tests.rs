use super::*;
use anyhow::Result;
use pretty_assertions::assert_eq;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

#[test]
fn posix_snapshot_restoration_never_executes_a_path_builtin() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let snapshot = AbsolutePathBuf::from_absolute_path(directory.path().join("snapshot.sh"))?;
    let startup = directory.path().join("startup.sh");
    let marker = directory.path().join("external-builtin-invoked");
    let external_builtin = directory.path().join("builtin");
    std::fs::write(
        &external_builtin,
        format!(
            "#!/bin/sh\nprintf invoked > '{}'\nexit 1\n",
            shell_single_quote(&marker.to_string_lossy())
        ),
    )?;
    std::fs::set_permissions(
        &external_builtin,
        std::fs::Permissions::from_mode(/*mode*/ 0o755),
    )?;
    std::fs::write(&startup, "export GH_TOKEN=real-test-credential\n")?;
    std::fs::write(
        &snapshot,
        format!(
            "export GH_TOKEN=real-test-credential\nexport ENV='{}'\n",
            shell_single_quote(&startup.to_string_lossy())
        ),
    )?;
    let dummy = "ghp_dummy_test_credential";
    let env = HashMap::from([
        ("PATH".to_string(), directory.path().display().to_string()),
        ("GH_TOKEN".to_string(), dummy.to_string()),
        (
            CREDENTIAL_BROKER_ACTIVE_ENV_KEY.to_string(),
            "1".to_string(),
        ),
        (
            format!("{SNAPSHOT_BROKERED_VALUE_ENV_PREFIX}GH_TOKEN"),
            dummy.to_string(),
        ),
        (
            SNAPSHOT_ORIGINAL_POSIX_ENV_ENV_KEY.to_string(),
            startup.display().to_string(),
        ),
    ]);
    let shell = Shell {
        shell_type: ShellType::Sh,
        shell_path: "/bin/sh".into(),
    };
    let command = maybe_wrap_shell_lc_with_snapshot(
        &shell.derive_exec_args(
            "printf '%s|%s' \"$GH_TOKEN\" \"${ENV-unset}\"",
            /*use_login_shell*/ true,
        ),
        &shell,
        Some(&snapshot),
        &HashMap::new(),
        &env,
        &RuntimePathPrepends::default(),
    );
    let output = Command::new(&command[0])
        .args(&command[1..])
        .env_clear()
        .envs(env)
        .output()?;
    assert!(output.status.success(), "{output:?}");
    assert_eq!(String::from_utf8(output.stdout)?, format!("{dummy}|unset"));
    assert!(
        !marker.exists(),
        "POSIX restoration executed an external builtin command"
    );
    Ok(())
}
