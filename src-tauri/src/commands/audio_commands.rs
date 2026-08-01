/*
 * Name: audio_commands.rs
 * Purpose: Write an audio script to a real sound file.
 * Description: Playback uses the webview's SpeechSynthesis, which has no way to
 *   hand back what it spoke: there is no MediaStream, no buffer, nothing to
 *   record. So the script could be heard and never kept, and "download the
 *   audio" had no answer.
 *
 *   Every desktop platform already ships a speech engine that writes to a file,
 *   and it is the same engine and the same voices the webview is using for
 *   playback. Driving that directly produces a genuine audio file with no
 *   bundled model, no extra download, and no drift between what you heard and
 *   what you saved.
 *
 *   The text is passed through a temporary file rather than the command line.
 *   A script is long, arbitrary, and user-derived; putting it in an argument
 *   invites both quoting failures and shell injection, and this way neither is
 *   possible.
 * Tech Stack: Rust, Tauri v2, platform speech engines
 * License: MIT
 * Authors: Amey Thakur (https://github.com/Amey-Thakur)
 *          Archit Konde (https://github.com/Archit-Konde)
 * Date: 2026-07-28
 */

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::{AppError, AppResult};

/// The file extension this platform's engine produces.
///
/// Windows and Linux write WAV, which is uncompressed and lossless. macOS is
/// asked for AAC in an m4a, because `say` encodes it directly and a lossless
/// AIFF of a long script is needlessly large.
#[tauri::command(rename_all = "snake_case")]
pub fn audio_export_extension() -> &'static str {
    if cfg!(target_os = "macos") {
        "m4a"
    } else {
        "wav"
    }
}

/// Speak `text` into `file_path` using the platform's own engine.
///
/// Returns the path written. Errors carry what the user can do about it, since
/// the common failure on Linux is simply that no engine is installed.
#[tauri::command(rename_all = "snake_case")]
pub async fn export_audio_file(text: String, file_path: String) -> AppResult<String> {
    if text.trim().is_empty() {
        return Err(AppError::InvalidInput("There is no script to save.".into()));
    }

    tauri::async_runtime::spawn_blocking(move || {
        let out = PathBuf::from(&file_path);
        let source = write_temp_text(&text)?;
        let result = synthesize(&source, &out);
        /* Best effort: a leftover temp file is not worth failing the export. */
        std::fs::remove_file(&source).ok();
        result?;

        if !out.exists() {
            return Err(AppError::Internal(
                "The speech engine reported success but wrote no file.".into(),
            ));
        }
        Ok(file_path)
    })
    .await
    .map_err(|e| AppError::Internal(format!("Audio export failed: {e}")))?
}

/// Put the script somewhere the engine can read it, so no text ever reaches a
/// command line.
fn write_temp_text(text: &str) -> AppResult<PathBuf> {
    let path =
        std::env::temp_dir().join(format!("notebooklab-script-{}.txt", uuid::Uuid::new_v4()));
    std::fs::write(&path, text)
        .map_err(|e| AppError::Internal(format!("Could not stage the script: {e}")))?;
    Ok(path)
}

#[cfg(target_os = "windows")]
fn synthesize(source: &Path, out: &Path) -> AppResult<()> {
    /* SAPI through PowerShell.

    The paths travel in environment variables rather than in the script text.
    `-Command` does not bind trailing arguments to a `param()` block, so that
    route silently passes nothing; and interpolating the paths into the script
    would mean a path containing a quote could change what runs. An environment
    variable is neither parsed nor quoted, so both problems disappear. */
    let script = "Add-Type -AssemblyName System.Speech
        $synth = New-Object System.Speech.Synthesis.SpeechSynthesizer
        $synth.SetOutputToWaveFile($env:NOTEBOOKLAB_AUDIO_OUT)
        $synth.Speak([System.IO.File]::ReadAllText($env:NOTEBOOKLAB_AUDIO_IN))
        $synth.Dispose()";

    let mut command = Command::new("powershell");
    command
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .env("NOTEBOOKLAB_AUDIO_IN", source)
        .env("NOTEBOOKLAB_AUDIO_OUT", out);

    /* Without this the console window belonging to PowerShell appears on top of
    the app for as long as synthesis runs, which for a full script is many
    seconds of a black box the user did not ask for and cannot dismiss. */
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    run(&mut command, "Windows speech")
}

#[cfg(target_os = "macos")]
fn synthesize(source: &Path, out: &Path) -> AppResult<()> {
    run(
        Command::new("say")
            .arg("-f")
            .arg(source)
            .arg("-o")
            .arg(out)
            .args(["--data-format=aac"]),
        "the macOS speech engine",
    )
}

#[cfg(all(unix, not(target_os = "macos")))]
fn synthesize(source: &Path, out: &Path) -> AppResult<()> {
    /* espeak-ng first, espeak as the older fallback. Neither is guaranteed to
    be installed, which is why the error names the packages. */
    for engine in ["espeak-ng", "espeak"] {
        let attempt = Command::new(engine)
            .arg("-f")
            .arg(source)
            .arg("-w")
            .arg(out)
            .output();
        match attempt {
            Ok(output) if output.status.success() => return Ok(()),
            /* Present but failing is a real error worth surfacing. */
            Ok(output) => {
                return Err(AppError::Internal(format!(
                    "{engine} could not write the file: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                )))
            }
            /* Not installed: try the next one. */
            Err(_) => continue,
        }
    }
    Err(AppError::InvalidInput(
        "No speech engine is installed. Install espeak-ng to save audio, or \
         download the transcript instead."
            .into(),
    ))
}

/// Run a synthesis command and turn a non-zero exit into a readable error.
#[cfg(not(all(unix, not(target_os = "macos"))))]
fn run(command: &mut Command, engine: &str) -> AppResult<()> {
    let output = command
        .output()
        .map_err(|e| AppError::Internal(format!("Could not start {engine}: {e}")))?;
    if output.status.success() {
        return Ok(());
    }
    Err(AppError::Internal(format!(
        "{engine} could not write the file: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_matches_the_platform() {
        let ext = audio_export_extension();
        assert!(ext == "wav" || ext == "m4a");
        if cfg!(target_os = "macos") {
            assert_eq!(ext, "m4a");
        } else {
            assert_eq!(ext, "wav");
        }
    }

    #[test]
    fn the_script_is_staged_in_a_file() {
        /* The whole point of staging is that the text never becomes a command
        argument, so this checks it round-trips through the file intact. */
        let text = "A: quotes \" and 'apostrophes' and $variables and `backticks`";
        let path = write_temp_text(text).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn each_staged_file_is_distinct() {
        /* Two exports at once must not write over each other's script. */
        let a = write_temp_text("one").unwrap();
        let b = write_temp_text("two").unwrap();
        assert_ne!(a, b);
        assert_eq!(std::fs::read_to_string(&a).unwrap(), "one");
        std::fs::remove_file(a).ok();
        std::fs::remove_file(b).ok();
    }
}
