use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use crate::bus::BusHandle;
use crate::error::BusError;
use crate::message::{ModuleTopics, Topic};

pub struct ModuleBus {
    inner: BusHandle,
    topics: ModuleTopics,
}

impl ModuleBus {
    pub fn new(inner: BusHandle, topics: ModuleTopics) -> Self {
        Self { inner, topics }
    }

    fn ensure_subscribe(&self, name: &'static str) -> Result<(), BusError> {
        if !self.topics.subscribes.contains(&name) {
            return Err(BusError::UndeclaredSubscribe(name));
        }
        Ok(())
    }

    fn ensure_publish(&self, name: &'static str) -> Result<(), BusError> {
        if !self.topics.publishes.contains(&name) {
            return Err(BusError::UndeclaredPublish(name));
        }
        Ok(())
    }

    pub fn subscribe_watch<T: Clone + Copy + Send + Sync + 'static>(
        &self,
        topic: &'static Topic<T>,
    ) -> Result<tokio::sync::watch::Receiver<T>, BusError> {
        self.ensure_subscribe(topic.name)?;
        self.inner.subscribe_watch(topic)
    }

    pub fn publish_watch<T: Clone + Copy + Send + Sync + 'static>(
        &self,
        topic: &'static Topic<T>,
        payload: T,
    ) -> Result<(), BusError> {
        self.ensure_publish(topic.name)?;
        self.inner.publish_watch(topic, payload)
    }

    pub fn subscribe_cmd<T: Clone + Copy + Send + Sync + 'static>(
        &self,
        topic: &'static Topic<T>,
    ) -> Result<tokio::sync::mpsc::Receiver<T>, BusError> {
        self.ensure_subscribe(topic.name)?;
        self.inner.subscribe_cmd(topic)
    }

    pub fn publish_cmd<T: Clone + Copy + Send + Sync + 'static>(
        &self,
        topic: &'static Topic<T>,
        payload: T,
    ) -> Result<(), BusError> {
        self.ensure_publish(topic.name)?;
        self.inner.publish_cmd(topic, payload)
    }
}

pub struct ModuleContext {
    pub bus: ModuleBus,
    pub shutdown: CancellationToken,
}

#[async_trait]
pub trait Module: Send + Sync + 'static {
    fn name(&self) -> &'static str;
    fn topics(&self) -> ModuleTopics;
    async fn run(&self, ctx: ModuleContext) -> Result<(), crate::error::ModuleError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TopicBus;
    use crate::message::topics;

    fn register_all(bus: &mut TopicBus) {
        bus.register(&topics::TICK).unwrap();
        bus.register(&topics::CART_POLE_STATE).unwrap();
        bus.register(&topics::FORCE_CMD).unwrap();
        bus.register(&topics::FORCE_CMD_SAFE).unwrap();
        bus.register(&topics::SAFETY_STATUS).unwrap();
    }

    #[test]
    fn rejects_undeclared_subscribe() {
        let (mut bus, handle) = TopicBus::new();
        register_all(&mut bus);

        let module_bus = ModuleBus::new(
            handle,
            crate::module_topics! {
                sub: [topics::TICK],
                publish: [],
            },
        );

        assert!(matches!(
            module_bus.subscribe_watch(&topics::CART_POLE_STATE),
            Err(BusError::UndeclaredSubscribe("state/cart_pole"))
        ));
    }

    #[test]
    fn rejects_undeclared_publish() {
        let (mut bus, handle) = TopicBus::new();
        register_all(&mut bus);

        let module_bus = ModuleBus::new(
            handle,
            crate::module_topics! {
                sub: [topics::TICK],
                publish: [],
            },
        );

        assert!(matches!(
            module_bus.publish_watch(&topics::CART_POLE_STATE, topics::CART_POLE_STATE.seed),
            Err(BusError::UndeclaredPublish("state/cart_pole"))
        ));
    }
}
