//! Optional executable integration; no formulas or reference data are linked by default.
use anyhow::{Context, Result, ensure};
use black_book_protocol::{MAX_BYTES, Request, Response, SCHEMA};
use std::{
    io::{Read, Write},
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

pub(super) enum Provider {
    External {
        path: PathBuf,
        sha256: String,
    },
    #[cfg(feature = "in-process-provider")]
    Linked(Box<dyn black_book_protocol::Evaluator>),
}

struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
impl Provider {
    pub fn configured() -> Result<Self> {
        if let Some(path) = std::env::var_os("SWARF_BLACK_BOOK_BIN") {
            return Self::external(PathBuf::from(path));
        }
        anyhow::bail!(
            "physics requires SWARF_BLACK_BOOK_BIN or an explicitly supplied in-process evaluator"
        )
    }
    pub fn external(path: PathBuf) -> Result<Self> {
        use sha2::{Digest, Sha256};
        ensure!(
            path.is_absolute() && path.is_file(),
            "provider executable must be an existing absolute path"
        );
        let file = std::fs::File::open(&path)?;
        ensure!(
            file.metadata()?.len() <= 100_000_000,
            "provider executable byte bound"
        );
        let mut digest = Sha256::new();
        let mut reader = std::io::BufReader::new(file);
        let mut buffer = [0; 8192];
        loop {
            let n = reader.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            digest.update(&buffer[..n]);
        }
        Ok(Self::External {
            path,
            sha256: format!("{:x}", digest.finalize()),
        })
    }
    pub fn identity(&self) -> String {
        match self {
            Self::External { sha256, .. } => format!("black-book-evaluate:sha256:{sha256}"),
            #[cfg(feature = "in-process-provider")]
            Self::Linked(e) => e.identity(),
        }
    }
    pub fn evaluate(&self, request: &Request) -> Result<Response> {
        let response = match self {
            #[cfg(feature = "in-process-provider")]
            Self::Linked(e) => e.evaluate(request).map_err(anyhow::Error::msg)?,
            Self::External { path, .. } => {
                let bytes = serde_json::to_vec(request)?;
                ensure!(bytes.len() <= MAX_BYTES, "provider input bound");
                let mut child = Process(
                    Command::new(path)
                        .stdin(Stdio::piped())
                        .stdout(Stdio::piped())
                        .stderr(Stdio::null())
                        .spawn()
                        .context("start Black Book provider")?,
                );
                let mut output = child.0.stdout.take().context("provider stdout")?;
                let reader = std::thread::spawn(move || {
                    let mut bytes = Vec::new();
                    output
                        .by_ref()
                        .take((MAX_BYTES + 1) as u64)
                        .read_to_end(&mut bytes)
                        .map(|_| bytes)
                });
                child
                    .0
                    .stdin
                    .take()
                    .context("provider stdin")?
                    .write_all(&bytes)?;
                let deadline = Instant::now() + Duration::from_secs(5);
                let status = loop {
                    if let Some(status) = child.0.try_wait()? {
                        break status;
                    }
                    ensure!(
                        Instant::now() < deadline,
                        "Black Book provider exceeded 5s deadline"
                    );
                    std::thread::sleep(Duration::from_millis(5));
                };
                ensure!(status.success(), "Black Book provider exited with {status}");
                // A provider must not leave descendants holding its output pipe open.
                while !reader.is_finished() {
                    ensure!(
                        Instant::now() < deadline,
                        "Black Book provider output deadline exceeded"
                    );
                    std::thread::sleep(Duration::from_millis(5));
                }
                let bytes = reader
                    .join()
                    .map_err(|_| anyhow::anyhow!("provider reader panicked"))??;
                ensure!(bytes.len() <= MAX_BYTES, "provider output bound");
                serde_json::from_slice::<Response>(&bytes).context("invalid Black Book response")?
            }
        };
        ensure!(
            response.schema == SCHEMA
                && !response.provider_version.is_empty()
                && response.provider_version.len() <= 64,
            "provider schema/version mismatch"
        );
        for range in [
            response.cutting_power_w,
            response.mean_tangential_force_n,
            response.spindle_torque_nm,
        ] {
            ensure!(
                range.iter().all(|v| v.is_finite() && *v >= 0.) && range[0] <= range[1],
                "invalid provider load range"
            );
        }
        ensure!(
            response.bulk_temperature_c.is_finite(),
            "invalid provider temperature"
        );
        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request() -> Request {
        Request {
            schema: SCHEMA.into(),
            mrr_mm3_min: 60.,
            specific_cutting_force_n_mm2: [1000.; 2],
            spindle_rpm: 1000.,
            tool_diameter_mm: 2.,
            thermal_capacity_j_k: 1.,
            conductance_w_k: 0.,
            initial_c: 20.,
            ambient_c: 20.,
            heat_power_w: 0.,
            interval_s: 1.,
        }
    }
    #[test]
    fn missing_executable_and_invalid_provider_output_fail() {
        assert!(Provider::external("relative/path".into()).is_err());
        assert!(Provider::external("/does/not/exist/black-book-evaluate".into()).is_err());
        let provider = Provider::external("/usr/bin/true".into()).unwrap();
        assert!(provider.evaluate(&request()).is_err());
        let provider = Provider::external("/bin/cat".into()).unwrap();
        assert!(provider.evaluate(&request()).is_err());
        let provider = Provider::external("/usr/bin/yes".into()).unwrap();
        assert!(provider.evaluate(&request()).is_err());
    }
    #[test]
    fn stalled_provider_is_killed_at_deadline() {
        let dir = std::env::temp_dir().join(format!(
            "swarf-provider-deadline-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&dir).unwrap();
        struct Temp(std::path::PathBuf);
        impl Drop for Temp {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _temp = Temp(dir.clone());
        let binary = dir.join("stalled-provider");
        let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/stalled_provider.rs");
        assert!(
            Command::new("rustc")
                .arg(source)
                .arg("-o")
                .arg(&binary)
                .status()
                .unwrap()
                .success()
        );
        let provider = Provider::external(binary).unwrap();
        let start = Instant::now();
        let error = provider.evaluate(&request()).unwrap_err();
        assert!(error.to_string().contains("5s deadline"));
        assert!(start.elapsed() < Duration::from_secs(10));
    }
}
