use std::collections::HashSet;
use std::time::Duration;

use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::bus::BusHandle;
use crate::error::{BusError, HelmError, ModuleError};
use crate::message::{Tick, Timestamp, topics};
use crate::module::{Module, ModuleBus, ModuleContext};

pub struct Runtime {
    bus: BusHandle,
    shutdown: CancellationToken,
    modules: Vec<Box<dyn Module>>,
    handles: Vec<JoinHandle<Result<(), ModuleError>>>,
    tick_handle: Option<JoinHandle<()>>,
    started: bool,
    publishers: HashSet<&'static str>,
}

impl Runtime {
    pub fn new(bus: BusHandle) -> Self {
        Self {
            bus,
            shutdown: CancellationToken::new(),
            modules: Vec::new(),
            handles: Vec::new(),
            tick_handle: None,
            started: false,
            publishers: HashSet::new(),
        }
    }

    /// Raw bus handle without topic-declaration enforcement.
    /// For tests and external recorders only — not for use inside Module impls.
    pub fn bus(&self) -> BusHandle {
        self.bus.clone()
    }

    pub fn add_module(&mut self, module: Box<dyn Module>) -> Result<(), HelmError> {
        let topics = module.topics();
        self.bus.validate_module_topics(&topics)?;
        for name in topics.publishes {
            if !self.publishers.insert(name) {
                return Err(BusError::DuplicatePublisher(name).into());
            }
        }
        self.modules.push(module);
        Ok(())
    }

    pub async fn start(&mut self) -> Result<(), HelmError> {
        if self.started {
            return Ok(());
        }

        let modules = std::mem::take(&mut self.modules);
        for module in modules {
            let topics = module.topics();
            let ctx = ModuleContext {
                bus: ModuleBus::new(self.bus.clone(), topics),
                shutdown: self.shutdown.clone(),
            };
            let handle = tokio::spawn(async move { module.run(ctx).await });
            self.handles.push(handle);
        }

        self.started = true;
        Ok(())
    }

    pub async fn run_for_ticks(&mut self, n: u64, dt: Duration) -> Result<(), HelmError> {
        self.start().await?;

        let bus = self.bus.clone();
        let shutdown = self.shutdown.clone();
        let tick_handle = tokio::spawn(async move {
            for tick in 1..=n {
                if shutdown.is_cancelled() {
                    break;
                }
                let ts = Timestamp {
                    tick,
                    dt_secs: dt.as_secs_f64(),
                };
                let _ = bus.publish_watch(&topics::TICK, Tick { timestamp: ts });
                tokio::time::sleep(dt).await;
            }
            shutdown.cancel();
        });
        self.tick_handle = Some(tick_handle);

        for handle in self.handles.drain(..) {
            match handle.await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => return Err(e.into()),
                Err(e) => return Err(HelmError::Runtime(e.to_string())),
            }
        }

        if let Some(handle) = self.tick_handle.take() {
            let _ = handle.await;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TopicBus;
    use crate::message::ModuleTopics;
    use async_trait::async_trait;

    struct DummyModule {
        topics: ModuleTopics,
    }

    #[async_trait]
    impl Module for DummyModule {
        fn name(&self) -> &'static str {
            "dummy"
        }

        fn topics(&self) -> ModuleTopics {
            self.topics.clone()
        }

        async fn run(&self, ctx: ModuleContext) -> Result<(), ModuleError> {
            ctx.shutdown.cancelled().await;
            Ok(())
        }
    }

    fn register_all(bus: &mut TopicBus) {
        bus.register(&topics::TICK).unwrap();
        bus.register(&topics::CART_POLE_STATE).unwrap();
        bus.register(&topics::FORCE_CMD).unwrap();
    }

    #[tokio::test]
    async fn unknown_module_topic_fails_at_add() {
        let (mut bus, handle) = TopicBus::new();
        register_all(&mut bus);

        let mut runtime = Runtime::new(handle);
        let module = DummyModule {
            topics: ModuleTopics {
                subscribes: &["bad/topic"],
                publishes: &[],
            },
        };
        assert!(runtime.add_module(Box::new(module)).is_err());
    }

    #[tokio::test]
    async fn duplicate_publisher_fails_at_add() {
        let (mut bus, handle) = TopicBus::new();
        register_all(&mut bus);

        let mut runtime = Runtime::new(handle);
        runtime
            .add_module(Box::new(DummyModule {
                topics: crate::module_topics! {
                    sub: [topics::TICK],
                    publish: [topics::FORCE_CMD],
                },
            }))
            .unwrap();

        assert!(matches!(
            runtime.add_module(Box::new(DummyModule {
                topics: crate::module_topics! {
                    sub: [topics::CART_POLE_STATE],
                    publish: [topics::FORCE_CMD],
                },
            })),
            Err(HelmError::Bus(BusError::DuplicatePublisher("cmd/force")))
        ));
    }

    #[tokio::test(start_paused = true)]
    async fn run_for_ticks_starts_and_stops() {
        let (mut bus, handle) = TopicBus::new();
        register_all(&mut bus);

        let mut runtime = Runtime::new(handle);
        runtime
            .add_module(Box::new(DummyModule {
                topics: crate::module_topics! {
                    sub: [topics::TICK],
                    publish: [],
                },
            }))
            .unwrap();

        let run = runtime.run_for_ticks(3, Duration::from_millis(10));
        tokio::pin!(run);
        for _ in 0..3 {
            tokio::time::advance(Duration::from_millis(10)).await;
        }
        run.await.unwrap();
    }
}
