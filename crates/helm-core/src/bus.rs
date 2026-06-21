use std::any::TypeId;
use std::collections::HashSet;
use std::sync::{Arc, Mutex, RwLock};

use tokio::sync::{mpsc, watch};

use crate::error::BusError;
use crate::message::{ModuleTopics, Topic, TopicKind};

struct CmdTopic {
    type_id: TypeId,
    tx: Box<dyn std::any::Any + Send + Sync>,
    rx: Mutex<Option<Box<dyn std::any::Any + Send>>>,
}

struct TopicBusInner {
    registered: RwLock<HashSet<&'static str>>,
    watch: RwLock<WatchRegistry>,
    cmd: RwLock<CmdRegistry>,
}

type WatchRegistry =
    std::collections::HashMap<&'static str, (TypeId, Box<dyn std::any::Any + Send + Sync>)>;
type CmdRegistry = std::collections::HashMap<&'static str, CmdTopic>;

pub struct TopicBus {
    inner: Arc<TopicBusInner>,
}

#[derive(Clone)]
pub struct BusHandle {
    inner: Arc<TopicBusInner>,
}

impl TopicBus {
    pub fn new() -> (Self, BusHandle) {
        let inner = Arc::new(TopicBusInner {
            registered: RwLock::new(HashSet::new()),
            watch: RwLock::new(std::collections::HashMap::new()),
            cmd: RwLock::new(std::collections::HashMap::new()),
        });
        let handle = BusHandle {
            inner: Arc::clone(&inner),
        };
        (Self { inner }, handle)
    }

    pub fn register<T: Clone + Copy + Send + Sync + 'static>(
        &mut self,
        topic: &'static Topic<T>,
    ) -> Result<(), BusError> {
        {
            let registered = self.inner.registered.read().expect("registered lock");
            if registered.contains(topic.name) {
                return Err(BusError::DuplicateTopic(topic.name));
            }
        }

        match topic.kind {
            TopicKind::Watch => {
                let (tx, _rx) = watch::channel(topic.seed);
                self.inner
                    .watch
                    .write()
                    .expect("watch registry lock")
                    .insert(topic.name, (TypeId::of::<T>(), Box::new(tx)));
            }
            TopicKind::Command => {
                let (tx, rx) = mpsc::channel::<T>(16);
                self.inner.cmd.write().expect("cmd registry lock").insert(
                    topic.name,
                    CmdTopic {
                        type_id: TypeId::of::<T>(),
                        tx: Box::new(tx),
                        rx: Mutex::new(Some(Box::new(rx))),
                    },
                );
            }
        }

        self.inner
            .registered
            .write()
            .expect("registered lock")
            .insert(topic.name);
        Ok(())
    }

    pub fn validate_module_topics(&self, topics: &ModuleTopics) -> Result<(), BusError> {
        let registered = self.inner.registered.read().expect("registered lock");
        for name in topics.subscribes {
            if !registered.contains(name) {
                return Err(BusError::UnknownTopic(name));
            }
        }
        for name in topics.publishes {
            if !registered.contains(name) {
                return Err(BusError::UnknownTopic(name));
            }
        }
        Ok(())
    }
}

impl BusHandle {
    fn is_registered(&self, name: &'static str) -> bool {
        self.inner
            .registered
            .read()
            .expect("registered lock")
            .contains(name)
    }

    pub fn subscribe_watch<T: Clone + Copy + Send + Sync + 'static>(
        &self,
        topic: &'static Topic<T>,
    ) -> Result<watch::Receiver<T>, BusError> {
        if topic.kind != TopicKind::Watch {
            return Err(BusError::TypeMismatch(topic.name));
        }
        if !self.is_registered(topic.name) {
            return Err(BusError::NotRegistered(topic.name));
        }

        let watch = self.inner.watch.read().expect("watch registry lock");
        let (type_id, sender) = watch
            .get(topic.name)
            .ok_or(BusError::NotRegistered(topic.name))?;

        if *type_id != TypeId::of::<T>() {
            return Err(BusError::TypeMismatch(topic.name));
        }

        let tx = sender
            .downcast_ref::<watch::Sender<T>>()
            .ok_or(BusError::TypeMismatch(topic.name))?;

        Ok(tx.subscribe())
    }

    pub fn publish_watch<T: Clone + Copy + Send + Sync + 'static>(
        &self,
        topic: &'static Topic<T>,
        payload: T,
    ) -> Result<(), BusError> {
        if topic.kind != TopicKind::Watch {
            return Err(BusError::TypeMismatch(topic.name));
        }
        if !self.is_registered(topic.name) {
            return Err(BusError::NotRegistered(topic.name));
        }

        let watch = self.inner.watch.read().expect("watch registry lock");
        let (type_id, sender) = watch
            .get(topic.name)
            .ok_or(BusError::NotRegistered(topic.name))?;

        if *type_id != TypeId::of::<T>() {
            return Err(BusError::TypeMismatch(topic.name));
        }

        let tx = sender
            .downcast_ref::<watch::Sender<T>>()
            .ok_or(BusError::TypeMismatch(topic.name))?;

        tx.send(payload).map_err(|_| BusError::ChannelClosed)
    }

    pub fn subscribe_cmd<T: Clone + Copy + Send + Sync + 'static>(
        &self,
        topic: &'static Topic<T>,
    ) -> Result<mpsc::Receiver<T>, BusError> {
        if topic.kind != TopicKind::Command {
            return Err(BusError::TypeMismatch(topic.name));
        }
        if !self.is_registered(topic.name) {
            return Err(BusError::NotRegistered(topic.name));
        }

        let mut cmd = self.inner.cmd.write().expect("cmd registry lock");
        let entry = cmd
            .get_mut(topic.name)
            .ok_or(BusError::NotRegistered(topic.name))?;

        if entry.type_id != TypeId::of::<T>() {
            return Err(BusError::TypeMismatch(topic.name));
        }

        let rx = entry
            .rx
            .lock()
            .expect("cmd rx lock")
            .take()
            .ok_or(BusError::CommandClosed)?;

        rx.downcast::<mpsc::Receiver<T>>()
            .map(|boxed| *boxed)
            .map_err(|_| BusError::TypeMismatch(topic.name))
    }

    pub fn publish_cmd<T: Clone + Copy + Send + Sync + 'static>(
        &self,
        topic: &'static Topic<T>,
        payload: T,
    ) -> Result<(), BusError> {
        if topic.kind != TopicKind::Command {
            return Err(BusError::TypeMismatch(topic.name));
        }
        if !self.is_registered(topic.name) {
            return Err(BusError::NotRegistered(topic.name));
        }

        let cmd = self.inner.cmd.read().expect("cmd registry lock");
        let entry = cmd
            .get(topic.name)
            .ok_or(BusError::NotRegistered(topic.name))?;

        if entry.type_id != TypeId::of::<T>() {
            return Err(BusError::TypeMismatch(topic.name));
        }

        let tx = entry
            .tx
            .downcast_ref::<mpsc::Sender<T>>()
            .ok_or(BusError::TypeMismatch(topic.name))?;

        match tx.try_send(payload) {
            Ok(()) => Ok(()),
            Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => Err(BusError::ChannelFull),
            Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => Err(BusError::CommandClosed),
        }
    }

    pub fn validate_module_topics(&self, topics: &ModuleTopics) -> Result<(), BusError> {
        let registered = self.inner.registered.read().expect("registered lock");
        for name in topics.subscribes {
            if !registered.contains(name) {
                return Err(BusError::UnknownTopic(name));
            }
        }
        for name in topics.publishes {
            if !registered.contains(name) {
                return Err(BusError::UnknownTopic(name));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::topics;
    use crate::message::{CartPoleState, ForceCommand, Tick};

    fn register_all(bus: &mut TopicBus) {
        bus.register(&topics::TICK).unwrap();
        bus.register(&topics::CART_POLE_STATE).unwrap();
        bus.register(&topics::FORCE_CMD).unwrap();
    }

    #[test]
    fn watch_roundtrip() {
        let (mut bus, handle) = TopicBus::new();
        register_all(&mut bus);

        let rx = handle.subscribe_watch(&topics::TICK).unwrap();
        handle
            .publish_watch(
                &topics::TICK,
                Tick {
                    timestamp: crate::message::Timestamp {
                        tick: 1,
                        dt_secs: 0.01,
                    },
                },
            )
            .unwrap();

        assert_eq!(rx.borrow().timestamp.tick, 1);
    }

    #[test]
    fn watch_two_subscribers_see_latest() {
        let (mut bus, handle) = TopicBus::new();
        register_all(&mut bus);

        let rx1 = handle.subscribe_watch(&topics::CART_POLE_STATE).unwrap();
        let rx2 = handle.subscribe_watch(&topics::CART_POLE_STATE).unwrap();

        let state = CartPoleState {
            x: 1.0,
            ..CartPoleState::INITIAL
        };
        handle.publish_watch(&topics::CART_POLE_STATE, state).unwrap();

        assert_eq!(rx1.borrow().x, 1.0);
        assert_eq!(rx2.borrow().x, 1.0);
    }

    #[test]
    fn watch_seeded_at_registration() {
        let (mut bus, handle) = TopicBus::new();
        register_all(&mut bus);

        let rx = handle.subscribe_watch(&topics::CART_POLE_STATE).unwrap();
        assert_eq!(rx.borrow().theta, CartPoleState::INITIAL.theta);
    }

    #[test]
    fn force_cmd_watch_roundtrip() {
        let (mut bus, handle) = TopicBus::new();
        register_all(&mut bus);

        let rx = handle.subscribe_watch(&topics::FORCE_CMD).unwrap();
        handle
            .publish_watch(&topics::FORCE_CMD, ForceCommand { force_n: 3.5 })
            .unwrap();

        assert_eq!(rx.borrow().force_n, 3.5);
    }

    #[test]
    fn unknown_topic_fails_validation() {
        let (_bus, handle) = TopicBus::new();
        let topics = ModuleTopics {
            subscribes: &["typo/topic"],
            publishes: &[],
        };
        assert!(matches!(
            handle.validate_module_topics(&topics),
            Err(BusError::UnknownTopic("typo/topic"))
        ));
    }

    #[test]
    fn duplicate_register_fails() {
        let (mut bus, _handle) = TopicBus::new();
        bus.register(&topics::TICK).unwrap();
        assert!(matches!(
            bus.register(&topics::TICK),
            Err(BusError::DuplicateTopic(_))
        ));
    }

    #[test]
    fn publish_before_register_fails() {
        let (_bus, handle) = TopicBus::new();
        assert!(matches!(
            handle.publish_watch(&topics::TICK, topics::TICK.seed),
            Err(BusError::NotRegistered(_))
        ));
    }

    #[test]
    fn watch_publish_after_all_receivers_dropped() {
        let (mut bus, handle) = TopicBus::new();
        register_all(&mut bus);

        let rx = handle.subscribe_watch(&topics::TICK).unwrap();
        drop(rx);

        assert!(matches!(
            handle.publish_watch(&topics::TICK, topics::TICK.seed),
            Err(BusError::ChannelClosed)
        ));
    }

    #[test]
    fn cmd_publish_full_buffer() {
        #[derive(Clone, Copy)]
        struct Cmd {
            _v: u8,
        }

        static CMD_TOPIC: Topic<Cmd> = Topic::new("test/cmd", TopicKind::Command, Cmd { _v: 0 });

        let (mut bus, handle) = TopicBus::new();
        bus.register(&CMD_TOPIC).unwrap();
        let _rx = handle.subscribe_cmd(&CMD_TOPIC).unwrap();

        for i in 0..16 {
            handle.publish_cmd(&CMD_TOPIC, Cmd { _v: i }).unwrap();
        }

        assert!(matches!(
            handle.publish_cmd(&CMD_TOPIC, Cmd { _v: 99 }),
            Err(BusError::ChannelFull)
        ));
    }
}
