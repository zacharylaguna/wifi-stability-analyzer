use crate::storage::MetricsStore;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Json},
    routing::get,
    Router,
};
use serde::Deserialize;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tracing::info;

type SharedStore = Arc<MetricsStore>;

pub async fn start_web_server(store: SharedStore, port: u16) -> anyhow::Result<()> {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/", get(dashboard_handler))
        .route("/api/current", get(current_handler))
        .route("/api/snapshots", get(snapshots_handler))
        .route("/api/timeseries", get(timeseries_handler))
        .route("/api/events", get(events_handler))
        .route("/api/statistics", get(statistics_handler))
        .route("/api/event-counts", get(event_counts_handler))
        .layer(cors)
        .with_state(store);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
    info!("Web server listening on port {}", port);
    axum::serve(listener, app).await?;
    Ok(())
}

async fn dashboard_handler() -> Html<&'static str> {
    Html(DASHBOARD_HTML)
}

#[derive(Deserialize)]
struct TimeRangeQuery {
    start: Option<String>,
    end: Option<String>,
    limit: Option<u32>,
}

#[derive(Deserialize)]
struct TimeseriesQuery {
    metric: String,
    start: Option<String>,
    end: Option<String>,
}

#[derive(Deserialize)]
struct EventsQuery {
    start: Option<String>,
    end: Option<String>,
    severity: Option<String>,
    event_type: Option<String>,
}

async fn current_handler(State(store): State<SharedStore>) -> impl IntoResponse {
    match store.get_latest_snapshot() {
        Ok(Some(snapshot)) => Json(serde_json::json!({
            "success": true,
            "data": snapshot
        })).into_response(),
        Ok(None) => Json(serde_json::json!({
            "success": true,
            "data": null,
            "message": "No data collected yet"
        })).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "success": false,
                "error": e.to_string()
            })),
        ).into_response(),
    }
}

async fn snapshots_handler(
    State(store): State<SharedStore>,
    Query(params): Query<TimeRangeQuery>,
) -> impl IntoResponse {
    match store.get_snapshots(params.start.as_deref(), params.end.as_deref(), params.limit) {
        Ok(snapshots) => Json(serde_json::json!({
            "success": true,
            "count": snapshots.len(),
            "data": snapshots
        })).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "success": false,
                "error": e.to_string()
            })),
        ).into_response(),
    }
}

async fn timeseries_handler(
    State(store): State<SharedStore>,
    Query(params): Query<TimeseriesQuery>,
) -> impl IntoResponse {
    match store.get_timeseries(&params.metric, params.start.as_deref(), params.end.as_deref()) {
        Ok(data) => Json(serde_json::json!({
            "success": true,
            "metric": params.metric,
            "count": data.len(),
            "data": data.into_iter().map(|(ts, val)| {
                serde_json::json!({ "timestamp": ts, "value": val })
            }).collect::<Vec<_>>()
        })).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "success": false,
                "error": e.to_string()
            })),
        ).into_response(),
    }
}

async fn events_handler(
    State(store): State<SharedStore>,
    Query(params): Query<EventsQuery>,
) -> impl IntoResponse {
    match store.get_events(
        params.start.as_deref(),
        params.end.as_deref(),
        params.severity.as_deref(),
        params.event_type.as_deref(),
    ) {
        Ok(events) => Json(serde_json::json!({
            "success": true,
            "count": events.len(),
            "data": events
        })).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "success": false,
                "error": e.to_string()
            })),
        ).into_response(),
    }
}

async fn statistics_handler(
    State(store): State<SharedStore>,
    Query(params): Query<TimeRangeQuery>,
) -> impl IntoResponse {
    match store.get_statistics(params.start.as_deref(), params.end.as_deref()) {
        Ok(stats) => Json(serde_json::json!({
            "success": true,
            "data": stats
        })).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "success": false,
                "error": e.to_string()
            })),
        ).into_response(),
    }
}

async fn event_counts_handler(
    State(store): State<SharedStore>,
    Query(params): Query<TimeRangeQuery>,
) -> impl IntoResponse {
    match store.get_event_counts_by_type(params.start.as_deref(), params.end.as_deref()) {
        Ok(counts) => Json(serde_json::json!({
            "success": true,
            "data": counts.into_iter().map(|(event_type, count)| {
                serde_json::json!({ "event_type": event_type, "count": count })
            }).collect::<Vec<_>>()
        })).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "success": false,
                "error": e.to_string()
            })),
        ).into_response(),
    }
}

const DASHBOARD_HTML: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>WiFi Stability Tracker - Dashboard</title>
    <script src="https://cdn.jsdelivr.net/npm/chart.js"></script>
    <script src="https://cdn.jsdelivr.net/npm/chartjs-adapter-date-fns"></script>
    <script src="https://cdn.tailwindcss.com"></script>
    <style>
        .status-good { color: #10b981; }
        .status-warning { color: #f59e0b; }
        .status-critical { color: #ef4444; }
        .severity-info { background-color: #3b82f6; }
        .severity-warning { background-color: #f59e0b; }
        .severity-error { background-color: #f97316; }
        .severity-critical { background-color: #ef4444; }
        .chart-container { position: relative; height: 250px; }
        .log-entry { font-family: 'Consolas', 'Monaco', monospace; font-size: 12px; }
    </style>
</head>
<body class="bg-gray-900 text-gray-100 min-h-screen">
    <div class="container mx-auto px-4 py-6">
        <!-- Header -->
        <header class="mb-8">
            <div class="flex justify-between items-start">
                <div>
                    <h1 class="text-3xl font-bold text-white mb-2">WiFi Stability Tracker</h1>
                    <p class="text-gray-400">Real-time monitoring and analysis dashboard</p>
                </div>
                <div class="bg-gray-800 rounded-lg p-4 border border-gray-700">
                    <label class="text-gray-400 text-sm font-medium mb-2 block">Time Range</label>
                    <select id="time-range" class="bg-gray-700 border border-gray-600 rounded px-4 py-2 text-sm min-w-[200px]">
                        <option value="5">Last 5 minutes</option>
                        <option value="15">Last 15 minutes</option>
                        <option value="30">Last 30 minutes</option>
                        <option value="60" selected>Last 1 hour</option>
                        <option value="180">Last 3 hours</option>
                        <option value="360">Last 6 hours</option>
                        <option value="720">Last 12 hours</option>
                        <option value="1440">Last 24 hours</option>
                        <option value="4320">Last 3 days</option>
                        <option value="10080">Last 7 days</option>
                        <option value="custom">Custom Range</option>
                    </select>
                    <div id="custom-range" class="mt-3 space-y-2 hidden">
                        <input type="datetime-local" id="start-time" class="bg-gray-700 border border-gray-600 rounded px-3 py-1 text-sm w-full">
                        <input type="datetime-local" id="end-time" class="bg-gray-700 border border-gray-600 rounded px-3 py-1 text-sm w-full">
                        <button onclick="applyCustomRange()" class="bg-blue-600 hover:bg-blue-700 px-3 py-1 rounded text-sm w-full">Apply</button>
                    </div>
                </div>
            </div>
        </header>

        <!-- Current Status Cards -->
        <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4 mb-8">
            <div class="bg-gray-800 rounded-lg p-4 border border-gray-700">
                <h3 class="text-gray-400 text-sm font-medium mb-1">Signal Strength</h3>
                <div class="flex items-baseline">
                    <span id="signal-value" class="text-2xl font-bold">--</span>
                    <span class="text-gray-500 ml-1">dBm</span>
                </div>
                <div class="mt-2 h-2 bg-gray-700 rounded-full overflow-hidden">
                    <div id="signal-bar" class="h-full bg-green-500 transition-all duration-500" style="width: 0%"></div>
                </div>
                <p id="signal-quality" class="text-gray-500 text-sm mt-1">--% quality</p>
            </div>

            <div class="bg-gray-800 rounded-lg p-4 border border-gray-700">
                <h3 class="text-gray-400 text-sm font-medium mb-1">Latency</h3>
                <div class="flex items-baseline">
                    <span id="latency-value" class="text-2xl font-bold">--</span>
                    <span class="text-gray-500 ml-1">ms</span>
                </div>
                <p id="latency-range" class="text-gray-500 text-sm mt-2">Min: -- / Max: --</p>
                <p id="jitter-value" class="text-gray-500 text-sm">Jitter: -- ms</p>
            </div>

            <div class="bg-gray-800 rounded-lg p-4 border border-gray-700">
                <h3 class="text-gray-400 text-sm font-medium mb-1">Connectivity</h3>
                <p id="loopback-status" class="text-sm mt-1">Loopback: <span class="font-semibold">--</span></p>
                <p id="router-status" class="text-sm mt-1">Router: <span class="font-semibold">--</span></p>
                <p id="internet-status" class="text-sm mt-1">Internet: <span class="font-semibold">--</span></p>
                <p id="connection-status" class="text-gray-500 text-xs mt-2">WiFi: <span class="font-semibold">--</span></p>
            </div>

            <div class="bg-gray-800 rounded-lg p-4 border border-gray-700">
                <h3 class="text-gray-400 text-sm font-medium mb-1">Network Info</h3>
                <p id="ssid-value" class="text-lg font-semibold truncate">--</p>
                <p id="channel-value" class="text-gray-500 text-sm mt-1">Channel: -- (--)</p>
                <p id="speed-value" class="text-gray-500 text-sm">Speed: -- Mbps</p>
            </div>
        </div>

        <!-- Statistics Summary -->
        <div class="bg-gray-800 rounded-lg p-4 border border-gray-700 mb-8">
            <h2 class="text-xl font-semibold mb-4">Session Statistics</h2>
            <div class="grid grid-cols-2 md:grid-cols-4 lg:grid-cols-6 gap-4">
                <div>
                    <p class="text-gray-400 text-sm">Samples</p>
                    <p id="stat-samples" class="text-lg font-semibold">--</p>
                </div>
                <div>
                    <p class="text-gray-400 text-sm">Uptime</p>
                    <p id="stat-uptime" class="text-lg font-semibold">--%</p>
                </div>
                <div>
                    <p class="text-gray-400 text-sm">Internet Uptime</p>
                    <p id="stat-internet-uptime" class="text-lg font-semibold">--%</p>
                </div>
                <div>
                    <p class="text-gray-400 text-sm">Avg Latency</p>
                    <p id="stat-latency" class="text-lg font-semibold">-- ms</p>
                </div>
                <div>
                    <p class="text-gray-400 text-sm">P95 Latency</p>
                    <p id="stat-p95" class="text-lg font-semibold">-- ms</p>
                </div>
                <div>
                    <p class="text-gray-400 text-sm">Disconnections</p>
                    <p id="stat-disconnections" class="text-lg font-semibold">--</p>
                </div>
            </div>
        </div>

        <!-- Charts -->
        <div class="grid grid-cols-1 lg:grid-cols-2 gap-6 mb-8">
            <div class="bg-gray-800 rounded-lg p-4 border border-gray-700">
                <h3 class="text-lg font-semibold mb-4">Signal Strength Over Time</h3>
                <div class="chart-container">
                    <canvas id="signal-chart"></canvas>
                </div>
            </div>

            <div class="bg-gray-800 rounded-lg p-4 border border-gray-700">
                <h3 class="text-lg font-semibold mb-4">Latency Over Time</h3>
                <div class="chart-container">
                    <canvas id="latency-chart"></canvas>
                </div>
            </div>

            <div class="bg-gray-800 rounded-lg p-4 border border-gray-700">
                <h3 class="text-lg font-semibold mb-4">Packet Loss Over Time</h3>
                <div class="chart-container">
                    <canvas id="packet-loss-chart"></canvas>
                </div>
            </div>

            <div class="bg-gray-800 rounded-lg p-4 border border-gray-700">
                <h3 class="text-lg font-semibold mb-4">Connection Status</h3>
                <div class="chart-container">
                    <canvas id="connection-chart"></canvas>
                </div>
            </div>
        </div>

        <!-- Event Counts -->
        <div class="grid grid-cols-1 lg:grid-cols-3 gap-6 mb-8">
            <div class="bg-gray-800 rounded-lg p-4 border border-gray-700">
                <h3 class="text-lg font-semibold mb-4">Events by Type</h3>
                <div class="chart-container">
                    <canvas id="event-type-chart"></canvas>
                </div>
            </div>

            <div class="lg:col-span-2 bg-gray-800 rounded-lg p-4 border border-gray-700">
                <h3 class="text-lg font-semibold mb-4">DNS Resolution Time</h3>
                <div class="chart-container">
                    <canvas id="dns-chart"></canvas>
                </div>
            </div>
        </div>

        <!-- Events Log -->
        <div class="bg-gray-800 rounded-lg p-4 border border-gray-700 mb-8">
            <div class="flex justify-between items-center mb-4">
                <h2 class="text-xl font-semibold">Event Log</h2>
                <div class="flex gap-2">
                    <select id="severity-filter" class="bg-gray-700 border border-gray-600 rounded px-3 py-1 text-sm">
                        <option value="">All Severities</option>
                        <option value="Critical">Critical</option>
                        <option value="Error">Error</option>
                        <option value="Warning">Warning</option>
                        <option value="Info">Info</option>
                    </select>
                    <button onclick="refreshEvents()" class="bg-blue-600 hover:bg-blue-700 px-3 py-1 rounded text-sm">Refresh</button>
                </div>
            </div>
            <div id="events-container" class="max-h-96 overflow-y-auto space-y-2">
                <p class="text-gray-500">Loading events...</p>
            </div>
        </div>

        <!-- Detailed Info -->
        <div class="bg-gray-800 rounded-lg p-4 border border-gray-700">
            <h2 class="text-xl font-semibold mb-4">Detailed Network Information</h2>
            <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
                <div>
                    <h4 class="text-gray-400 text-sm font-medium mb-2">WiFi Details</h4>
                    <div class="space-y-1 text-sm">
                        <p><span class="text-gray-500">BSSID:</span> <span id="detail-bssid">--</span></p>
                        <p><span class="text-gray-500">PHY Type:</span> <span id="detail-phy">--</span></p>
                        <p><span class="text-gray-500">Security:</span> <span id="detail-security">--</span></p>
                        <p><span class="text-gray-500">Frequency:</span> <span id="detail-frequency">-- MHz</span></p>
                    </div>
                </div>
                <div>
                    <h4 class="text-gray-400 text-sm font-medium mb-2">IP Configuration</h4>
                    <div class="space-y-1 text-sm">
                        <p><span class="text-gray-500">IPv4:</span> <span id="detail-ipv4">--</span></p>
                        <p><span class="text-gray-500">IPv6:</span> <span id="detail-ipv6" class="truncate block max-w-xs">--</span></p>
                        <p><span class="text-gray-500">Gateway:</span> <span id="detail-gateway">--</span></p>
                        <p><span class="text-gray-500">DNS:</span> <span id="detail-dns">--</span></p>
                    </div>
                </div>
                <div>
                    <h4 class="text-gray-400 text-sm font-medium mb-2">System Stats</h4>
                    <div class="space-y-1 text-sm">
                        <p><span class="text-gray-500">CPU Usage:</span> <span id="detail-cpu">--%</span></p>
                        <p><span class="text-gray-500">Memory Usage:</span> <span id="detail-memory">--%</span></p>
                        <p><span class="text-gray-500">Bytes Sent:</span> <span id="detail-bytes-sent">--</span></p>
                        <p><span class="text-gray-500">Bytes Received:</span> <span id="detail-bytes-recv">--</span></p>
                    </div>
                </div>
            </div>
        </div>

        <footer class="mt-8 text-center text-gray-500 text-sm">
            <p>WiFi Stability Tracker | Last updated: <span id="last-update">--</span></p>
        </footer>
    </div>

    <script>
        // Chart instances
        let signalChart, latencyChart, packetLossChart, connectionChart, eventTypeChart, dnsChart;
        
        // Time range state
        let currentTimeRange = { minutes: 60, start: null, end: null };
        
        // Get adaptive time unit based on range
        function getTimeUnit(minutes) {
            if (minutes <= 15) return 'minute';
            if (minutes <= 180) return 'minute';
            if (minutes <= 1440) return 'hour';
            return 'day';
        }
        
        // Get time range parameters
        function getTimeRangeParams() {
            if (currentTimeRange.start && currentTimeRange.end) {
                return `start=${currentTimeRange.start}&end=${currentTimeRange.end}`;
            }
            const end = new Date();
            const start = new Date(end.getTime() - currentTimeRange.minutes * 60000);
            return `start=${start.toISOString()}&end=${end.toISOString()}`;
        }
        
        // Initialize charts
        function initCharts() {
            const chartOptions = {
                responsive: true,
                maintainAspectRatio: false,
                scales: {
                    x: {
                        type: 'time',
                        time: { 
                            unit: getTimeUnit(currentTimeRange.minutes),
                            displayFormats: {
                                minute: 'HH:mm',
                                hour: 'MMM d HH:mm',
                                day: 'MMM d'
                            }
                        },
                        grid: { color: 'rgba(255,255,255,0.1)' },
                        ticks: { color: '#9ca3af', maxRotation: 45, minRotation: 0 }
                    },
                    y: {
                        grid: { color: 'rgba(255,255,255,0.1)' },
                        ticks: { color: '#9ca3af' }
                    }
                },
                plugins: {
                    legend: { display: false }
                }
            };

            signalChart = new Chart(document.getElementById('signal-chart'), {
                type: 'line',
                data: { datasets: [{ label: 'Signal (dBm)', borderColor: '#10b981', backgroundColor: 'rgba(16,185,129,0.1)', fill: true, tension: 0.3 }] },
                options: { ...chartOptions, scales: { ...chartOptions.scales, y: { ...chartOptions.scales.y, reverse: false, min: -100, max: -30 } } }
            });

            latencyChart = new Chart(document.getElementById('latency-chart'), {
                type: 'line',
                data: { 
                    datasets: [
                        { label: 'Loopback', borderColor: '#10b981', backgroundColor: 'transparent', tension: 0.3, borderWidth: 2 },
                        { label: 'Router', borderColor: '#f59e0b', backgroundColor: 'transparent', tension: 0.3, borderWidth: 2 },
                        { label: 'Avg Latency', borderColor: '#3b82f6', backgroundColor: 'rgba(59,130,246,0.1)', fill: true, tension: 0.3 },
                        { label: 'Max Latency', borderColor: '#ef4444', backgroundColor: 'transparent', borderDash: [5, 5], tension: 0.3 }
                    ] 
                },
                options: { ...chartOptions, plugins: { legend: { display: true, labels: { color: '#9ca3af' } } } }
            });

            packetLossChart = new Chart(document.getElementById('packet-loss-chart'), {
                type: 'line',
                data: { datasets: [{ label: 'Packet Loss (%)', borderColor: '#f59e0b', backgroundColor: 'rgba(245,158,11,0.1)', fill: true, tension: 0.3 }] },
                options: { ...chartOptions, scales: { ...chartOptions.scales, y: { ...chartOptions.scales.y, min: 0, max: 100 } } }
            });

            connectionChart = new Chart(document.getElementById('connection-chart'), {
                type: 'line',
                data: { 
                    datasets: [
                        { label: 'WiFi Connected', borderColor: '#10b981', stepped: true },
                        { label: 'Router Reachable', borderColor: '#f59e0b', stepped: true },
                        { label: 'Internet Reachable', borderColor: '#3b82f6', stepped: true }
                    ] 
                },
                options: { ...chartOptions, scales: { ...chartOptions.scales, y: { ...chartOptions.scales.y, min: 0, max: 1.2 } }, plugins: { legend: { display: true, labels: { color: '#9ca3af' } } } }
            });

            eventTypeChart = new Chart(document.getElementById('event-type-chart'), {
                type: 'doughnut',
                data: { labels: [], datasets: [{ data: [], backgroundColor: ['#ef4444', '#f59e0b', '#3b82f6', '#10b981', '#8b5cf6', '#ec4899'] }] },
                options: { responsive: true, maintainAspectRatio: false, plugins: { legend: { position: 'right', labels: { color: '#9ca3af' } } } }
            });

            dnsChart = new Chart(document.getElementById('dns-chart'), {
                type: 'line',
                data: { datasets: [{ label: 'DNS Resolution (ms)', borderColor: '#8b5cf6', backgroundColor: 'rgba(139,92,246,0.1)', fill: true, tension: 0.3 }] },
                options: chartOptions
            });
        }

        // Update current status
        async function updateCurrent() {
            try {
                const response = await fetch('/api/current');
                const result = await response.json();
                
                if (result.success && result.data) {
                    const data = result.data;
                    
                    // Update signal
                    if (data.wifi_info) {
                        const wifi = data.wifi_info;
                        document.getElementById('signal-value').textContent = wifi.signal_strength_dbm;
                        document.getElementById('signal-quality').textContent = `${wifi.signal_quality_percent}% quality`;
                        document.getElementById('signal-bar').style.width = `${wifi.signal_quality_percent}%`;
                        
                        const signalEl = document.getElementById('signal-value');
                        signalEl.className = wifi.signal_strength_dbm > -60 ? 'text-2xl font-bold status-good' :
                                            wifi.signal_strength_dbm > -70 ? 'text-2xl font-bold status-warning' : 'text-2xl font-bold status-critical';
                        
                        document.getElementById('ssid-value').textContent = wifi.ssid || '--';
                        document.getElementById('channel-value').textContent = `Channel: ${wifi.channel} (${wifi.band.replace('Band', '').replace('_', '.')})`;
                        document.getElementById('speed-value').textContent = `Speed: ${wifi.link_speed_mbps} Mbps`;
                        
                        document.getElementById('detail-bssid').textContent = wifi.bssid || '--';
                        document.getElementById('detail-phy').textContent = wifi.phy_type || '--';
                        document.getElementById('detail-security').textContent = wifi.security_type || '--';
                        document.getElementById('detail-frequency').textContent = `${wifi.frequency_mhz} MHz`;
                        document.getElementById('detail-ipv4').textContent = wifi.ipv4_address || '--';
                        document.getElementById('detail-ipv6').textContent = wifi.ipv6_address || '--';
                        document.getElementById('detail-gateway').textContent = wifi.gateway || '--';
                        document.getElementById('detail-dns').textContent = wifi.dns_servers?.join(', ') || '--';
                    }
                    
                    // Update latency
                    if (data.latency) {
                        const lat = data.latency;
                        document.getElementById('latency-value').textContent = lat.average_latency_ms?.toFixed(1) || '--';
                        document.getElementById('latency-range').textContent = `Min: ${lat.min_latency_ms?.toFixed(1) || '--'} / Max: ${lat.max_latency_ms?.toFixed(1) || '--'}`;
                        document.getElementById('jitter-value').textContent = `Jitter: ${lat.jitter_ms?.toFixed(1) || '--'} ms`;
                        document.getElementById('packet-loss-value').textContent = lat.packet_loss_percent?.toFixed(1) || '0';
                        
                        const latEl = document.getElementById('latency-value');
                        const avgLat = lat.average_latency_ms || 0;
                        latEl.className = avgLat < 50 ? 'text-2xl font-bold status-good' :
                                         avgLat < 100 ? 'text-2xl font-bold status-warning' : 'text-2xl font-bold status-critical';
                    }
                    
                    // Update connectivity
                    if (data.connectivity) {
                        const conn = data.connectivity;
                        document.getElementById('loopback-status').innerHTML = `Loopback: <span class="font-semibold ${conn.loopback_reachable ? 'status-good' : 'status-critical'}">${conn.loopback_reachable ? 'OK' : 'Failed'}</span>`;
                        document.getElementById('router-status').innerHTML = `Router: <span class="font-semibold ${conn.router_reachable ? 'status-good' : 'status-critical'}">${conn.router_reachable ? 'Reachable' : 'Unreachable'}</span>`;
                        document.getElementById('internet-status').innerHTML = `Internet: <span class="font-semibold ${conn.internet_reachable ? 'status-good' : 'status-critical'}">${conn.internet_reachable ? 'Reachable' : 'Unreachable'}</span>`;
                        document.getElementById('connection-status').innerHTML = `WiFi: <span class="font-semibold ${conn.is_connected ? 'status-good' : 'status-critical'}">${conn.is_connected ? 'Connected' : 'Disconnected'}</span>`;
                    }
                    
                    // Update system info
                    if (data.system_info) {
                        const sys = data.system_info;
                        document.getElementById('detail-cpu').textContent = `${sys.cpu_usage_percent?.toFixed(1)}%`;
                        document.getElementById('detail-memory').textContent = `${sys.memory_usage_percent?.toFixed(1)}%`;
                        document.getElementById('detail-bytes-sent').textContent = formatBytes(sys.bytes_sent);
                        document.getElementById('detail-bytes-recv').textContent = formatBytes(sys.bytes_received);
                    }
                    
                    document.getElementById('last-update').textContent = new Date(data.timestamp).toLocaleString();
                }
            } catch (e) {
                console.error('Failed to fetch current data:', e);
            }
        }

        // Update chart time scales
        function updateChartTimeScales() {
            const timeUnit = getTimeUnit(currentTimeRange.minutes);
            const charts = [signalChart, latencyChart, packetLossChart, connectionChart, dnsChart];
            
            charts.forEach(chart => {
                if (chart && chart.options.scales.x) {
                    chart.options.scales.x.time.unit = timeUnit;
                    chart.update('none');
                }
            });
        }
        
        // Update charts
        async function updateCharts() {
            try {
                const timeParams = getTimeRangeParams();
                const [signalRes, latencyLoopbackRes, latencyRouterRes, latencyAvgRes, latencyMaxRes, packetLossRes, connectedRes, routerRes, internetRes, dnsRes] = await Promise.all([
                    fetch(`/api/timeseries?metric=signal_dbm&${timeParams}`),
                    fetch(`/api/timeseries?metric=latency_loopback&${timeParams}`),
                    fetch(`/api/timeseries?metric=latency_router&${timeParams}`),
                    fetch(`/api/timeseries?metric=latency_avg&${timeParams}`),
                    fetch(`/api/timeseries?metric=latency_max&${timeParams}`),
                    fetch(`/api/timeseries?metric=packet_loss&${timeParams}`),
                    fetch(`/api/timeseries?metric=connected&${timeParams}`),
                    fetch(`/api/timeseries?metric=router_reachable&${timeParams}`),
                    fetch(`/api/timeseries?metric=internet_reachable&${timeParams}`),
                    fetch(`/api/timeseries?metric=dns_resolution_time&${timeParams}`)
                ]);

                const [signalData, latencyLoopbackData, latencyRouterData, latencyAvgData, latencyMaxData, packetLossData, connectedData, routerData, internetData, dnsData] = await Promise.all([
                    signalRes.json(), latencyLoopbackRes.json(), latencyRouterRes.json(), latencyAvgRes.json(), latencyMaxRes.json(), packetLossRes.json(), connectedRes.json(), routerRes.json(), internetRes.json(), dnsRes.json()
                ]);

                if (signalData.success) {
                    signalChart.data.datasets[0].data = signalData.data.map(d => ({ x: new Date(d.timestamp), y: d.value }));
                    signalChart.update('none');
                }

                if (latencyLoopbackData.success && latencyRouterData.success && latencyAvgData.success && latencyMaxData.success) {
                    latencyChart.data.datasets[0].data = latencyLoopbackData.data.map(d => ({ x: new Date(d.timestamp), y: d.value }));
                    latencyChart.data.datasets[1].data = latencyRouterData.data.map(d => ({ x: new Date(d.timestamp), y: d.value }));
                    latencyChart.data.datasets[2].data = latencyAvgData.data.map(d => ({ x: new Date(d.timestamp), y: d.value }));
                    latencyChart.data.datasets[3].data = latencyMaxData.data.map(d => ({ x: new Date(d.timestamp), y: d.value }));
                    latencyChart.update('none');
                }

                if (packetLossData.success) {
                    packetLossChart.data.datasets[0].data = packetLossData.data.map(d => ({ x: new Date(d.timestamp), y: d.value }));
                    packetLossChart.update('none');
                }

                if (connectedData.success && routerData.success && internetData.success) {
                    connectionChart.data.datasets[0].data = connectedData.data.map(d => ({ x: new Date(d.timestamp), y: d.value }));
                    connectionChart.data.datasets[1].data = routerData.data.map(d => ({ x: new Date(d.timestamp), y: d.value }));
                    connectionChart.data.datasets[2].data = internetData.data.map(d => ({ x: new Date(d.timestamp), y: d.value }));
                    connectionChart.update('none');
                }

                if (dnsData.success) {
                    dnsChart.data.datasets[0].data = dnsData.data.map(d => ({ x: new Date(d.timestamp), y: d.value }));
                    dnsChart.update('none');
                }
            } catch (e) {
                console.error('Failed to update charts:', e);
            }
        }

        // Update event type chart
        async function updateEventCounts() {
            try {
                const timeParams = getTimeRangeParams();
                const response = await fetch(`/api/event-counts?${timeParams}`);
                const result = await response.json();
                
                if (result.success && result.data.length > 0) {
                    eventTypeChart.data.labels = result.data.map(d => d.event_type);
                    eventTypeChart.data.datasets[0].data = result.data.map(d => d.count);
                    eventTypeChart.update('none');
                }
            } catch (e) {
                console.error('Failed to fetch event counts:', e);
            }
        }

        // Update statistics
        async function updateStatistics() {
            try {
                const timeParams = getTimeRangeParams();
                const response = await fetch(`/api/statistics?${timeParams}`);
                const result = await response.json();
                
                if (result.success && result.data) {
                    const stats = result.data;
                    document.getElementById('stat-samples').textContent = stats.sample_count || '--';
                    document.getElementById('stat-uptime').textContent = `${stats.connection_uptime_percent?.toFixed(1) || '--'}%`;
                    document.getElementById('stat-internet-uptime').textContent = `${stats.internet_uptime_percent?.toFixed(1) || '--'}%`;
                    document.getElementById('stat-latency').textContent = `${stats.latency_avg_ms?.toFixed(1) || '--'} ms`;
                    document.getElementById('stat-p95').textContent = `${stats.latency_p95_ms?.toFixed(1) || '--'} ms`;
                    document.getElementById('stat-disconnections').textContent = stats.total_disconnections || '0';
                }
            } catch (e) {
                console.error('Failed to fetch statistics:', e);
            }
        }

        // Refresh events
        async function refreshEvents() {
            try {
                const severity = document.getElementById('severity-filter').value;
                const timeParams = getTimeRangeParams();
                const url = severity ? `/api/events?severity=${severity}&${timeParams}` : `/api/events?${timeParams}`;
                const response = await fetch(url);
                const result = await response.json();
                
                const container = document.getElementById('events-container');
                
                if (result.success && result.data.length > 0) {
                    container.innerHTML = result.data.slice(0, 100).map(event => `
                        <div class="log-entry bg-gray-700 rounded p-2 flex items-start gap-3">
                            <span class="severity-${event.severity.toLowerCase()} text-white text-xs px-2 py-0.5 rounded">${event.severity}</span>
                            <span class="text-gray-400 whitespace-nowrap">${new Date(event.timestamp).toLocaleString()}</span>
                            <span class="text-blue-400">[${event.event_type}]</span>
                            <span class="text-gray-200 flex-1">${event.description}</span>
                        </div>
                    `).join('');
                } else {
                    container.innerHTML = '<p class="text-gray-500">No events recorded yet.</p>';
                }
            } catch (e) {
                console.error('Failed to fetch events:', e);
            }
        }

        // Helper function
        function formatBytes(bytes) {
            if (!bytes) return '--';
            const units = ['B', 'KB', 'MB', 'GB', 'TB'];
            let i = 0;
            while (bytes >= 1024 && i < units.length - 1) {
                bytes /= 1024;
                i++;
            }
            return `${bytes.toFixed(1)} ${units[i]}`;
        }

        // Handle time range change
        function onTimeRangeChange() {
            const select = document.getElementById('time-range');
            const customRange = document.getElementById('custom-range');
            
            if (select.value === 'custom') {
                customRange.classList.remove('hidden');
                // Set default values to last hour
                const end = new Date();
                const start = new Date(end.getTime() - 60 * 60000);
                document.getElementById('end-time').value = end.toISOString().slice(0, 16);
                document.getElementById('start-time').value = start.toISOString().slice(0, 16);
            } else {
                customRange.classList.add('hidden');
                currentTimeRange.minutes = parseInt(select.value);
                currentTimeRange.start = null;
                currentTimeRange.end = null;
                updateChartTimeScales();
                refreshAllData();
            }
        }

        // Apply custom time range
        function applyCustomRange() {
            const startInput = document.getElementById('start-time').value;
            const endInput = document.getElementById('end-time').value;
            
            if (!startInput || !endInput) {
                alert('Please select both start and end times');
                return;
            }
            
            const start = new Date(startInput);
            const end = new Date(endInput);
            
            if (start >= end) {
                alert('Start time must be before end time');
                return;
            }
            
            currentTimeRange.start = start.toISOString();
            currentTimeRange.end = end.toISOString();
            currentTimeRange.minutes = Math.floor((end - start) / 60000);
            
            updateChartTimeScales();
            refreshAllData();
        }

        // Refresh all data
        function refreshAllData() {
            updateCharts();
            updateEventCounts();
            updateStatistics();
            refreshEvents();
        }

        // Initialize
        document.addEventListener('DOMContentLoaded', () => {
            initCharts();
            updateCurrent();
            updateCharts();
            updateEventCounts();
            updateStatistics();
            refreshEvents();

            // Auto-refresh
            setInterval(updateCurrent, 5000);
            setInterval(updateCharts, 10000);
            setInterval(updateEventCounts, 30000);
            setInterval(updateStatistics, 30000);
            setInterval(refreshEvents, 15000);
            
            // Event listeners
            document.getElementById('time-range').addEventListener('change', onTimeRangeChange);
            document.getElementById('severity-filter').addEventListener('change', refreshEvents);
        });
    </script>
</body>
</html>
"##;
