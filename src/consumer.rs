use std::pin::Pin;
use tokio::io::AsyncWrite;

mod file;
pub use file::FileConsumer;
mod stream;

pub type ConsumerOutput = Pin<Box<dyn AsyncWrite + Send + Sync + 'static>>;
