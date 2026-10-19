//! Model catalog and verified, resumable downloads (ADR 0006).

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("download failed: {0}")]
    Http(#[from] ureq::Error),
    #[error("{file} does not match its expected checksum")]
    Checksum { file: String },
    #[error("download cancelled")]
    Cancelled,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ModelFile {
    pub name: &'static str,
    pub url: &'static str,
    pub size: u64,
    pub sha256: &'static str,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Model {
    /// Directory name in the model store.
    pub id: &'static str,
    pub name: &'static str,
    /// SPDX identifier, shown to the user before downloading.
    pub license: &'static str,
    pub files: &'static [ModelFile],
}

impl Model {
    pub fn size(&self) -> u64 {
        self.files.iter().map(|file| file.size).sum()
    }
}

pub static SILERO_VAD: Model = Model {
    id: "silero-vad",
    name: "Silero VAD",
    license: "MIT",
    files: &[ModelFile {
        name: "silero_vad.onnx",
        url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/silero_vad.onnx",
        size: 643_854,
        sha256: "9e2449e1087496d8d4caba907f23e0bd3f78d91fa552479bb9c23ac09cbb1fd6",
    }],
};

/// Pinned to a commit so the files behind the URLs cannot change.
macro_rules! parakeet_url {
    ($file:literal) => {
        concat!(
            "https://huggingface.co/csukuangfj/sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8/resolve/",
            "2bda32ec70b097a55adaa07d9a7173915b43cc78/",
            $file
        )
    };
}

pub static PARAKEET_TDT_V3: Model = Model {
    id: "parakeet-tdt-0.6b-v3-int8",
    name: "NVIDIA Parakeet TDT 0.6B v3",
    license: "CC-BY-4.0",
    files: &[
        ModelFile {
            name: "encoder.int8.onnx",
            url: parakeet_url!("encoder.int8.onnx"),
            size: 652_184_281,
            sha256: "acfc2b4456377e15d04f0243af540b7fe7c992f8d898d751cf134c3a55fd2247",
        },
        ModelFile {
            name: "decoder.int8.onnx",
            url: parakeet_url!("decoder.int8.onnx"),
            size: 11_845_275,
            sha256: "179e50c43d1a9de79c8a24149a2f9bac6eb5981823f2a2ed88d655b24248db4e",
        },
        ModelFile {
            name: "joiner.int8.onnx",
            url: parakeet_url!("joiner.int8.onnx"),
            size: 6_355_277,
            sha256: "3164c13fc2821009440d20fcb5fdc78bff28b4db2f8d0f0b329101719c0948b3",
        },
        ModelFile {
            name: "tokens.txt",
            url: parakeet_url!("tokens.txt"),
            size: 93_939,
            sha256: "d58544679ea4bc6ac563d1f545eb7d474bd6cfa467f0a6e2c1dc1c7d37e3c35d",
        },
    ],
};

/// Models a transcription needs.
pub static TRANSCRIBE_MODELS: [&Model; 2] = [&SILERO_VAD, &PARAKEET_TDT_V3];

/// Directory holding one subdirectory per installed model.
#[derive(Debug, Clone)]
pub struct ModelStore {
    root: PathBuf,
}

impl ModelStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn path(&self, model: &Model, file: &str) -> PathBuf {
        self.root.join(model.id).join(file)
    }

    /// Checks presence and size only. Checksums are verified once, when a file
    /// is downloaded, because hashing hundreds of megabytes on every check is
    /// too slow.
    pub fn is_installed(&self, model: &Model) -> bool {
        model.files.iter().all(|file| self.has_file(model, file))
    }

    pub fn missing<'a>(&self, models: &[&'a Model]) -> Vec<&'a Model> {
        models
            .iter()
            .copied()
            .filter(|model| !self.is_installed(model))
            .collect()
    }

    /// Downloads every missing file of `models`, resuming partial downloads.
    /// `progress` receives the bytes done and the total bytes to download;
    /// returning `ControlFlow::Break` stops with [`Error::Cancelled`] and
    /// keeps the partial file for the next attempt.
    pub fn install(
        &self,
        models: &[&Model],
        mut progress: impl FnMut(u64, u64) -> ControlFlow<()>,
    ) -> Result<(), Error> {
        let pending: Vec<_> = models
            .iter()
            .flat_map(|model| model.files.iter().map(move |file| (*model, file)))
            .filter(|(model, file)| !self.has_file(model, file))
            .collect();
        let total = pending.iter().map(|(_, file)| file.size).sum();

        let agent = agent();
        let mut completed = 0;
        for (model, file) in pending {
            download(&agent, file, &self.path(model, file.name), &mut |done| {
                progress(completed + done, total)
            })?;
            completed += file.size;
        }
        Ok(())
    }

    fn has_file(&self, model: &Model, file: &ModelFile) -> bool {
        fs::metadata(self.path(model, file.name)).is_ok_and(|meta| meta.len() == file.size)
    }
}

/// Uses the system certificate store, so downloads work behind TLS-inspecting
/// corporate proxies whose CA is installed on the machine.
fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .tls_config(
            ureq::tls::TlsConfig::builder()
                .root_certs(ureq::tls::RootCerts::PlatformVerifier)
                .build(),
        )
        .build()
        .into()
}

/// Downloads `file` to `dest` through a `.part` file, resuming it when one
/// exists. `on_progress` receives the bytes of this file on disk so far.
fn download(
    agent: &ureq::Agent,
    file: &ModelFile,
    dest: &Path,
    on_progress: &mut dyn FnMut(u64) -> ControlFlow<()>,
) -> Result<(), Error> {
    if let Some(dir) = dest.parent() {
        fs::create_dir_all(dir)?;
    }
    let part = dest.with_file_name(format!("{}.part", file.name));

    let mut offset = fs::metadata(&part).map(|meta| meta.len()).unwrap_or(0);
    if offset > file.size {
        fs::remove_file(&part)?;
        offset = 0;
    }

    let mut hasher = Sha256::new();
    if offset < file.size {
        let mut request = agent.get(file.url);
        if offset > 0 {
            request = request.header("Range", format!("bytes={offset}-"));
        }
        let response = request.call()?;

        // A server that ignores the range sends the whole file again.
        let resumed = offset > 0 && response.status() == 206;
        let mut out = if resumed {
            hash_file(&part, &mut hasher)?;
            OpenOptions::new().append(true).open(&part)?
        } else {
            offset = 0;
            File::create(&part)?
        };
        if on_progress(offset).is_break() {
            return Err(Error::Cancelled);
        }

        let mut body = response.into_body().into_reader();
        let mut buf = vec![0; 64 * 1024];
        loop {
            let n = body.read(&mut buf)?;
            if n == 0 {
                break;
            }
            out.write_all(&buf[..n])?;
            hasher.update(&buf[..n]);
            offset += n as u64;
            if on_progress(offset).is_break() {
                out.sync_all()?;
                return Err(Error::Cancelled);
            }
        }
        out.sync_all()?;
    } else {
        hash_file(&part, &mut hasher)?;
    }

    if offset != file.size || hex(&hasher.finalize()) != file.sha256 {
        fs::remove_file(&part)?;
        return Err(Error::Checksum {
            file: file.name.to_owned(),
        });
    }
    fs::rename(&part, dest)?;
    Ok(())
}

fn hash_file(path: &Path, hasher: &mut Sha256) -> io::Result<()> {
    let mut file = File::open(path)?;
    let mut buf = vec![0; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            return Ok(());
        }
        hasher.update(&buf[..n]);
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    /// Serves `body` over HTTP, honouring `Range: bytes=N-`, and records the
    /// range of each request.
    fn serve(body: Vec<u8>) -> (String, Arc<Mutex<Vec<Option<u64>>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/file", listener.local_addr().unwrap());
        let ranges = Arc::new(Mutex::new(Vec::new()));
        let seen = ranges.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let mut stream = stream.unwrap();
                let mut range = None;
                for line in BufReader::new(&stream).lines() {
                    let line = line.unwrap();
                    if line.is_empty() {
                        break;
                    }
                    if let Some((name, value)) = line.split_once(':')
                        && name.eq_ignore_ascii_case("range")
                    {
                        range = value
                            .trim()
                            .trim_start_matches("bytes=")
                            .trim_end_matches('-')
                            .parse()
                            .ok();
                    }
                }
                seen.lock().unwrap().push(range);
                let (status, chunk) = match range {
                    Some(start) => ("206 Partial Content", &body[start as usize..]),
                    None => ("200 OK", &body[..]),
                };
                let head = format!(
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    chunk.len()
                );
                stream.write_all(head.as_bytes()).unwrap();
                stream.write_all(chunk).unwrap();
            }
        });
        (url, ranges)
    }

    fn model_for(url: String, body: &[u8], sha256: Option<String>) -> &'static Model {
        let sha256 = sha256.unwrap_or_else(|| hex(&Sha256::digest(body)));
        let files = vec![ModelFile {
            name: "model.onnx",
            url: url.leak(),
            size: body.len() as u64,
            sha256: sha256.leak(),
        }];
        Box::leak(Box::new(Model {
            id: "test-model",
            name: "Test",
            license: "MIT",
            files: files.leak(),
        }))
    }

    fn body() -> Vec<u8> {
        (0..200_000u32).map(|i| (i % 251) as u8).collect()
    }

    #[test]
    fn installs_and_verifies_model() {
        let body = body();
        let (url, _) = serve(body.clone());
        let model = model_for(url, &body, None);
        let root = tempfile::tempdir().unwrap();
        let store = ModelStore::new(root.path());
        let mut last = (0, 0);

        store
            .install(&[model], |done, total| {
                last = (done, total);
                ControlFlow::Continue(())
            })
            .unwrap();

        assert!(store.is_installed(model));
        assert_eq!(fs::read(store.path(model, "model.onnx")).unwrap(), body);
        assert_eq!(last, (body.len() as u64, body.len() as u64));
    }

    #[test]
    fn resumes_partial_download() {
        let body = body();
        let (url, ranges) = serve(body.clone());
        let model = model_for(url, &body, None);
        let root = tempfile::tempdir().unwrap();
        let store = ModelStore::new(root.path());
        let part = root.path().join("test-model/model.onnx.part");
        fs::create_dir_all(part.parent().unwrap()).unwrap();
        fs::write(&part, &body[..50_000]).unwrap();

        store
            .install(&[model], |_, _| ControlFlow::Continue(()))
            .unwrap();

        assert_eq!(*ranges.lock().unwrap(), [Some(50_000)]);
        assert_eq!(fs::read(store.path(model, "model.onnx")).unwrap(), body);
    }

    #[test]
    fn cancelling_keeps_partial_download() {
        let body = body();
        let (url, _) = serve(body.clone());
        let model = model_for(url, &body, None);
        let root = tempfile::tempdir().unwrap();
        let store = ModelStore::new(root.path());

        let result = store.install(&[model], |done, _| {
            if done > 0 {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        });

        assert!(matches!(result, Err(Error::Cancelled)));
        assert!(!store.is_installed(model));
        assert!(root.path().join("test-model/model.onnx.part").exists());
    }

    #[test]
    fn rejects_checksum_mismatch() {
        let body = body();
        let (url, _) = serve(body.clone());
        let model = model_for(url, &body, Some("00".repeat(32)));
        let root = tempfile::tempdir().unwrap();
        let store = ModelStore::new(root.path());

        let result = store.install(&[model], |_, _| ControlFlow::Continue(()));

        assert!(matches!(result, Err(Error::Checksum { .. })));
        assert!(!store.is_installed(model));
        assert!(!root.path().join("test-model/model.onnx.part").exists());
    }
}
