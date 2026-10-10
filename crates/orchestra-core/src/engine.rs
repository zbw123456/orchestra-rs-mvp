use crate::{
    domain::{NewTask, Status, Task},
    runtime::{self, Outcome, Provider},
    scheduler::{STRATEGIES, Scheduler},
    store::Store,
};
use anyhow::Result;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, HashSet},
    time::Duration,
};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

pub enum Action {
    Create(NewTask),
    Cancel(String),
    Snapshot,
    Events(i64),
    Strategy(String),
}
struct Request {
    action: Action,
    reply: oneshot::Sender<Result<Value, String>>,
}

#[derive(Clone)]
pub struct Client {
    tx: mpsc::Sender<Request>,
}
impl Client {
    pub async fn call(&self, action: Action) -> Result<Value, String> {
        let (reply, result) = oneshot::channel();
        self.tx
            .send(Request { action, reply })
            .await
            .map_err(|_| "engine unavailable")?;
        result.await.map_err(|_| "engine unavailable")?
    }
}

struct Active {
    cancel: CancellationToken,
    handle: tokio::task::JoinHandle<()>,
}

pub struct Engine {
    store: Store,
    tasks: BTreeMap<String, Task>,
    providers: BTreeMap<String, Provider>,
    active: BTreeMap<String, Active>,
    scheduler: Scheduler,
    strategy: String,
    source: String,
    capacity: usize,
    requests: mpsc::Receiver<Request>,
    finish_tx: mpsc::Sender<(String, Outcome)>,
    finishes: mpsc::Receiver<(String, Outcome)>,
    stop: CancellationToken,
}

impl Engine {
    pub fn new(
        mut store: Store,
        providers: BTreeMap<String, Provider>,
        scheduler: Scheduler,
        strategy: String,
        capacity: usize,
        stop: CancellationToken,
    ) -> Result<(Self, Client)> {
        anyhow::ensure!(STRATEGIES.contains(&strategy.as_str()), "unknown strategy");
        anyhow::ensure!((1..=32).contains(&capacity), "workers must be 1..32");
        let mut tasks = BTreeMap::new();
        for mut task in store.load()? {
            // Never replay an execution whose effects are unknown after a crash.
            if task.status == Status::Running {
                task.transition(
                    Status::Interrupted,
                    "core restarted; previous execution outcome is unknown",
                )
                .map_err(anyhow::Error::msg)?;
                store.save(&task)?;
            }
            tasks.insert(task.id.clone(), task);
        }
        let (tx, requests) = mpsc::channel(128);
        let (finish_tx, finishes) = mpsc::channel(64);
        Ok((
            Self {
                store,
                tasks,
                providers,
                active: BTreeMap::new(),
                scheduler,
                strategy,
                source: "no scheduling decision yet".into(),
                capacity,
                requests,
                finish_tx,
                finishes,
                stop,
            },
            Client { tx },
        ))
    }

    fn commit(&mut self, task: Task) -> Result<()> {
        self.store.save(&task)?;
        self.tasks.insert(task.id.clone(), task);
        Ok(())
    }

    fn handle(&mut self, action: Action) -> Result<Value> {
        match action {
            Action::Create(spec) => {
                spec.validate().map_err(anyhow::Error::msg)?;
                anyhow::ensure!(
                    self.tasks.len() < 1000,
                    "MVP limit: 1000 retained tasks; use a new database for a new experiment"
                );
                anyhow::ensure!(
                    self.providers.contains_key(&spec.provider),
                    "unknown provider"
                );
                let unique: HashSet<_> = spec.depends_on.iter().collect();
                anyhow::ensure!(
                    unique.len() == spec.depends_on.len(),
                    "duplicate dependencies"
                );
                anyhow::ensure!(
                    spec.depends_on.iter().all(|id| self.tasks.contains_key(id)),
                    "dependencies must refer to existing tasks"
                );
                // Dependencies can only point backwards to immutable existing tasks: cycles are impossible.
                let task = Task::new(spec);
                self.commit(task.clone())?;
                Ok(serde_json::to_value(task)?)
            }
            Action::Cancel(id) => {
                let mut task = self
                    .tasks
                    .get(&id)
                    .ok_or_else(|| anyhow::anyhow!("task not found"))?
                    .clone();
                if !task.status.terminal() && !task.cancel_requested {
                    task.cancel_requested = true;
                    if task.status == Status::Queued {
                        task.transition(Status::Cancelled, "cancelled while queued")
                            .map_err(anyhow::Error::msg)?;
                    } else {
                        task.detail = "cancellation requested; waiting for process cleanup".into();
                        task.revision += 1;
                    }
                    self.commit(task.clone())?;
                    if let Some(active) = self.active.get(&id) {
                        active.cancel.cancel();
                    }
                }
                Ok(serde_json::to_value(task)?)
            }
            Action::Snapshot => {
                let mut tasks: Vec<_> = self.tasks.values().collect();
                tasks.sort_by_key(|t| (std::cmp::Reverse(t.created_at_ms), &t.id));
                Ok(
                    json!({"schema_version":1, "tasks":tasks, "capacity":self.capacity,
                    "running":self.active.len(), "strategy":self.strategy,
                    "scheduling_source":self.source, "providers":self.providers.keys().collect::<Vec<_>>()}),
                )
            }
            Action::Events(after) => Ok(serde_json::to_value(self.store.events(after)?)?),
            Action::Strategy(strategy) => {
                anyhow::ensure!(STRATEGIES.contains(&strategy.as_str()), "unknown strategy");
                self.strategy = strategy;
                Ok(json!({"strategy":self.strategy}))
            }
        }
    }

    async fn dispatch(&mut self) -> Result<()> {
        // Propagate failed dependencies until no further queued task can be blocked.
        loop {
            let blocked: Vec<_> = self
                .tasks
                .values()
                .filter(|t| {
                    t.status == Status::Queued
                        && t.spec.depends_on.iter().any(|id| {
                            self.tasks.get(id).is_none_or(|d| {
                                d.status.terminal() && d.status != Status::Succeeded
                            })
                        })
                })
                .cloned()
                .collect();
            if blocked.is_empty() {
                break;
            }
            for mut task in blocked {
                task.transition(Status::Blocked, "dependency did not succeed")
                    .map_err(anyhow::Error::msg)?;
                self.commit(task)?;
            }
        }
        let slots = self.capacity.saturating_sub(self.active.len());
        if slots == 0 {
            return Ok(());
        }
        let ready: Vec<_> = self
            .tasks
            .values()
            .filter(|t| {
                t.status == Status::Queued
                    && t.spec.depends_on.iter().all(|id| {
                        self.tasks
                            .get(id)
                            .is_some_and(|d| d.status == Status::Succeeded)
                    })
            })
            .cloned()
            .collect();
        if ready.is_empty() {
            return Ok(());
        }
        let (order, source) = self.scheduler.rank(&ready, &self.strategy).await;
        self.source = source;
        if self.stop.is_cancelled() {
            return Ok(());
        }
        for id in order.into_iter().take(slots) {
            let mut task = self.tasks[&id].clone();
            let Some(provider) = self.providers.get(&task.spec.provider).cloned() else {
                task.transition(Status::Blocked, "provider removed from configuration")
                    .map_err(anyhow::Error::msg)?;
                self.commit(task)?;
                continue;
            };
            task.transition(Status::Running, format!("dispatched via {}", self.source))
                .map_err(anyhow::Error::msg)?;
            self.commit(task.clone())?;
            let cancel = CancellationToken::new();
            let token = cancel.clone();
            let finish = self.finish_tx.clone();
            let task_id = id.clone();
            let handle = tokio::spawn(async move {
                let outcome = runtime::run(task, provider, token).await;
                let _ = finish.send((task_id, outcome)).await;
            });
            self.active.insert(id, Active { cancel, handle });
        }
        Ok(())
    }

    pub async fn run(mut self) -> Result<()> {
        let result = self.run_loop().await;
        self.stop.cancel();
        for active in self.active.values() {
            active.cancel.cancel();
        }
        for (_, active) in std::mem::take(&mut self.active) {
            let _ = active.handle.await;
        }
        result
    }

    async fn run_loop(&mut self) -> Result<()> {
        let mut interval = tokio::time::interval(Duration::from_millis(100));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut stopping = false;
        loop {
            if stopping && self.active.is_empty() {
                return Ok(());
            }
            tokio::select! {
                _ = self.stop.cancelled(), if !stopping => {
                    stopping = true;
                    for active in self.active.values() { active.cancel.cancel(); }
                }
                Some(request) = self.requests.recv() => {
                    let result = if stopping { Err("engine shutting down".into()) }
                        else { self.handle(request.action).map_err(|e| e.to_string()) };
                    let _ = request.reply.send(result);
                }
                Some((id, outcome)) = self.finishes.recv() => {
                    self.active.remove(&id);
                    let mut task = self.tasks[&id].clone();
                    let status = if task.cancel_requested { Status::Cancelled } else { outcome.status };
                    let detail = if task.cancel_requested { "cancelled by user".into() } else { outcome.detail };
                    task.output = outcome.output;
                    task.transition(status, detail).map_err(anyhow::Error::msg)?;
                    self.commit(task)?;
                }
                _ = interval.tick(), if !stopping => self.dispatch().await?,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn engine(store: Store) -> Engine {
        Engine::new(
            store,
            runtime::providers(None).unwrap(),
            Scheduler::new("http://127.0.0.1:1".into()).unwrap(),
            "fifo".into(),
            2,
            CancellationToken::new(),
        )
        .unwrap()
        .0
    }
    #[test]
    fn restart_marks_inflight_interrupted_but_keeps_queue() {
        let mut store = Store::open(":memory:").unwrap();
        let mut task = Task::new(serde_json::from_str(r#"{"title":"inflight"}"#).unwrap());
        task.transition(Status::Running, "start").unwrap();
        store.save(&task).unwrap();
        let queued = Task::new(serde_json::from_str(r#"{"title":"queue"}"#).unwrap());
        store.save(&queued).unwrap();
        let engine = engine(store);
        assert_eq!(engine.tasks[&task.id].status, Status::Interrupted);
        assert_eq!(engine.tasks[&queued.id].status, Status::Queued);
    }
    #[tokio::test]
    async fn dependencies_block_and_unknown_dependencies_are_rejected() {
        let mut engine = engine(Store::open(":memory:").unwrap());
        let spec = serde_json::from_str(r#"{"title":"parent"}"#).unwrap();
        let id = engine.handle(Action::Create(spec)).unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        let child = serde_json::from_value(json!({"title":"child", "depends_on":[id]})).unwrap();
        let child_id = engine.handle(Action::Create(child)).unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        engine.handle(Action::Cancel(id)).unwrap();
        engine.dispatch().await.unwrap();
        assert_eq!(engine.tasks[&child_id].status, Status::Blocked);
        assert!(
            engine
                .handle(Action::Create(
                    serde_json::from_value(json!({"title":"bad", "depends_on":["missing"]}))
                        .unwrap()
                ))
                .is_err()
        );
    }
}
