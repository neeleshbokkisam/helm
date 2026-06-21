use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use crate::bus::BusHandle;
use crate::error::ModuleError;
use crate::message::ModuleTopics;

pub struct ModuleContext {
    pub bus: BusHandle,
    pub shutdown: CancellationToken,
}

#[async_trait]
pub trait Module: Send + Sync + 'static {
    fn name(&self) -> &'static str;
    fn topics(&self) -> ModuleTopics;
    async fn run(&self, ctx: ModuleContext) -> Result<(), ModuleError>;
}
