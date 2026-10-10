use crate::domain::Task;
use serde::Deserialize;
use std::{collections::HashSet, time::Duration};

pub const STRATEGIES: [&str; 4] = ["fifo", "sjf", "priority", "aging"];

pub struct Scheduler {
    client: reqwest::Client,
    url: String,
}

#[derive(Deserialize)]
struct Ranking {
    schema_version: u32,
    ordered_ids: Vec<String>,
}

pub fn fifo(tasks: &[Task]) -> Vec<String> {
    let mut tasks: Vec<_> = tasks.iter().collect();
    tasks.sort_by_key(|t| (t.created_at_ms, &t.id));
    tasks.into_iter().map(|t| t.id.clone()).collect()
}

pub fn valid_ranking(ids: &[String], tasks: &[Task]) -> bool {
    let expected: HashSet<_> = tasks.iter().map(|t| &t.id).collect();
    ids.len() == tasks.len() && ids.iter().collect::<HashSet<_>>() == expected
}

impl Scheduler {
    pub fn new(url: String) -> anyhow::Result<Self> {
        let url_parsed = reqwest::Url::parse(&url)?;
        anyhow::ensure!(
            url_parsed.scheme() == "http"
                && matches!(
                    url_parsed.host_str(),
                    Some("127.0.0.1" | "localhost" | "[::1]")
                ),
            "scheduler must use loopback HTTP"
        );
        Ok(Self {
            url,
            client: reqwest::Client::builder()
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_millis(300))
                .build()?,
        })
    }
    pub async fn rank(&self, tasks: &[Task], strategy: &str) -> (Vec<String>, String) {
        let candidates: Vec<_> = tasks.iter().map(|t| serde_json::json!({"id": t.id,
            "created_at_ms": t.created_at_ms, "estimate_ms": t.spec.estimate_ms, "priority": t.spec.priority})).collect();
        let request = serde_json::json!({"schema_version": 1, "strategy": strategy,
            "now_ms": crate::domain::now_ms(), "tasks": candidates});
        let result = async {
            let mut response = self
                .client
                .post(format!("{}/v1/rank", self.url))
                .json(&request)
                .send()
                .await?
                .error_for_status()?;
            let mut body = Vec::new();
            while let Some(chunk) = response.chunk().await? {
                anyhow::ensure!(
                    body.len() + chunk.len() <= 256_000,
                    "scheduler response too large"
                );
                body.extend_from_slice(&chunk);
            }
            let ranking: Ranking = serde_json::from_slice(&body)?;
            anyhow::ensure!(
                ranking.schema_version == 1 && valid_ranking(&ranking.ordered_ids, tasks),
                "invalid scheduler permutation"
            );
            Ok::<_, anyhow::Error>(ranking.ordered_ids)
        }
        .await;
        match result {
            Ok(ids) => (ids, format!("python:{strategy}")),
            Err(error) => (fifo(tasks), format!("fallback:fifo ({error})")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ranking_must_be_an_exact_permutation() {
        let a = Task::new(serde_json::from_str(r#"{"title":"a"}"#).unwrap());
        let b = Task::new(serde_json::from_str(r#"{"title":"b"}"#).unwrap());
        let tasks = vec![a.clone(), b.clone()];
        assert!(valid_ranking(&[b.id.clone(), a.id.clone()], &tasks));
        assert!(!valid_ranking(&[a.id.clone(), a.id.clone()], &tasks));
        assert!(!valid_ranking(&["unknown".into(), b.id.clone()], &tasks));
        assert!(!valid_ranking(&[a.id], &tasks));
    }
    #[tokio::test]
    async fn outage_falls_back_without_losing_work() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        let scheduler = Scheduler::new(format!("http://{address}")).unwrap();
        let tasks = vec![Task::new(serde_json::from_str(r#"{"title":"a"}"#).unwrap())];
        let (ids, source) = scheduler.rank(&tasks, "sjf").await;
        assert_eq!(ids, fifo(&tasks));
        assert!(source.starts_with("fallback:fifo"));
    }
}
