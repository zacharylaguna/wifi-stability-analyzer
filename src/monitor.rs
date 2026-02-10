use crate::metrics::*;
use crate::storage::MetricsStore;
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time;
use tracing::{debug, error, info, warn};
use sysinfo::{Networks, System};

pub struct WifiMonitor {
    store: Arc<MetricsStore>,
    interval_secs: u64,
    ping_targets: Vec<String>,
    dns_servers: Vec<String>,
    thresholds: AlertThresholds,
    last_state: Option<MonitorState>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct MonitorState {
    was_connected: bool,
    last_ssid: Option<String>,
    last_bssid: Option<String>,
    last_channel: Option<u32>,
    last_band: Option<WifiBand>,
    last_signal_dbm: Option<i32>,
    last_ip: Option<String>,
    internet_was_reachable: bool,
}

impl WifiMonitor {
    pub fn new(
        store: Arc<MetricsStore>,
        interval_secs: u64,
        ping_targets: Vec<String>,
        dns_servers: Vec<String>,
    ) -> Self {
        Self {
            store,
            interval_secs,
            ping_targets,
            dns_servers,
            thresholds: AlertThresholds::default(),
            last_state: None,
        }
    }

    pub async fn start(mut self) {
        info!("Starting WiFi monitoring with {}s interval", self.interval_secs);
        let mut interval = time::interval(Duration::from_secs(self.interval_secs));

        loop {
            interval.tick().await;
            
            match self.collect_snapshot().await {
                Ok(snapshot) => {
                    // Log summary
                    self.log_snapshot_summary(&snapshot);
                    
                    // Store the snapshot
                    if let Err(e) = self.store.save_snapshot(&snapshot) {
                        error!("Failed to save snapshot: {}", e);
                    }
                    
                    // Update state for next iteration
                    self.update_state(&snapshot);
                }
                Err(e) => {
                    error!("Failed to collect snapshot: {}", e);
                }
            }
        }
    }

    async fn collect_snapshot(&self) -> anyhow::Result<WifiSnapshot> {
        let mut snapshot = WifiSnapshot::new();
        let mut events = Vec::new();

        // Collect WiFi information
        snapshot.wifi_info = self.collect_wifi_info(&mut events).await;

        // Collect system network stats
        snapshot.system_info = self.collect_system_info();

        // Test connectivity (pass gateway if available)
        let gateway = snapshot.wifi_info.as_ref().and_then(|w| w.gateway.as_deref());
        snapshot.connectivity = self.test_connectivity(gateway).await;

        // Measure latency (pass gateway for router latency)
        snapshot.latency = self.measure_latency(gateway).await;

        // Test DNS
        snapshot.dns_metrics = self.test_dns().await;

        // Detect events based on state changes and thresholds
        self.detect_events(&snapshot, &mut events);

        snapshot.events = events;
        Ok(snapshot)
    }

    async fn collect_wifi_info(&self, events: &mut Vec<NetworkEvent>) -> Option<WifiInfo> {
        // Use netsh to get WiFi information on Windows
        let output = Command::new("netsh")
            .args(["wlan", "show", "interfaces"])
            .output();

        match output {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                self.parse_netsh_output(&stdout, events)
            }
            Err(e) => {
                error!("Failed to run netsh: {}", e);
                None
            }
        }
    }

    fn parse_netsh_output(&self, output: &str, events: &mut Vec<NetworkEvent>) -> Option<WifiInfo> {
        let mut wifi_info = WifiInfo {
            ssid: String::new(),
            bssid: String::new(),
            signal_strength_dbm: 0,
            signal_quality_percent: 0,
            channel: 0,
            frequency_mhz: 0,
            band: WifiBand::Unknown,
            phy_type: String::new(),
            link_speed_mbps: 0,
            rx_rate_mbps: None,
            tx_rate_mbps: None,
            security_type: String::new(),
            adapter_name: String::new(),
            adapter_mac: String::new(),
            ipv4_address: None,
            ipv6_address: None,
            gateway: None,
            dns_servers: Vec::new(),
        };

        let mut is_connected = false;

        for line in output.lines() {
            let line = line.trim();
            if let Some((key, value)) = line.split_once(':') {
                let key = key.trim().to_lowercase();
                let value = value.trim();

                match key.as_str() {
                    "name" => wifi_info.adapter_name = value.to_string(),
                    "state" => is_connected = value.to_lowercase() == "connected",
                    "ssid" => wifi_info.ssid = value.to_string(),
                    "bssid" => wifi_info.bssid = value.to_string(),
                    "network type" | "radio type" => wifi_info.phy_type = value.to_string(),
                    "authentication" => wifi_info.security_type = value.to_string(),
                    "channel" => {
                        wifi_info.channel = value.parse().unwrap_or(0);
                        // Estimate frequency from channel
                        wifi_info.frequency_mhz = channel_to_frequency(wifi_info.channel);
                        wifi_info.band = WifiBand::from_frequency(wifi_info.frequency_mhz);
                    }
                    "receive rate (mbps)" => {
                        wifi_info.rx_rate_mbps = value.parse().ok();
                        if wifi_info.link_speed_mbps == 0 {
                            wifi_info.link_speed_mbps = value.parse().unwrap_or(0);
                        }
                    }
                    "transmit rate (mbps)" => {
                        wifi_info.tx_rate_mbps = value.parse().ok();
                    }
                    "signal" => {
                        // Signal is reported as percentage
                        let percent_str = value.trim_end_matches('%');
                        if let Ok(percent) = percent_str.parse::<u8>() {
                            wifi_info.signal_quality_percent = percent;
                            // Convert percentage to approximate dBm
                            // Windows reports quality as 0-100%, roughly maps to -100 to -30 dBm
                            wifi_info.signal_strength_dbm = quality_to_dbm(percent);
                        }
                    }
                    "physical address" => wifi_info.adapter_mac = value.to_string(),
                    _ => {}
                }
            }
        }

        if !is_connected {
            events.push(NetworkEvent::new(
                EventType::ConnectionDropped,
                EventSeverity::Critical,
                "WiFi is not connected",
            ));
            return None;
        }

        // Get IP configuration
        if let Ok(output) = Command::new("ipconfig").output() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            self.parse_ipconfig(&stdout, &mut wifi_info);
        }

        // Check for state changes
        if let Some(ref last_state) = self.last_state {
            if last_state.last_bssid.as_ref() != Some(&wifi_info.bssid) && last_state.last_bssid.is_some() {
                events.push(NetworkEvent::new(
                    EventType::BssidChange,
                    EventSeverity::Warning,
                    &format!("BSSID changed from {:?} to {}", last_state.last_bssid, wifi_info.bssid),
                ).with_details(serde_json::json!({
                    "old_bssid": last_state.last_bssid,
                    "new_bssid": wifi_info.bssid
                })));
            }

            if last_state.last_channel.as_ref() != Some(&wifi_info.channel) && last_state.last_channel.is_some() {
                events.push(NetworkEvent::new(
                    EventType::ChannelChange,
                    EventSeverity::Info,
                    &format!("Channel changed from {:?} to {}", last_state.last_channel, wifi_info.channel),
                ).with_details(serde_json::json!({
                    "old_channel": last_state.last_channel,
                    "new_channel": wifi_info.channel
                })));
            }

            if last_state.last_band.as_ref() != Some(&wifi_info.band) && last_state.last_band.is_some() {
                events.push(NetworkEvent::new(
                    EventType::BandSwitch,
                    EventSeverity::Warning,
                    &format!("Band switched from {:?} to {:?}", last_state.last_band, wifi_info.band),
                ).with_details(serde_json::json!({
                    "old_band": format!("{:?}", last_state.last_band),
                    "new_band": format!("{:?}", wifi_info.band)
                })));
            }
        }

        Some(wifi_info)
    }

    fn parse_ipconfig(&self, output: &str, wifi_info: &mut WifiInfo) {
        let mut in_wifi_section = false;
        
        for line in output.lines() {
            let line_lower = line.to_lowercase();
            
            // Check if we're entering the WiFi adapter section
            if line_lower.contains("wireless") || line_lower.contains("wi-fi") || line_lower.contains("wlan") {
                in_wifi_section = true;
                continue;
            }
            
            // Check if we're leaving the section (new adapter starts)
            if !line.starts_with(' ') && !line.is_empty() && in_wifi_section && !line.contains(':') {
                in_wifi_section = false;
            }
            
            if in_wifi_section {
                if let Some((key, value)) = line.split_once(':') {
                    let key = key.trim().to_lowercase();
                    let value = value.trim();
                    
                    if key.contains("ipv4") {
                        wifi_info.ipv4_address = Some(value.to_string());
                    } else if key.contains("ipv6") && wifi_info.ipv6_address.is_none() {
                        wifi_info.ipv6_address = Some(value.to_string());
                    } else if key.contains("default gateway") && !value.is_empty() {
                        wifi_info.gateway = Some(value.to_string());
                    } else if key.contains("dns") {
                        wifi_info.dns_servers.push(value.to_string());
                    }
                }
            }
        }
    }

    fn collect_system_info(&self) -> SystemNetworkInfo {
        let mut sys = System::new_all();
        sys.refresh_all();

        let networks = Networks::new_with_refreshed_list();
        
        let mut info = SystemNetworkInfo::default();
        
        for (_interface_name, data) in &networks {
            // Aggregate all network interface stats
            info.bytes_sent += data.total_transmitted();
            info.bytes_received += data.total_received();
            info.packets_sent += data.total_packets_transmitted();
            info.packets_received += data.total_packets_received();
            info.errors_in += data.total_errors_on_received();
            info.errors_out += data.total_errors_on_transmitted();
        }

        info.cpu_usage_percent = sys.global_cpu_info().cpu_usage();
        info.memory_usage_percent = (sys.used_memory() as f32 / sys.total_memory() as f32) * 100.0;

        info
    }

    async fn test_connectivity(&self, gateway: Option<&str>) -> ConnectivityMetrics {
        let mut metrics = ConnectivityMetrics::default();

        // Check if we have a WiFi connection
        let output = Command::new("netsh")
            .args(["wlan", "show", "interfaces"])
            .output();

        if let Ok(output) = output {
            let stdout = String::from_utf8_lossy(&output.stdout);
            metrics.is_connected = stdout.to_lowercase().contains("state") 
                && stdout.to_lowercase().contains("connected");
        }

        // Test loopback (127.0.0.1) - verifies network stack is working
        let loopback_ping = self.ping_target("127.0.0.1", 2).await;
        metrics.loopback_reachable = loopback_ping.packets_received > 0;
        debug!("Loopback ping: {} packets received", loopback_ping.packets_received);

        // Test router/gateway connectivity (local network)
        if let Some(gw) = gateway {
            let router_ping = self.ping_target(gw, 2).await;
            metrics.router_reachable = router_ping.packets_received > 0;
            debug!("Router ping: {} packets received from {}", router_ping.packets_received, gw);
        } else {
            // If no gateway, assume router is reachable if WiFi is connected
            metrics.router_reachable = metrics.is_connected;
        }

        // Test HTTP connectivity (internet)
        let start = Instant::now();
        match reqwest::get("http://www.gstatic.com/generate_204").await {
            Ok(response) => {
                metrics.http_test_success = response.status().is_success() || response.status().as_u16() == 204;
                metrics.http_response_time_ms = Some(start.elapsed().as_millis() as u64);
                metrics.internet_reachable = metrics.http_test_success;
            }
            Err(e) => {
                debug!("HTTP connectivity test failed: {}", e);
                metrics.http_test_success = false;
                metrics.internet_reachable = false;
            }
        }

        metrics
    }

    async fn measure_latency(&self, gateway: Option<&str>) -> LatencyMetrics {
        let mut metrics = LatencyMetrics::default();
        let mut all_times: Vec<f64> = Vec::new();
        let mut total_sent = 0u32;
        let mut total_received = 0u32;

        // Measure loopback latency
        let loopback_result = self.ping_target("127.0.0.1", 4).await;
        if let Some(avg) = loopback_result.avg_ms {
            metrics.loopback_latency_ms = Some(avg);
        }

        // Measure router latency
        if let Some(gw) = gateway {
            let router_result = self.ping_target(gw, 4).await;
            if let Some(avg) = router_result.avg_ms {
                metrics.router_latency_ms = Some(avg);
            }
        }

        for target in &self.ping_targets {
            let result = self.ping_target(target, 4).await;
            
            if !result.individual_times_ms.is_empty() {
                all_times.extend(result.individual_times_ms.iter().cloned());
            }
            
            total_sent += result.packets_sent;
            total_received += result.packets_received;
            
            metrics.targets.push(result);
        }

        if !all_times.is_empty() {
            all_times.sort_by(|a, b| a.partial_cmp(b).unwrap());
            
            metrics.min_latency_ms = all_times.first().cloned();
            metrics.max_latency_ms = all_times.last().cloned();
            metrics.average_latency_ms = Some(all_times.iter().sum::<f64>() / all_times.len() as f64);
            
            // Calculate jitter (average deviation from mean)
            if all_times.len() > 1 {
                let mean = metrics.average_latency_ms.unwrap();
                let variance: f64 = all_times.iter()
                    .map(|t| (t - mean).powi(2))
                    .sum::<f64>() / all_times.len() as f64;
                metrics.jitter_ms = Some(variance.sqrt());
            }
        }

        if total_sent > 0 {
            metrics.packet_loss_percent = ((total_sent - total_received) as f64 / total_sent as f64) * 100.0;
        }

        metrics
    }

    async fn ping_target(&self, target: &str, count: u32) -> PingResult {
        let mut result = PingResult {
            target: target.to_string(),
            resolved_ip: None,
            packets_sent: count,
            packets_received: 0,
            packet_loss_percent: 100.0,
            min_ms: None,
            avg_ms: None,
            max_ms: None,
            stddev_ms: None,
            individual_times_ms: Vec::new(),
            error: None,
        };

        // Use Windows ping command
        let output = Command::new("ping")
            .args(["-n", &count.to_string(), target])
            .output();

        match output {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                self.parse_ping_output(&stdout, &mut result);
            }
            Err(e) => {
                result.error = Some(format!("Failed to execute ping: {}", e));
            }
        }

        result
    }

    fn parse_ping_output(&self, output: &str, result: &mut PingResult) {
        let mut times = Vec::new();
        
        for line in output.lines() {
            let line_lower = line.to_lowercase();
            
            // Parse individual ping times
            if line_lower.contains("time=") || line_lower.contains("time<") {
                if let Some(time_part) = line.split("time").nth(1) {
                    let time_str: String = time_part
                        .chars()
                        .skip_while(|c| !c.is_ascii_digit())
                        .take_while(|c| c.is_ascii_digit() || *c == '.')
                        .collect();
                    
                    if let Ok(time) = time_str.parse::<f64>() {
                        times.push(time);
                    }
                }
            }
            
            // Parse reply from IP
            if line_lower.contains("reply from") {
                if let Some(ip_part) = line.split("Reply from").nth(1) {
                    let ip: String = ip_part
                        .trim()
                        .chars()
                        .take_while(|c| *c != ':' && *c != ' ')
                        .collect();
                    result.resolved_ip = Some(ip);
                }
            }
            
            // Parse statistics
            if line_lower.contains("packets:") || line_lower.contains("received =") {
                // Windows format: "Packets: Sent = 4, Received = 4, Lost = 0"
                if let Some(recv_part) = line.split("Received").nth(1) {
                    let recv_str: String = recv_part
                        .chars()
                        .skip_while(|c| !c.is_ascii_digit())
                        .take_while(|c| c.is_ascii_digit())
                        .collect();
                    
                    if let Ok(recv) = recv_str.parse::<u32>() {
                        result.packets_received = recv;
                    }
                }
            }
            
            // Parse min/max/avg from statistics line
            if line_lower.contains("minimum") && line_lower.contains("maximum") {
                // Windows format: "Minimum = 10ms, Maximum = 15ms, Average = 12ms"
                for part in line.split(',') {
                    let part_lower = part.to_lowercase();
                    let value: String = part
                        .chars()
                        .filter(|c| c.is_ascii_digit() || *c == '.')
                        .collect();
                    
                    if let Ok(val) = value.parse::<f64>() {
                        if part_lower.contains("minimum") {
                            result.min_ms = Some(val);
                        } else if part_lower.contains("maximum") {
                            result.max_ms = Some(val);
                        } else if part_lower.contains("average") {
                            result.avg_ms = Some(val);
                        }
                    }
                }
            }
        }

        result.individual_times_ms = times;
        
        if result.packets_sent > 0 {
            result.packet_loss_percent = 
                ((result.packets_sent - result.packets_received) as f64 / result.packets_sent as f64) * 100.0;
        }

        // Calculate stddev if we have individual times
        if result.individual_times_ms.len() > 1 {
            let mean = result.individual_times_ms.iter().sum::<f64>() / result.individual_times_ms.len() as f64;
            let variance: f64 = result.individual_times_ms.iter()
                .map(|t| (t - mean).powi(2))
                .sum::<f64>() / result.individual_times_ms.len() as f64;
            result.stddev_ms = Some(variance.sqrt());
        }
    }

    async fn test_dns(&self) -> DnsMetrics {
        let mut metrics = DnsMetrics::default();
        let test_domains = vec!["google.com", "cloudflare.com", "microsoft.com"];
        let mut total_time = 0.0;
        let mut successful_queries = 0;

        for dns_server in &self.dns_servers {
            for domain in &test_domains {
                let result = self.test_dns_query(domain, dns_server).await;
                
                if result.success {
                    if let Some(time) = result.resolution_time_ms {
                        total_time += time;
                        successful_queries += 1;
                    }
                } else {
                    metrics.failures += 1;
                }
                
                metrics.queries.push(result);
            }
        }

        if successful_queries > 0 {
            metrics.average_resolution_time_ms = Some(total_time / successful_queries as f64);
        }

        metrics
    }

    async fn test_dns_query(&self, domain: &str, dns_server: &str) -> DnsQueryResult {
        let start = Instant::now();
        
        // Use nslookup for DNS testing on Windows
        let output = Command::new("nslookup")
            .args([domain, dns_server])
            .output();

        match output {
            Ok(output) => {
                let elapsed = start.elapsed().as_millis() as f64;
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                
                let mut resolved_ips = Vec::new();
                let mut in_answer_section = false;
                
                for line in stdout.lines() {
                    if line.contains("Name:") && line.contains(domain) {
                        in_answer_section = true;
                        continue;
                    }
                    
                    if in_answer_section && line.contains("Address") {
                        if let Some(ip) = line.split(':').nth(1) {
                            let ip = ip.trim();
                            if !ip.is_empty() && !ip.contains(dns_server) {
                                resolved_ips.push(ip.to_string());
                            }
                        }
                    }
                }

                let success = !resolved_ips.is_empty() || output.status.success();
                
                DnsQueryResult {
                    domain: domain.to_string(),
                    dns_server: dns_server.to_string(),
                    resolution_time_ms: Some(elapsed),
                    resolved_ips,
                    success,
                    error: if success { None } else { Some(stderr.to_string()) },
                }
            }
            Err(e) => {
                DnsQueryResult {
                    domain: domain.to_string(),
                    dns_server: dns_server.to_string(),
                    resolution_time_ms: None,
                    resolved_ips: Vec::new(),
                    success: false,
                    error: Some(format!("Failed to execute nslookup: {}", e)),
                }
            }
        }
    }

    fn detect_events(&self, snapshot: &WifiSnapshot, events: &mut Vec<NetworkEvent>) {
        // Check signal strength
        if let Some(ref wifi) = snapshot.wifi_info {
            if wifi.signal_strength_dbm <= self.thresholds.signal_strength_critical_dbm {
                events.push(NetworkEvent::new(
                    EventType::SignalStrengthLow,
                    EventSeverity::Critical,
                    &format!("Critical signal strength: {} dBm ({}%)", 
                        wifi.signal_strength_dbm, wifi.signal_quality_percent),
                ).with_details(serde_json::json!({
                    "signal_dbm": wifi.signal_strength_dbm,
                    "signal_percent": wifi.signal_quality_percent
                })));
            } else if wifi.signal_strength_dbm <= self.thresholds.signal_strength_warning_dbm {
                events.push(NetworkEvent::new(
                    EventType::SignalStrengthLow,
                    EventSeverity::Warning,
                    &format!("Low signal strength: {} dBm ({}%)", 
                        wifi.signal_strength_dbm, wifi.signal_quality_percent),
                ).with_details(serde_json::json!({
                    "signal_dbm": wifi.signal_strength_dbm,
                    "signal_percent": wifi.signal_quality_percent
                })));
            }
        }

        // Check latency
        if let Some(avg_latency) = snapshot.latency.average_latency_ms {
            if avg_latency >= self.thresholds.latency_critical_ms {
                events.push(NetworkEvent::new(
                    EventType::HighLatency,
                    EventSeverity::Critical,
                    &format!("Critical latency: {:.1}ms", avg_latency),
                ).with_details(serde_json::json!({
                    "latency_ms": avg_latency
                })));
            } else if avg_latency >= self.thresholds.latency_warning_ms {
                events.push(NetworkEvent::new(
                    EventType::HighLatency,
                    EventSeverity::Warning,
                    &format!("High latency: {:.1}ms", avg_latency),
                ).with_details(serde_json::json!({
                    "latency_ms": avg_latency
                })));
            }
        }

        // Check jitter
        if let Some(jitter) = snapshot.latency.jitter_ms {
            if jitter >= self.thresholds.jitter_warning_ms {
                events.push(NetworkEvent::new(
                    EventType::HighJitter,
                    EventSeverity::Warning,
                    &format!("High jitter: {:.1}ms", jitter),
                ).with_details(serde_json::json!({
                    "jitter_ms": jitter
                })));
            }
        }

        // Check packet loss
        if snapshot.latency.packet_loss_percent >= self.thresholds.packet_loss_critical_percent {
            events.push(NetworkEvent::new(
                EventType::PacketLoss,
                EventSeverity::Critical,
                &format!("Critical packet loss: {:.1}%", snapshot.latency.packet_loss_percent),
            ).with_details(serde_json::json!({
                "packet_loss_percent": snapshot.latency.packet_loss_percent
            })));
        } else if snapshot.latency.packet_loss_percent >= self.thresholds.packet_loss_warning_percent {
            events.push(NetworkEvent::new(
                EventType::PacketLoss,
                EventSeverity::Warning,
                &format!("Packet loss detected: {:.1}%", snapshot.latency.packet_loss_percent),
            ).with_details(serde_json::json!({
                "packet_loss_percent": snapshot.latency.packet_loss_percent
            })));
        }

        // Check router and internet connectivity
        if snapshot.connectivity.is_connected {
            if !snapshot.connectivity.router_reachable {
                events.push(NetworkEvent::new(
                    EventType::InternetUnreachable,
                    EventSeverity::Critical,
                    "Router/gateway is not reachable (local network issue)",
                ).with_details(serde_json::json!({
                    "issue_type": "router_unreachable"
                })));
            } else if !snapshot.connectivity.internet_reachable {
                events.push(NetworkEvent::new(
                    EventType::InternetUnreachable,
                    EventSeverity::Critical,
                    "Internet is not reachable (router OK, ISP/internet issue)",
                ).with_details(serde_json::json!({
                    "issue_type": "internet_unreachable",
                    "router_reachable": true
                })));
            }
        }

        // Check DNS failures
        if snapshot.dns_metrics.failures > 0 {
            events.push(NetworkEvent::new(
                EventType::DnsFailure,
                EventSeverity::Warning,
                &format!("{} DNS queries failed", snapshot.dns_metrics.failures),
            ).with_details(serde_json::json!({
                "failures": snapshot.dns_metrics.failures
            })));
        }

        // Check for connection restoration
        if let Some(ref last_state) = self.last_state {
            if !last_state.was_connected && snapshot.wifi_info.is_some() {
                events.push(NetworkEvent::new(
                    EventType::ConnectionRestored,
                    EventSeverity::Info,
                    "WiFi connection restored",
                ));
            }

            if !last_state.internet_was_reachable && snapshot.connectivity.internet_reachable {
                events.push(NetworkEvent::new(
                    EventType::ConnectionRestored,
                    EventSeverity::Info,
                    "Internet connectivity restored",
                ));
            }
        }
    }

    fn log_snapshot_summary(&self, snapshot: &WifiSnapshot) {
        if let Some(ref wifi) = snapshot.wifi_info {
            info!(
                ssid = %wifi.ssid,
                signal_dbm = wifi.signal_strength_dbm,
                signal_percent = wifi.signal_quality_percent,
                channel = wifi.channel,
                band = ?wifi.band,
                "WiFi Status"
            );
        } else {
            warn!("WiFi not connected");
        }

        if let Some(avg) = snapshot.latency.average_latency_ms {
            info!(
                avg_ms = format!("{:.1}", avg),
                min_ms = snapshot.latency.min_latency_ms.map(|v| format!("{:.1}", v)),
                max_ms = snapshot.latency.max_latency_ms.map(|v| format!("{:.1}", v)),
                jitter_ms = snapshot.latency.jitter_ms.map(|v| format!("{:.1}", v)),
                packet_loss = format!("{:.1}%", snapshot.latency.packet_loss_percent),
                "Latency"
            );
        }

        info!(
            connected = snapshot.connectivity.is_connected,
            loopback = snapshot.connectivity.loopback_reachable,
            router = snapshot.connectivity.router_reachable,
            internet = snapshot.connectivity.internet_reachable,
            http_time_ms = snapshot.connectivity.http_response_time_ms,
            "Connectivity"
        );

        for event in &snapshot.events {
            match event.severity {
                EventSeverity::Critical => error!(event_type = ?event.event_type, "{}", event.description),
                EventSeverity::Error => error!(event_type = ?event.event_type, "{}", event.description),
                EventSeverity::Warning => warn!(event_type = ?event.event_type, "{}", event.description),
                EventSeverity::Info => info!(event_type = ?event.event_type, "{}", event.description),
            }
        }
    }

    fn update_state(&mut self, snapshot: &WifiSnapshot) {
        self.last_state = Some(MonitorState {
            was_connected: snapshot.wifi_info.is_some(),
            last_ssid: snapshot.wifi_info.as_ref().map(|w| w.ssid.clone()),
            last_bssid: snapshot.wifi_info.as_ref().map(|w| w.bssid.clone()),
            last_channel: snapshot.wifi_info.as_ref().map(|w| w.channel),
            last_band: snapshot.wifi_info.as_ref().map(|w| w.band.clone()),
            last_signal_dbm: snapshot.wifi_info.as_ref().map(|w| w.signal_strength_dbm),
            last_ip: snapshot.wifi_info.as_ref().and_then(|w| w.ipv4_address.clone()),
            internet_was_reachable: snapshot.connectivity.internet_reachable,
        });
    }
}

/// Convert WiFi channel number to frequency in MHz
fn channel_to_frequency(channel: u32) -> u32 {
    match channel {
        // 2.4 GHz band
        1..=13 => 2407 + (channel * 5),
        14 => 2484,
        // 5 GHz band
        36 => 5180,
        40 => 5200,
        44 => 5220,
        48 => 5240,
        52 => 5260,
        56 => 5280,
        60 => 5300,
        64 => 5320,
        100 => 5500,
        104 => 5520,
        108 => 5540,
        112 => 5560,
        116 => 5580,
        120 => 5600,
        124 => 5620,
        128 => 5640,
        132 => 5660,
        136 => 5680,
        140 => 5700,
        144 => 5720,
        149 => 5745,
        153 => 5765,
        157 => 5785,
        161 => 5805,
        165 => 5825,
        // 6 GHz band (simplified)
        1..=233 if channel > 165 => 5950 + (channel * 5),
        _ => 0,
    }
}

/// Convert signal quality percentage to approximate dBm
fn quality_to_dbm(quality: u8) -> i32 {
    // Windows reports quality as 0-100%
    // Roughly maps: 100% = -30 dBm, 0% = -100 dBm
    -100 + ((quality as i32 * 70) / 100)
}
