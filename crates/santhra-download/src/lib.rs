use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use santhra_models::{ModelEntry, RemoteFile, model_dir, verify_file};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug)]
pub enum Progress {
    Start {
        file: &'static str,
        bytes: u64,
    },
    Bytes {
        file: &'static str,
        done: u64,
        total: u64,
    },
    Done {
        file: &'static str,
    },
}

/// Downloads every file of `entry` into its model directory, verifying size and SHA256.
/// Already-valid files are skipped. A cancelled or failed download leaves no final file.
pub fn download_all(
    entry: &ModelEntry,
    cancel: &AtomicBool,
    on_progress: impl Fn(Progress),
) -> Result<()> {
    let dir = model_dir(entry.id)?;
    fs::create_dir_all(dir.join(".tmp"))?;
    for file in entry.files {
        let dest = dir.join(file.name);
        if verify_file(&dest, file).is_ok() {
            on_progress(Progress::Done { file: file.name });
            continue;
        }
        download_file(file, &dir, cancel, &on_progress)?;
        if cancel.load(Ordering::Relaxed) {
            bail!("cancelled");
        }
    }
    Ok(())
}

fn download_file(
    file: &RemoteFile,
    dir: &Path,
    cancel: &AtomicBool,
    on_progress: &impl Fn(Progress),
) -> Result<()> {
    if !file.url.starts_with("https://huggingface.co/") {
        bail!("refusing non-Hugging-Face URL: {}", file.url);
    }
    let tmp = dir.join(".tmp").join(format!("{}.part", file.name));
    if tmp.exists() {
        fs::remove_file(&tmp)?;
    }

    let response = ureq::get(file.url)
        .call()
        .map_err(|err| anyhow::anyhow!("download {}: {err}", file.name))?;
    let mut reader = response.into_reader();
    let mut out = File::create(&tmp).with_context(|| format!("create {}", tmp.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    let mut done = 0u64;
    let mut last_emit = Instant::now();
    let mut last_percent = 0u64;
    on_progress(Progress::Start {
        file: file.name,
        bytes: file.bytes,
    });

    loop {
        if cancel.load(Ordering::Relaxed) {
            drop(out);
            let _ = fs::remove_file(&tmp);
            bail!("cancelled");
        }
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n])?;
        hasher.update(&buf[..n]);
        done += n as u64;
        let percent = done
            .saturating_mul(100)
            .checked_div(file.bytes)
            .unwrap_or(100);
        if done == file.bytes
            || percent.saturating_sub(last_percent) >= 1
            || last_emit.elapsed() >= Duration::from_millis(100)
        {
            on_progress(Progress::Bytes {
                file: file.name,
                done,
                total: file.bytes,
            });
            last_percent = percent;
            last_emit = Instant::now();
        }
    }
    drop(out);

    let digest = format!("{:x}", hasher.finalize());
    if done != file.bytes || digest != file.sha256 {
        let _ = fs::remove_file(&tmp);
        bail!(
            "verification failed for {}: {} bytes / {digest}",
            file.name,
            done
        );
    }
    fs::rename(&tmp, dir.join(file.name))?;
    on_progress(Progress::Done { file: file.name });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "network; downloads a real model from Hugging Face"]
    fn downloads_and_verifies_a_real_model() {
        let entry = santhra_models::entry("zipformer-en-20m").unwrap();
        let cancel = AtomicBool::new(false);
        download_all(entry, &cancel, |_| {}).expect("download");
        let dir = santhra_models::model_dir(entry.id).unwrap();
        santhra_models::resolve(entry, &dir).expect("verify");
    }

    #[test]
    fn rejects_non_https_sources() {
        let cancel = AtomicBool::new(false);
        let dir = std::env::temp_dir();
        let file = RemoteFile {
            name: "bad.txt",
            url: "http://example.com/bad.txt",
            sha256: "0",
            bytes: 1,
        };
        assert!(download_file(&file, &dir, &cancel, &|_| {}).is_err());
    }
}
