use thiserror::Error;

#[derive(Debug, Error)]
pub enum BusError {
    #[error("unknown topic: {0}")]
    UnknownTopic(&'static str),
    #[error("duplicate topic: {0}")]
    DuplicateTopic(&'static str),
    #[error("duplicate publisher for topic: {0}")]
    DuplicatePublisher(&'static str),
    #[error("topic type mismatch: {0}")]
    TypeMismatch(&'static str),
    #[error("topic not registered: {0}")]
    NotRegistered(&'static str),
    #[error("undeclared subscribe: {0}")]
    UndeclaredSubscribe(&'static str),
    #[error("undeclared publish: {0}")]
    UndeclaredPublish(&'static str),
    #[error("watch channel closed")]
    ChannelClosed,
    #[error("command channel full")]
    ChannelFull,
    #[error("command channel closed")]
    CommandClosed,
}

#[derive(Debug, Error)]
pub enum ModuleError {
    #[error("module {0} failed: {1}")]
    Failed(&'static str, String),
    #[error("bus error: {0}")]
    Bus(#[from] BusError),
}

#[derive(Debug, Error)]
pub enum HelmError {
    #[error("module error: {0}")]
    Module(#[from] ModuleError),
    #[error("bus error: {0}")]
    Bus(#[from] BusError),
    #[error("runtime error: {0}")]
    Runtime(String),
}
