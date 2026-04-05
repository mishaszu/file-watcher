use std::{cmp::max, io::Read, path::PathBuf, sync::Arc};

use tokio::sync::{Semaphore, mpsc};

use crate::{Error, Result};

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Clone)]
pub struct HashCandidateInfo {
    pub job_id: u64,
    pub path: PathBuf,
}

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Clone)]
pub struct HashedInfo {
    pub job_id: u64,
    pub path: PathBuf,
    pub new_hash: String,
}

pub struct HasherIncomingMsg(pub HashCandidateInfo);
pub struct HasherReadyMsg(pub HashedInfo);

pub async fn hash_worker(
    mut files_to_hash_rx: mpsc::Receiver<HasherIncomingMsg>,
    complete_hash_tx: mpsc::Sender<HasherReadyMsg>,
) -> Result<()> {
    let max_workers = max(1, num_cpus::get().saturating_sub(2));

    let semaphore = Arc::new(Semaphore::new(max_workers));

    while let Some(HasherIncomingMsg(info)) = files_to_hash_rx.recv().await {
        spawn_hash_job(info, semaphore.clone(), complete_hash_tx.clone()).await?;
    }

    Ok(())
}

async fn spawn_hash_job(
    HashCandidateInfo { job_id, path }: HashCandidateInfo,
    semaphore: Arc<Semaphore>,
    complete_tx: mpsc::Sender<HasherReadyMsg>,
) -> Result<()> {
    let permit = semaphore
        .acquire_owned()
        .await
        .map_err(|_| Error::SemaphoreClosed)?;

    tokio::spawn(async move {
        let send_path = path.clone();

        let result = tokio::task::spawn_blocking(move || -> std::io::Result<String> {
            let mut hasher = blake3::Hasher::new();
            let mut file = std::fs::File::open(&send_path)?;

            let mut buf = [0u8; 8192];

            loop {
                let n = file.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                hasher.update(&buf[..n]);
            }

            Ok(hasher.finalize().to_hex().to_string())
        })
        .await;

        match result {
            Ok(Ok(new_hash)) => {
                if let Err(err) = complete_tx
                    .send(HasherReadyMsg(HashedInfo {
                        job_id,
                        path,
                        new_hash,
                    }))
                    .await
                {
                    eprintln!(
                        "failed to send hash completion for path {:?} job_id {}: {}",
                        err.0.0.path, err.0.0.job_id, err
                    );
                }
            }
            Ok(Err(err)) => {
                eprintln!("failed to hash path {:?} job_id {}: {}", path, job_id, err);
            }
            Err(err) => {
                eprintln!(
                    "hash task failed for path {:?} job_id {}: {}",
                    path, job_id, err
                );
            }
        }

        drop(permit);
    });
    Ok(())
}
