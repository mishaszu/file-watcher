use std::{cmp::max, collections::HashMap, io::Read, path::PathBuf, sync::Arc};

use tokio::sync::{Semaphore, mpsc};

pub struct HasherIncomingMsg(pub u64, pub PathBuf);
pub struct HasherReadyMsg(pub u64, pub PathBuf, pub String);

pub async fn hash_worker(
    mut files_to_hash_rx: mpsc::Receiver<HasherIncomingMsg>,
    complete_hash_tx: mpsc::Sender<HasherReadyMsg>,
) {
    let mut state: HashMap<PathBuf, u64> = HashMap::new();

    let max_workers = max(1, num_cpus::get() - 2);

    let semaphore = Arc::new(Semaphore::new(max_workers));

    while let Some(HasherIncomingMsg(new_ver, path)) = files_to_hash_rx.recv().await {
        if let Some(saved_ver) = state.get(&path)
            && new_ver <= *saved_ver
        {
            // file with bigger version alread in process
        } else {
            state.insert(path.clone(), new_ver);
            spawn_hash_job(
                HasherIncomingMsg(new_ver, path),
                semaphore.clone(),
                complete_hash_tx.clone(),
            )
            .await;
        }
    }
}

async fn spawn_hash_job(
    HasherIncomingMsg(version, path): HasherIncomingMsg,
    semaphore: Arc<Semaphore>,
    complete_tx: mpsc::Sender<HasherReadyMsg>,
) {
    let permit = semaphore.acquire_owned().await.unwrap();

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

        if let Ok(Ok(hash)) = result {
            let _ = complete_tx.send(HasherReadyMsg(version, path, hash)).await;
        }

        drop(permit);
    });
}
