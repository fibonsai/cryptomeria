use reqwest::Client;
use std::time::Duration;

const SCHEMA_VERSION_DDL: &str =
    "CREATE TABLE IF NOT EXISTS schema_version (version INT, name STRING, applied_on STRING)";

pub struct Migration {
    pub version: i32,
    pub name: &'static str,
    pub sql: &'static str,
}

pub struct AppliedMigration {
    pub version: i32,
    pub name: String,
    pub applied_on: String,
}

pub struct QuestDbMigrator {
    client: Client,
    http_addr: String,
}

impl QuestDbMigrator {
    pub fn new(http_addr: &str) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("reqwest client should build");
        QuestDbMigrator {
            client,
            http_addr: http_addr.to_string(),
        }
    }

    async fn execute_sql(&self, sql: &str) -> Result<(), String> {
        let url = format!(
            "http://{}/exec?query={}",
            self.http_addr,
            urlencoding::encode(sql)
        );
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !response.status().is_success() {
            let text = response.text().await.map_err(|e| e.to_string())?;
            return Err(format!("QuestDB SQL error: {}", text));
        }
        Ok(())
    }

    async fn query_json(&self, sql: &str) -> Result<serde_json::Value, String> {
        let url = format!(
            "http://{}/exec?query={}",
            self.http_addr,
            urlencoding::encode(sql)
        );
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !response.status().is_success() {
            let text = response.text().await.map_err(|e| e.to_string())?;
            return Err(format!("QuestDB SQL error: {}", text));
        }
        let body = response.text().await.map_err(|e| e.to_string())?;
        serde_json::from_str(&body).map_err(|e| format!("JSON parse error: {e}"))
    }

    async fn ensure_schema_version_table(&self) -> Result<(), String> {
        self.execute_sql(SCHEMA_VERSION_DDL).await
    }

    pub async fn list_applied(&self) -> Result<Vec<AppliedMigration>, String> {
        if self.execute_sql(SCHEMA_VERSION_DDL).await.is_err() {
            return Ok(Vec::new());
        }
        let json = self
            .query_json("SELECT version, name, applied_on FROM schema_version ORDER BY version ASC")
            .await?;
        let dataset = json["dataset"].as_array().cloned().unwrap_or_default();
        let mut applied = Vec::with_capacity(dataset.len());
        for row in &dataset {
            let parts = row.as_array().cloned().unwrap_or_default();
            if parts.len() < 3 {
                continue;
            }
            let version = parts[0].as_i64().unwrap_or(0) as i32;
            let name = parts[1].as_str().unwrap_or("").to_string();
            let applied_on = parts[2].as_str().unwrap_or("").to_string();
            applied.push(AppliedMigration {
                version,
                name,
                applied_on,
            });
        }
        Ok(applied)
    }

    pub async fn run_migrations(&self, migrations: &[Migration]) -> Result<(), String> {
        self.ensure_schema_version_table().await?;
        let applied = self.list_applied().await?;
        let applied_map: std::collections::HashMap<i32, &AppliedMigration> =
            applied.iter().map(|m| (m.version, m)).collect();

        for migration in migrations {
            let version = migration.version;
            if applied_map.contains_key(&version) {
                let existing = applied_map[&version];
                if existing.name != migration.name {
                    eprintln!(
                        "[MIGRATE] Divergent migration V{}: embedded name '{}' != applied name '{}'",
                        version, migration.name, existing.name
                    );
                }
                continue;
            }
            eprintln!("[MIGRATE] Applying V{}__{}...", version, migration.name);
            self.execute_sql(migration.sql)
                .await
                .map_err(|e| format!("Migration V{}__{} failed: {e}", version, migration.name))?;
            let insert_sql = format!(
                "INSERT INTO schema_version (version, name, applied_on) VALUES ({}, '{}', '{}')",
                version,
                migration.name.replace('\'', "''"),
                chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ")
            );
            self.execute_sql(&insert_sql)
                .await
                .map_err(|e| format!("Failed to record V{}__{}: {e}", version, migration.name))?;
            eprintln!("[MIGRATE] V{}__{} applied", version, migration.name);
        }
        Ok(())
    }
}
