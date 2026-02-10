use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Represents a single WiFi measurement snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WifiSnapshot {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub wifi_info: Option<WifiInfo>,
    pub connectivity: ConnectivityMetrics,
    pub latency: LatencyMetrics,
    pub dns_metrics: DnsMetrics,
    pub system_info: SystemNetworkInfo,
    pub events: Vec<NetworkEvent>,
}

impl WifiSnapshot {
    pub fn new() -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            wifi_info: None,
            connectivity: ConnectivityMetrics::default(),
            latency: LatencyMetrics::default(),
            dns_metrics: DnsMetrics::default(),
            system_info: SystemNetworkInfo::default(),
            events: Vec::new(),
        }
    }
}

/// WiFi adapter and connection information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WifiInfo {
    pub ssid: String,
    pub bssid: String,
    pub signal_strength_dbm: i32,
    pub signal_quality_percent: u8,
    pub channel: u32,
    pub frequency_mhz: u32,
    pub band: WifiBand,
    pub phy_type: String,
    pub link_speed_mbps: u32,
    pub rx_rate_mbps: Option<u32>,
    pub tx_rate_mbps: Option<u32>,
    pub security_type: String,
    pub adapter_name: String,
    pub adapter_mac: String,
    pub ipv4_address: Option<String>,
    pub ipv6_address: Option<String>,
    pub gateway: Option<String>,
    pub dns_servers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum WifiBand {
    Band2_4GHz,
    Band5GHz,
    Band6GHz,
    Unknown,
}

impl WifiBand {
    pub fn from_frequency(freq_mhz: u32) -> Self {
        match freq_mhz {
            2400..=2500 => WifiBand::Band2_4GHz,
            5150..=5900 => WifiBand::Band5GHz,
            5925..=7125 => WifiBand::Band6GHz,
            _ => WifiBand::Unknown,
        }
    }
}

/// Connectivity test results
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConnectivityMetrics {
    pub is_connected: bool,
    pub loopback_reachable: bool,
    pub router_reachable: bool,
    pub internet_reachable: bool,
    pub http_test_success: bool,
    pub http_response_time_ms: Option<u64>,
    pub tcp_connections_established: u32,
    pub tcp_connections_failed: u32,
}

/// Latency measurements from ping tests
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LatencyMetrics {
    pub targets: Vec<PingResult>,
    pub loopback_latency_ms: Option<f64>,
    pub router_latency_ms: Option<f64>,
    pub average_latency_ms: Option<f64>,
    pub min_latency_ms: Option<f64>,
    pub max_latency_ms: Option<f64>,
    pub jitter_ms: Option<f64>,
    pub packet_loss_percent: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PingResult {
    pub target: String,
    pub resolved_ip: Option<String>,
    pub packets_sent: u32,
    pub packets_received: u32,
    pub packet_loss_percent: f64,
    pub min_ms: Option<f64>,
    pub avg_ms: Option<f64>,
    pub max_ms: Option<f64>,
    pub stddev_ms: Option<f64>,
    pub individual_times_ms: Vec<f64>,
    pub error: Option<String>,
}

/// DNS resolution metrics
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DnsMetrics {
    pub queries: Vec<DnsQueryResult>,
    pub average_resolution_time_ms: Option<f64>,
    pub failures: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsQueryResult {
    pub domain: String,
    pub dns_server: String,
    pub resolution_time_ms: Option<f64>,
    pub resolved_ips: Vec<String>,
    pub success: bool,
    pub error: Option<String>,
}

/// System-level network information
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SystemNetworkInfo {
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub packets_sent: u64,
    pub packets_received: u64,
    pub errors_in: u64,
    pub errors_out: u64,
    pub drops_in: u64,
    pub drops_out: u64,
    pub active_connections: u32,
    pub cpu_usage_percent: f32,
    pub memory_usage_percent: f32,
}

/// Network events that may indicate issues
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkEvent {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub event_type: EventType,
    pub severity: EventSeverity,
    pub description: String,
    pub details: serde_json::Value,
}

impl NetworkEvent {
    pub fn new(event_type: EventType, severity: EventSeverity, description: &str) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            event_type,
            severity,
            description: description.to_string(),
            details: serde_json::Value::Null,
        }
    }

    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = details;
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EventType {
    ConnectionDropped,
    ConnectionRestored,
    SignalStrengthLow,
    SignalStrengthRecovered,
    HighLatency,
    LatencyNormalized,
    PacketLoss,
    DnsFailure,
    DnsRecovered,
    BandSwitch,
    ChannelChange,
    BssidChange,
    IpAddressChange,
    GatewayUnreachable,
    InternetUnreachable,
    HighJitter,
    AdapterReset,
    SpeedDegraded,
    SpeedRecovered,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum EventSeverity {
    Info,
    Warning,
    Error,
    Critical,
}

/// Thresholds for detecting issues
#[derive(Debug, Clone)]
pub struct AlertThresholds {
    pub signal_strength_warning_dbm: i32,
    pub signal_strength_critical_dbm: i32,
    pub latency_warning_ms: f64,
    pub latency_critical_ms: f64,
    pub jitter_warning_ms: f64,
    pub packet_loss_warning_percent: f64,
    pub packet_loss_critical_percent: f64,
}

impl Default for AlertThresholds {
    fn default() -> Self {
        Self {
            signal_strength_warning_dbm: -70,
            signal_strength_critical_dbm: -80,
            latency_warning_ms: 100.0,
            latency_critical_ms: 300.0,
            jitter_warning_ms: 30.0,
            packet_loss_warning_percent: 1.0,
            packet_loss_critical_percent: 5.0,
        }
    }
}

/// Statistics for a time period
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeriodStatistics {
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub sample_count: u32,
    
    // Signal statistics
    pub signal_strength_avg_dbm: Option<f64>,
    pub signal_strength_min_dbm: Option<i32>,
    pub signal_strength_max_dbm: Option<i32>,
    pub signal_quality_avg_percent: Option<f64>,
    
    // Latency statistics
    pub latency_avg_ms: Option<f64>,
    pub latency_min_ms: Option<f64>,
    pub latency_max_ms: Option<f64>,
    pub latency_p95_ms: Option<f64>,
    pub latency_p99_ms: Option<f64>,
    pub jitter_avg_ms: Option<f64>,
    
    // Reliability statistics
    pub packet_loss_avg_percent: f64,
    pub connection_uptime_percent: f64,
    pub internet_uptime_percent: f64,
    pub total_disconnections: u32,
    
    // Event counts
    pub warning_events: u32,
    pub error_events: u32,
    pub critical_events: u32,
}
