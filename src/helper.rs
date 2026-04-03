use tokio::sync::mpsc::error::TrySendError;

pub fn eprint_try_send_error<T>(err: TrySendError<T>) {
    match err {
        TrySendError::Full(_) => {
            eprintln!("controller: sink channel full; dropping event");
        }
        TrySendError::Closed(_) => {
            panic!("controller: sink channel closed; stopping controller");
        }
    }
}
