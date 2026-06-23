use async_trait::async_trait;

use helm_core::{Module, ModuleContext, ModuleError, ModuleTopics, module_topics, topics};

pub struct DashboardConfig {
    pub port: u16,
}

impl DashboardConfig {
    pub fn new(port: u16) -> Self {
        Self { port }
    }
}

pub struct DashboardModule {
    config: DashboardConfig,
}

impl DashboardModule {
    pub fn new(config: DashboardConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl Module for DashboardModule {
    fn name(&self) -> &'static str {
        "dashboard"
    }

    fn topics(&self) -> ModuleTopics {
        module_topics! {
            sub: [
                topics::TICK,
                topics::CART_POLE_STATE,
                topics::FORCE_CMD_SAFE,
                topics::SAFETY_STATUS,
            ],
            publish: [],
        }
    }

    async fn run(&self, ctx: ModuleContext) -> Result<(), ModuleError> {
        let _ = self.config.port;
        ctx.shutdown.cancelled().await;
        Ok(())
    }
}
