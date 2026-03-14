//! Sound utility for task completion notifications.
//!
//! Plays a system sound when a task completes. Platform-aware:
//! - macOS: `afplay` with Glass sound
//! - Linux: tries `paplay`, `aplay`, `play`, `cvlc` with common sound files
//! - Other: terminal bell (`\a`)
//!
//! Includes a cooldown to prevent rapid repeated sounds.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use tracing::debug;

/// Minimum seconds between consecutive sounds.
const COOLDOWN_SECONDS: u64 = 30;

/// Monotonic timestamp of the last played sound (epoch millis approximation).
static LAST_PLAYED_MS: AtomicU64 = AtomicU64::new(0);

/// Lazy-initialized start time for monotonic clock.
static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();

fn now_ms() -> u64 {
    let start = START.get_or_init(Instant::now);
    start.elapsed().as_millis() as u64
}

/// Play a sound to indicate task completion.
///
/// Fails silently if no sound player is available.
/// Respects a 30-second cooldown between plays.
pub fn play_finish_sound() {
    let now = now_ms();
    let last = LAST_PLAYED_MS.load(Ordering::Relaxed);
    if now.saturating_sub(last) < COOLDOWN_SECONDS * 1000 {
        return;
    }
    LAST_PLAYED_MS.store(now, Ordering::Relaxed);

    if let Err(e) = play_platform_sound() {
        debug!("Failed to play finish sound: {e}");
    }
}

fn play_platform_sound() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("afplay")
            .arg("/System/Library/Sounds/Glass.aiff")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    #[cfg(target_os = "linux")]
    {
        let players = ["paplay", "aplay", "play", "cvlc"];
        let sounds = [
            "/usr/share/sounds/freedesktop/stereo/complete.oga",
            "/usr/share/sounds/gnome/default/alerts/glass.ogg",
            "/usr/share/sounds/alsa/Front_Center.wav",
        ];

        for player in &players {
            let which = std::process::Command::new("which").arg(player).output();
            if let Ok(output) = which {
                if output.status.success() {
                    for sound in &sounds {
                        if std::path::Path::new(sound).exists() {
                            let mut cmd = std::process::Command::new(player);
                            if *player == "cvlc" {
                                cmd.arg("--play-and-exit");
                            }
                            cmd.arg(sound)
                                .stdout(std::process::Stdio::null())
                                .stderr(std::process::Stdio::null())
                                .spawn()
                                .map_err(|e| e.to_string())?;
                            return Ok(());
                        }
                    }
                }
            }
        }

        // Fallback: terminal bell
        print!("\x07");
        return Ok(());
    }

    #[cfg(target_os = "windows")]
    {
        let sound_path = r"C:\Windows\Media\notify.wav";
        let ps_cmd = format!("(New-Object Media.SoundPlayer '{sound_path}').PlaySync()");
        std::process::Command::new("powershell")
            .args(["-Command", &ps_cmd])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| e.to_string())?;
        return Ok(());
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        print!("\x07");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cooldown_logic() {
        // Test that the cooldown mechanism works
        let now = now_ms();
        let later = now_ms();
        assert!(later >= now);

        // If LAST_PLAYED is set to now, subsequent calls within 30s should be blocked
        LAST_PLAYED_MS.store(now, Ordering::Relaxed);

        let new_now = now_ms();
        let last = LAST_PLAYED_MS.load(Ordering::Relaxed);
        // Within cooldown window
        assert!(new_now.saturating_sub(last) < COOLDOWN_SECONDS * 1000);
    }

    #[test]
    fn test_now_ms() {
        let t1 = now_ms();
        let t2 = now_ms();
        assert!(t2 >= t1);
    }
}
