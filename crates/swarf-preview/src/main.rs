use anyhow::{Result, ensure};
use serde::Deserialize;
use std::io::{Read, Write};
use swarf_preview::{Settings, compile, path_stl, seek};
#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
enum Operation {
    Compile {
        source_text: String,
        settings: Settings,
    },
    Seek {
        source_text: String,
        settings: Settings,
        at_ms: f64,
    },
    Mesh {
        source_text: String,
        settings: Settings,
        at_ms: f64,
        display_radius_mm: f64,
    },
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    schema: String,
    request: Operation,
}
fn run() -> Result<()> {
    ensure!(
        std::env::args().count() == 1,
        "usage: swarf-preview < request.json"
    );
    let mut bytes = vec![];
    std::io::stdin()
        .take((swarf_preview::MAX_SOURCE + 65536) as u64 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= swarf_preview::MAX_SOURCE + 65536,
        "request byte bound exceeded"
    );
    let request: Request = serde_json::from_slice(&bytes)?;
    ensure!(
        request.schema == "swarf.preview-request.v1",
        "wrong request version"
    );
    let output = match request.request {
        Operation::Compile {
            source_text,
            settings,
        } => serde_json::to_vec(&compile(&source_text, &settings)?)?,
        Operation::Seek {
            source_text,
            settings,
            at_ms,
        } => serde_json::to_vec(&seek(&compile(&source_text, &settings)?, at_ms)?)?,
        Operation::Mesh {
            source_text,
            settings,
            at_ms,
            display_radius_mm,
        } => path_stl(&compile(&source_text, &settings)?, at_ms, display_radius_mm)?,
    };
    std::io::stdout().lock().write_all(&output)?;
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("swarf-preview: {error:#}");
        std::process::exit(2);
    }
}
