//! F-62: the audio recordings of an interview folder, and the unit start
//! times read as seconds. No transcription: audio is only linked to the
//! transcript already parsed.

use std::path::Path;

use anyhow::Result;
use serde::Serialize;
use walkdir::WalkDir;

/// Audio formats the player accepts, with their media type.
pub const AUDIO_TYPES: [(&str, &str); 3] = [
    ("m4a", "audio/mp4"),
    ("mp3", "audio/mpeg"),
    ("wav", "audio/wav"),
];

/// A recording found in an interview folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AudioFile {
    /// Path relative to the interview folder, with `/` separators.
    pub rel_path: String,
    pub size: u64,
    /// Media type for the `Content-Type` header.
    pub mime: String,
}

/// Media type of an audio file name, `None` when it is not an accepted format.
pub fn audio_mime(name: &str) -> Option<&'static str> {
    let ext = name.rsplit_once('.')?.1.to_lowercase();
    AUDIO_TYPES
        .iter()
        .find(|(e, _)| *e == ext)
        .map(|(_, m)| *m)
}

/// The m4a, mp3 and wav files of an interview folder, sub-folders included,
/// in file name order. Hidden files and links are left out: a link could lead
/// out of the library.
pub fn find_audio(folder: &Path) -> Result<Vec<AudioFile>> {
    let mut out = Vec::new();
    if !folder.is_dir() {
        return Ok(out);
    }
    for entry in WalkDir::new(folder)
        .min_depth(1)
        .sort_by_file_name()
        .follow_links(false)
    {
        let Ok(entry) = entry else { continue };
        if !entry.file_type().is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let Some(mime) = audio_mime(&name) else {
            continue;
        };
        let Ok(rel) = entry.path().strip_prefix(folder) else {
            continue;
        };
        let rel_path = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");
        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
        out.push(AudioFile {
            rel_path,
            size,
            mime: mime.to_string(),
        });
    }
    Ok(out)
}

/// Seconds of a unit timestamp: `h:mm:ss`, `mm:ss` or `m:ss`, with an optional
/// decimal part after a dot or a comma on the seconds (`00:01:53.250`,
/// `00:01:53,250`). `None` for a stamp that is not a time (`--:--`, `P03`).
pub fn timestamp_seconds(stamp: &str) -> Option<f64> {
    let stamp = stamp.trim();
    let parts: Vec<&str> = stamp.split(':').collect();
    if !(2..=3).contains(&parts.len()) {
        return None;
    }
    let whole = |s: &str| -> Option<u64> {
        (!s.is_empty() && s.len() <= 4 && s.bytes().all(|b| b.is_ascii_digit()))
            .then(|| s.parse().ok())
            .flatten()
    };
    let (secs_part, rest) = parts.split_last()?;
    let (sec_whole, frac) = match secs_part.split_once(['.', ',']) {
        Some((w, f)) => (w, Some(f)),
        None => (*secs_part, None),
    };
    let sec = whole(sec_whole)?;
    if sec >= 60 {
        return None;
    }
    let frac = match frac {
        Some(f) if !f.is_empty() && f.bytes().all(|b| b.is_ascii_digit()) => {
            format!("0.{f}").parse::<f64>().ok()?
        }
        Some(_) => return None,
        None => 0.0,
    };
    let mut total = 0f64;
    // Hours (when three parts) then minutes.
    let mut mult = 60f64;
    for p in rest.iter().rev() {
        let v = whole(p)?;
        total += v as f64 * mult;
        mult *= 60.0;
    }
    // `mm:ss` has minutes below 60 only when an hour part exists; a bare
    // `mm:ss` may run past 59 minutes in a long recording, so it is accepted.
    if rest.len() == 2 && whole(rest[1])? >= 60 {
        return None;
    }
    Some(total + sec as f64 + frac)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamps_become_seconds() {
        assert_eq!(timestamp_seconds("00:01:53"), Some(113.0));
        assert_eq!(timestamp_seconds("01:00:05"), Some(3605.0));
        assert_eq!(timestamp_seconds("02:05"), Some(125.0));
        assert_eq!(timestamp_seconds("0:07"), Some(7.0));
        assert_eq!(timestamp_seconds("00:00:11.500"), Some(11.5));
        assert_eq!(timestamp_seconds("00:00:11,250"), Some(11.25));
        assert_eq!(timestamp_seconds(" 00:23 "), Some(23.0));
        // A bare mm:ss may pass one hour in a long recording.
        assert_eq!(timestamp_seconds("75:30"), Some(4530.0));
    }

    #[test]
    fn stamps_that_are_not_times_give_nothing() {
        for bad in ["--:--", "P03", "", "12", "aa:bb", "00:61", "01:75:00", "1:2:3:4", "00:00:11."] {
            assert_eq!(timestamp_seconds(bad), None, "{bad}");
        }
    }

    #[test]
    fn audio_files_are_found_by_extension_and_hidden_ones_skipped() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("b.M4A"), b"12345").unwrap();
        std::fs::write(dir.path().join("a.mp3"), b"1").unwrap();
        std::fs::write(dir.path().join(".hidden.wav"), b"1").unwrap();
        std::fs::write(dir.path().join("notes.txt"), b"x").unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("sub/c.wav"), b"123").unwrap();
        let found = find_audio(dir.path()).unwrap();
        let names: Vec<_> = found.iter().map(|f| f.rel_path.as_str()).collect();
        assert_eq!(names, ["a.mp3", "b.M4A", "sub/c.wav"]);
        assert_eq!(found[1].size, 5);
        assert_eq!(found[0].mime, "audio/mpeg");
        assert_eq!(found[1].mime, "audio/mp4");
        assert_eq!(found[2].mime, "audio/wav");
    }

    #[cfg(unix)]
    #[test]
    fn a_link_is_not_followed() {
        let dir = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        std::fs::write(out.path().join("secret.mp3"), b"x").unwrap();
        std::os::unix::fs::symlink(out.path().join("secret.mp3"), dir.path().join("link.mp3"))
            .unwrap();
        assert!(find_audio(dir.path()).unwrap().is_empty());
    }
}
