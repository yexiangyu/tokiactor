#[derive(Debug, thiserror::Error)]
pub enum HandleError {
    #[error("send actor input error")]
    SendError,
    #[error("recv actor input error")]
    RecvError,
}

pub type HandleResult<T> = Result<T, HandleError>;
