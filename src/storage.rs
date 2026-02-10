use crate::metrics::*;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use std::path::PathBuf;
use std::sync::Mutex;
use tracing::debug;

pub struct MetricsStore {
    #[allow(dead_code)]
    db_path: PathBuf,
    conn: Mutex<Connection>,
}

unsafe impl Send for MetricsStore {}
unsafe impl Sync for MetricsStore {}

impl MetricsStore {
    pub fn new<P: AsRef<std::path::Path>>(path: P) -> anyhow::Result<Self> {
        let db_path = path.as_ref().to_path_buf();
        let conn = Connection::open(&db_path)?;
        let store = Self { 
            db_path,
            conn: Mutex::new(conn),
        };
        store.initialize_schema()?;
        Ok(store)
    }

    fn initialize_schema(&self) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            r#"
            -- Main snapshots table
            CREATE TABLE IF NOT EXISTS snapshots (
                id TEXT PRIMARY KEY,
                timestamp TEXT NOT NULL,
                data JSON NOT NULL
            );

            -- Index for time-based queries
            CREATE INDEX IF NOT EXISTS idx_snapshots_timestamp ON snapshots(timestamp);

            -- Events table for quick event queries
            CREATE TABLE IF NOT EXISTS events (
                id TEXT PRIMARY KEY,
                snapshot_id TEXT NOT NULL,
                timestamp TEXT NOT NULL,
                event_type TEXT NOT NULL,
                severity TEXT NOT NULL,
                description TEXT NOT NULL,
                details JSON,
                FOREIGN KEY (snapshot_id) REFERENCES snapshots(id)
            );

            CREATE INDEX IF NOT EXISTS idx_events_timestamp ON events(timestamp);
            CREATE INDEX IF NOT EXISTS idx_events_type ON events(event_type);
            CREATE INDEX IF NOT EXISTS idx_events_severity ON events(severity);

            -- Time series data for efficient charting
            CREATE TABLE IF NOT EXISTS timeseries (
                timestamp TEXT NOT NULL,
                metric_name TEXT NOT NULL,
                value REAL NOT NULL,
                PRIMARY KEY (timestamp, metric_name)
            );

            CREATE INDEX IF NOT EXISTS idx_timeseries_metric ON timeseries(metric_name, timestamp);

            -- Statistics aggregates (hourly)
            CREATE TABLE IF NOT EXISTS hourly_stats (
                hour TEXT PRIMARY KEY,
                sample_count INTEGER NOT NULL,
                signal_avg REAL,
                signal_min INTEGER,
                signal_max INTEGER,
                latency_avg REAL,
                latency_min REAL,
                latency_max REAL,
                jitter_avg REAL,
                packet_loss_avg REAL,
                uptime_percent REAL,
                internet_uptime_percent REAL,
                disconnections INTEGER,
                warning_events INTEGER,
                error_events INTEGER,
                critical_events INTEGER
            );
            "#,
        )?;

        Ok(())
    }

    pub fn save_snapshot(&self, snapshot: &WifiSnapshot) -> anyhow::Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;

        // Save main snapshot
        let data = serde_json::to_string(snapshot)?;
        tx.execute(
            "INSERT INTO snapshots (id, timestamp, data) VALUES (?1, ?2, ?3)",
            params![
                snapshot.id,
                snapshot.timestamp.to_rfc3339(),
                data
            ],
        )?;

        // Save events
        for event in &snapshot.events {
            let details = serde_json::to_string(&event.details)?;
            tx.execute(
                "INSERT INTO events (id, snapshot_id, timestamp, event_type, severity, description, details) 
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    event.id,
                    snapshot.id,
                    event.timestamp.to_rfc3339(),
                    format!("{:?}", event.event_type),
                    format!("{:?}", event.severity),
                    event.description,
                    details
                ],
            )?;
        }

        // Save time series data
        let ts = snapshot.timestamp.to_rfc3339();

        if let Some(ref wifi) = snapshot.wifi_info {
            tx.execute(
                "INSERT OR REPLACE INTO timeseries (timestamp, metric_name, value) VALUES (?1, ?2, ?3)",
                params![ts, "signal_dbm", wifi.signal_strength_dbm as f64],
            )?;
            tx.execute(
                "INSERT OR REPLACE INTO timeseries (timestamp, metric_name, value) VALUES (?1, ?2, ?3)",
                params![ts, "signal_percent", wifi.signal_quality_percent as f64],
            )?;
            tx.execute(
                "INSERT OR REPLACE INTO timeseries (timestamp, metric_name, value) VALUES (?1, ?2, ?3)",
                params![ts, "channel", wifi.channel as f64],
            )?;
            tx.execute(
                "INSERT OR REPLACE INTO timeseries (timestamp, metric_name, value) VALUES (?1, ?2, ?3)",
                params![ts, "link_speed", wifi.link_speed_mbps as f64],
            )?;
        }

        if let Some(loopback) = snapshot.latency.loopback_latency_ms {
            tx.execute(
                "INSERT OR REPLACE INTO timeseries (timestamp, metric_name, value) VALUES (?1, ?2, ?3)",
                params![ts, "latency_loopback", loopback],
            )?;
        }
        if let Some(router) = snapshot.latency.router_latency_ms {
            tx.execute(
                "INSERT OR REPLACE INTO timeseries (timestamp, metric_name, value) VALUES (?1, ?2, ?3)",
                params![ts, "latency_router", router],
            )?;
        }
        if let Some(avg) = snapshot.latency.average_latency_ms {
            tx.execute(
                "INSERT OR REPLACE INTO timeseries (timestamp, metric_name, value) VALUES (?1, ?2, ?3)",
                params![ts, "latency_avg", avg],
            )?;
        }
        if let Some(min) = snapshot.latency.min_latency_ms {
            tx.execute(
                "INSERT OR REPLACE INTO timeseries (timestamp, metric_name, value) VALUES (?1, ?2, ?3)",
                params![ts, "latency_min", min],
            )?;
        }
        if let Some(max) = snapshot.latency.max_latency_ms {
            tx.execute(
                "INSERT OR REPLACE INTO timeseries (timestamp, metric_name, value) VALUES (?1, ?2, ?3)",
                params![ts, "latency_max", max],
            )?;
        }
        if let Some(jitter) = snapshot.latency.jitter_ms {
            tx.execute(
                "INSERT OR REPLACE INTO timeseries (timestamp, metric_name, value) VALUES (?1, ?2, ?3)",
                params![ts, "jitter", jitter],
            )?;
        }
        tx.execute(
            "INSERT OR REPLACE INTO timeseries (timestamp, metric_name, value) VALUES (?1, ?2, ?3)",
            params![ts, "packet_loss", snapshot.latency.packet_loss_percent],
        )?;

        tx.execute(
            "INSERT OR REPLACE INTO timeseries (timestamp, metric_name, value) VALUES (?1, ?2, ?3)",
            params![ts, "connected", if snapshot.connectivity.is_connected { 1.0 } else { 0.0 }],
        )?;
        tx.execute(
            "INSERT OR REPLACE INTO timeseries (timestamp, metric_name, value) VALUES (?1, ?2, ?3)",
            params![ts, "loopback_reachable", if snapshot.connectivity.loopback_reachable { 1.0 } else { 0.0 }],
        )?;
        tx.execute(
            "INSERT OR REPLACE INTO timeseries (timestamp, metric_name, value) VALUES (?1, ?2, ?3)",
            params![ts, "router_reachable", if snapshot.connectivity.router_reachable { 1.0 } else { 0.0 }],
        )?;
        tx.execute(
            "INSERT OR REPLACE INTO timeseries (timestamp, metric_name, value) VALUES (?1, ?2, ?3)",
            params![ts, "internet_reachable", if snapshot.connectivity.internet_reachable { 1.0 } else { 0.0 }],
        )?;

        if let Some(http_time) = snapshot.connectivity.http_response_time_ms {
            tx.execute(
                "INSERT OR REPLACE INTO timeseries (timestamp, metric_name, value) VALUES (?1, ?2, ?3)",
                params![ts, "http_response_time", http_time as f64],
            )?;
        }

        if let Some(dns_time) = snapshot.dns_metrics.average_resolution_time_ms {
            tx.execute(
                "INSERT OR REPLACE INTO timeseries (timestamp, metric_name, value) VALUES (?1, ?2, ?3)",
                params![ts, "dns_resolution_time", dns_time],
            )?;
        }

        tx.execute(
            "INSERT OR REPLACE INTO timeseries (timestamp, metric_name, value) VALUES (?1, ?2, ?3)",
            params![ts, "cpu_usage", snapshot.system_info.cpu_usage_percent as f64],
        )?;
        tx.execute(
            "INSERT OR REPLACE INTO timeseries (timestamp, metric_name, value) VALUES (?1, ?2, ?3)",
            params![ts, "memory_usage", snapshot.system_info.memory_usage_percent as f64],
        )?;

        tx.commit()?;
        debug!("Saved snapshot {}", snapshot.id);
        Ok(())
    }

    pub fn get_snapshots(&self, start: Option<&str>, end: Option<&str>, limit: Option<u32>) -> anyhow::Result<Vec<WifiSnapshot>> {
        let mut query = String::from("SELECT data FROM snapshots WHERE 1=1");
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(s) = start {
            query.push_str(" AND timestamp >= ?");
            params_vec.push(Box::new(s.to_string()));
        }
        if let Some(e) = end {
            query.push_str(" AND timestamp <= ?");
            params_vec.push(Box::new(e.to_string()));
        }

        query.push_str(" ORDER BY timestamp DESC");

        if let Some(l) = limit {
            query.push_str(&format!(" LIMIT {}", l));
        }

        let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
        
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(&query)?;
        let rows = stmt.query_map(params_refs.as_slice(), |row| {
            let data: String = row.get(0)?;
            Ok(data)
        })?;

        let mut snapshots = Vec::new();
        for row in rows {
            if let Ok(data) = row {
                if let Ok(snapshot) = serde_json::from_str::<WifiSnapshot>(&data) {
                    snapshots.push(snapshot);
                }
            }
        }

        Ok(snapshots)
    }

    pub fn get_latest_snapshot(&self) -> anyhow::Result<Option<WifiSnapshot>> {
        let snapshots = self.get_snapshots(None, None, Some(1))?;
        Ok(snapshots.into_iter().next())
    }

    pub fn get_timeseries(&self, metric: &str, start: Option<&str>, end: Option<&str>) -> anyhow::Result<Vec<(String, f64)>> {
        let mut query = String::from(
            "SELECT timestamp, value FROM timeseries WHERE metric_name = ?"
        );
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(metric.to_string())];

        if let Some(s) = start {
            query.push_str(" AND timestamp >= ?");
            params_vec.push(Box::new(s.to_string()));
        }
        if let Some(e) = end {
            query.push_str(" AND timestamp <= ?");
            params_vec.push(Box::new(e.to_string()));
        }

        query.push_str(" ORDER BY timestamp ASC");

        let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();

        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(&query)?;
        let rows = stmt.query_map(params_refs.as_slice(), |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
        })?;

        let mut data = Vec::new();
        for row in rows {
            if let Ok(point) = row {
                data.push(point);
            }
        }

        Ok(data)
    }

    pub fn get_events(&self, start: Option<&str>, end: Option<&str>, severity: Option<&str>, event_type: Option<&str>) -> anyhow::Result<Vec<NetworkEvent>> {
        let mut query = String::from(
            "SELECT id, timestamp, event_type, severity, description, details FROM events WHERE 1=1"
        );
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(s) = start {
            query.push_str(" AND timestamp >= ?");
            params_vec.push(Box::new(s.to_string()));
        }
        if let Some(e) = end {
            query.push_str(" AND timestamp <= ?");
            params_vec.push(Box::new(e.to_string()));
        }
        if let Some(sev) = severity {
            query.push_str(" AND severity = ?");
            params_vec.push(Box::new(sev.to_string()));
        }
        if let Some(et) = event_type {
            query.push_str(" AND event_type = ?");
            params_vec.push(Box::new(et.to_string()));
        }

        query.push_str(" ORDER BY timestamp DESC LIMIT 1000");

        let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();

        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(&query)?;
        let rows = stmt.query_map(params_refs.as_slice(), |row| {
            let event_type_str: String = row.get(2)?;
            let severity_str: String = row.get(3)?;
            let details_str: String = row.get(5)?;

            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                event_type_str,
                severity_str,
                row.get::<_, String>(4)?,
                details_str,
            ))
        })?;

        let mut events = Vec::new();
        for row in rows {
            if let Ok((id, timestamp, event_type_str, severity_str, description, details_str)) = row {
                let event_type = parse_event_type(&event_type_str);
                let severity = parse_severity(&severity_str);
                let details: serde_json::Value = serde_json::from_str(&details_str).unwrap_or(serde_json::Value::Null);
                let timestamp = DateTime::parse_from_rfc3339(&timestamp)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now());

                events.push(NetworkEvent {
                    id,
                    timestamp,
                    event_type,
                    severity,
                    description,
                    details,
                });
            }
        }

        Ok(events)
    }

    pub fn get_statistics(&self, start: Option<&str>, end: Option<&str>) -> anyhow::Result<PeriodStatistics> {
        let snapshots = self.get_snapshots(start, end, None)?;
        
        if snapshots.is_empty() {
            return Ok(PeriodStatistics {
                start_time: Utc::now(),
                end_time: Utc::now(),
                sample_count: 0,
                signal_strength_avg_dbm: None,
                signal_strength_min_dbm: None,
                signal_strength_max_dbm: None,
                signal_quality_avg_percent: None,
                latency_avg_ms: None,
                latency_min_ms: None,
                latency_max_ms: None,
                latency_p95_ms: None,
                latency_p99_ms: None,
                jitter_avg_ms: None,
                packet_loss_avg_percent: 0.0,
                connection_uptime_percent: 0.0,
                internet_uptime_percent: 0.0,
                total_disconnections: 0,
                warning_events: 0,
                error_events: 0,
                critical_events: 0,
            });
        }

        let mut signal_values: Vec<i32> = Vec::new();
        let mut quality_values: Vec<u8> = Vec::new();
        let mut latency_values: Vec<f64> = Vec::new();
        let mut jitter_values: Vec<f64> = Vec::new();
        let mut packet_loss_values: Vec<f64> = Vec::new();
        let mut connected_count = 0u32;
        let mut internet_count = 0u32;
        let mut disconnections = 0u32;
        let mut warning_events = 0u32;
        let mut error_events = 0u32;
        let mut critical_events = 0u32;
        let mut was_connected = true;

        for snapshot in &snapshots {
            if let Some(ref wifi) = snapshot.wifi_info {
                signal_values.push(wifi.signal_strength_dbm);
                quality_values.push(wifi.signal_quality_percent);
                connected_count += 1;
                
                if !was_connected {
                    // Was disconnected, now connected - this is a reconnection after disconnection
                }
                was_connected = true;
            } else {
                if was_connected {
                    disconnections += 1;
                }
                was_connected = false;
            }

            if snapshot.connectivity.internet_reachable {
                internet_count += 1;
            }

            if let Some(avg) = snapshot.latency.average_latency_ms {
                latency_values.push(avg);
            }
            if let Some(jitter) = snapshot.latency.jitter_ms {
                jitter_values.push(jitter);
            }
            packet_loss_values.push(snapshot.latency.packet_loss_percent);

            for event in &snapshot.events {
                match event.severity {
                    EventSeverity::Warning => warning_events += 1,
                    EventSeverity::Error => error_events += 1,
                    EventSeverity::Critical => critical_events += 1,
                    _ => {}
                }
            }
        }

        let sample_count = snapshots.len() as u32;

        // Calculate statistics
        let signal_strength_avg_dbm = if !signal_values.is_empty() {
            Some(signal_values.iter().map(|&v| v as f64).sum::<f64>() / signal_values.len() as f64)
        } else {
            None
        };

        let signal_strength_min_dbm = signal_values.iter().min().cloned();
        let signal_strength_max_dbm = signal_values.iter().max().cloned();

        let signal_quality_avg_percent = if !quality_values.is_empty() {
            Some(quality_values.iter().map(|&v| v as f64).sum::<f64>() / quality_values.len() as f64)
        } else {
            None
        };

        latency_values.sort_by(|a, b| a.partial_cmp(b).unwrap());
        
        let latency_avg_ms = if !latency_values.is_empty() {
            Some(latency_values.iter().sum::<f64>() / latency_values.len() as f64)
        } else {
            None
        };
        let latency_min_ms = latency_values.first().cloned();
        let latency_max_ms = latency_values.last().cloned();
        let latency_p95_ms = if !latency_values.is_empty() {
            let idx = (latency_values.len() as f64 * 0.95) as usize;
            latency_values.get(idx.min(latency_values.len() - 1)).cloned()
        } else {
            None
        };
        let latency_p99_ms = if !latency_values.is_empty() {
            let idx = (latency_values.len() as f64 * 0.99) as usize;
            latency_values.get(idx.min(latency_values.len() - 1)).cloned()
        } else {
            None
        };

        let jitter_avg_ms = if !jitter_values.is_empty() {
            Some(jitter_values.iter().sum::<f64>() / jitter_values.len() as f64)
        } else {
            None
        };

        let packet_loss_avg_percent = if !packet_loss_values.is_empty() {
            packet_loss_values.iter().sum::<f64>() / packet_loss_values.len() as f64
        } else {
            0.0
        };

        let connection_uptime_percent = (connected_count as f64 / sample_count as f64) * 100.0;
        let internet_uptime_percent = (internet_count as f64 / sample_count as f64) * 100.0;

        Ok(PeriodStatistics {
            start_time: snapshots.last().map(|s| s.timestamp).unwrap_or_else(Utc::now),
            end_time: snapshots.first().map(|s| s.timestamp).unwrap_or_else(Utc::now),
            sample_count,
            signal_strength_avg_dbm,
            signal_strength_min_dbm,
            signal_strength_max_dbm,
            signal_quality_avg_percent,
            latency_avg_ms,
            latency_min_ms,
            latency_max_ms,
            latency_p95_ms,
            latency_p99_ms,
            jitter_avg_ms,
            packet_loss_avg_percent,
            connection_uptime_percent,
            internet_uptime_percent,
            total_disconnections: disconnections,
            warning_events,
            error_events,
            critical_events,
        })
    }

    pub fn export_json(&self, start: Option<&str>, end: Option<&str>) -> anyhow::Result<String> {
        let snapshots = self.get_snapshots(start, end, None)?;
        let events = self.get_events(start, end, None, None)?;
        let stats = self.get_statistics(start, end)?;

        let export = serde_json::json!({
            "exported_at": Utc::now().to_rfc3339(),
            "statistics": stats,
            "events": events,
            "snapshots": snapshots,
        });

        Ok(serde_json::to_string_pretty(&export)?)
    }

    pub fn get_event_counts_by_type(&self, start: Option<&str>, end: Option<&str>) -> anyhow::Result<Vec<(String, i64)>> {
        let mut query = String::from(
            "SELECT event_type, COUNT(*) as count FROM events WHERE 1=1"
        );
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(s) = start {
            query.push_str(" AND timestamp >= ?");
            params_vec.push(Box::new(s.to_string()));
        }
        if let Some(e) = end {
            query.push_str(" AND timestamp <= ?");
            params_vec.push(Box::new(e.to_string()));
        }

        query.push_str(" GROUP BY event_type ORDER BY count DESC");

        let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();

        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(&query)?;
        let rows = stmt.query_map(params_refs.as_slice(), |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;

        let mut counts = Vec::new();
        for row in rows {
            if let Ok(count) = row {
                counts.push(count);
            }
        }

        Ok(counts)
    }
}

fn parse_event_type(s: &str) -> EventType {
    match s {
        "ConnectionDropped" => EventType::ConnectionDropped,
        "ConnectionRestored" => EventType::ConnectionRestored,
        "SignalStrengthLow" => EventType::SignalStrengthLow,
        "SignalStrengthRecovered" => EventType::SignalStrengthRecovered,
        "HighLatency" => EventType::HighLatency,
        "LatencyNormalized" => EventType::LatencyNormalized,
        "PacketLoss" => EventType::PacketLoss,
        "DnsFailure" => EventType::DnsFailure,
        "DnsRecovered" => EventType::DnsRecovered,
        "BandSwitch" => EventType::BandSwitch,
        "ChannelChange" => EventType::ChannelChange,
        "BssidChange" => EventType::BssidChange,
        "IpAddressChange" => EventType::IpAddressChange,
        "GatewayUnreachable" => EventType::GatewayUnreachable,
        "InternetUnreachable" => EventType::InternetUnreachable,
        "HighJitter" => EventType::HighJitter,
        "AdapterReset" => EventType::AdapterReset,
        "SpeedDegraded" => EventType::SpeedDegraded,
        "SpeedRecovered" => EventType::SpeedRecovered,
        _ => EventType::ConnectionDropped,
    }
}

fn parse_severity(s: &str) -> EventSeverity {
    match s {
        "Info" => EventSeverity::Info,
        "Warning" => EventSeverity::Warning,
        "Error" => EventSeverity::Error,
        "Critical" => EventSeverity::Critical,
        _ => EventSeverity::Info,
    }
}
