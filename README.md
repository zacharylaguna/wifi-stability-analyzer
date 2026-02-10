# WiFi Stability Tracker

A comprehensive WiFi stability debugging tool written in Rust that collects detailed network metrics and provides real-time visualizations to help diagnose connectivity issues.

## Features

- **Real-time Monitoring**: Continuously monitors WiFi connection quality
- **Comprehensive Metrics Collection**:
  - Signal strength (dBm and quality percentage)
  - Latency measurements (ping to multiple targets)
  - Packet loss detection
  - DNS resolution times
  - Connection state tracking
  - BSSID/Channel/Band changes
  - System network statistics
- **Event Detection**: Automatically detects and logs network events:
  - Connection drops and recoveries
  - Signal strength degradation
  - High latency spikes
  - Packet loss
  - DNS failures
  - Band/Channel switches
  - BSSID roaming
- **Web Dashboard**: Beautiful real-time visualization dashboard with:
  - Current status cards
  - Time-series charts for all metrics
  - Event log with filtering
  - Statistics summary
- **Data Persistence**: SQLite database for storing all metrics
- **Analysis Reports**: Generate detailed reports with recommendations
- **JSON Export**: Export collected data for external analysis

## Installation

### Prerequisites

- Rust 1.70 or later
- Windows 10/11 (uses Windows-specific network commands)

### Building

```bash
cd wifi-stability-tracker
cargo build --release
```

The compiled binary will be at `target/release/wifi-stability-tracker.exe`

## Usage

### Start Monitoring

```bash
# Basic monitoring with default settings
wifi-stability-tracker monitor

# Custom interval and port
wifi-stability-tracker monitor --interval 10 --port 3000

# Custom ping targets
wifi-stability-tracker monitor --ping-targets "8.8.8.8,1.1.1.1,cloudflare.com"

# Full options
wifi-stability-tracker monitor \
    --interval 5 \
    --database wifi_data.db \
    --port 8080 \
    --log-dir ./logs \
    --ping-targets "8.8.8.8,1.1.1.1" \
    --dns-servers "8.8.8.8,1.1.1.1"
```

Then open `http://localhost:8080` in your browser to view the dashboard.

### View Dashboard Only (without new monitoring)

```bash
wifi-stability-tracker dashboard --database wifi_data.db --port 8080
```

### Export Data

```bash
# Export all data
wifi-stability-tracker export --database wifi_data.db --output wifi_export.json

# Export with time filter
wifi-stability-tracker export \
    --database wifi_data.db \
    --output wifi_export.json \
    --start "2024-01-01T00:00:00Z" \
    --end "2024-01-02T00:00:00Z"
```

### Generate Analysis Report

```bash
wifi-stability-tracker analyze --database wifi_data.db --output report.txt
```

## Dashboard Features

### Current Status Cards
- **Signal Strength**: Current signal in dBm with quality bar
- **Latency**: Average, min, max latency and jitter
- **Packet Loss**: Current packet loss percentage
- **Network Info**: SSID, channel, band, and link speed

### Charts
- Signal strength over time
- Latency trends (average and max)
- Packet loss history
- Connection status timeline
- DNS resolution times
- Event distribution by type

### Event Log
- Real-time event feed
- Filter by severity (Critical, Error, Warning, Info)
- Detailed event information with timestamps

### Detailed Information
- WiFi details (BSSID, PHY type, security)
- IP configuration (IPv4, IPv6, gateway, DNS)
- System stats (CPU, memory, network I/O)

## Metrics Collected

| Metric | Description |
|--------|-------------|
| Signal Strength | WiFi signal in dBm (-30 to -100) |
| Signal Quality | Percentage (0-100%) |
| Channel | WiFi channel number |
| Frequency | Operating frequency in MHz |
| Band | 2.4GHz, 5GHz, or 6GHz |
| Link Speed | Connection speed in Mbps |
| Latency | Round-trip time to ping targets |
| Jitter | Latency variation |
| Packet Loss | Percentage of lost packets |
| DNS Time | DNS resolution latency |
| HTTP Time | HTTP connectivity test time |

## Event Types

| Event | Severity | Description |
|-------|----------|-------------|
| ConnectionDropped | Critical | WiFi disconnected |
| ConnectionRestored | Info | WiFi reconnected |
| SignalStrengthLow | Warning/Critical | Signal below threshold |
| HighLatency | Warning/Critical | Latency above threshold |
| HighJitter | Warning | Jitter above 30ms |
| PacketLoss | Warning/Critical | Packet loss detected |
| DnsFailure | Warning | DNS resolution failed |
| BandSwitch | Warning | Switched between 2.4/5/6 GHz |
| ChannelChange | Info | WiFi channel changed |
| BssidChange | Warning | Connected to different AP |
| InternetUnreachable | Critical | Cannot reach internet |

## Thresholds

Default alert thresholds (can be customized in code):

| Metric | Warning | Critical |
|--------|---------|----------|
| Signal Strength | -70 dBm | -80 dBm |
| Latency | 100 ms | 300 ms |
| Jitter | 30 ms | - |
| Packet Loss | 1% | 5% |

## Troubleshooting

### "netsh" command not found
Ensure you're running on Windows and the command prompt has access to system utilities.

### Permission denied
Run the tool as Administrator for full access to network information.

### No data in dashboard
Wait for at least one monitoring interval (default 5 seconds) for data to appear.

## Architecture

```
wifi-stability-tracker/
├── src/
│   ├── main.rs        # CLI and application entry point
│   ├── metrics.rs     # Data structures for all metrics
│   ├── monitor.rs     # WiFi monitoring and data collection
│   ├── storage.rs     # SQLite database operations
│   ├── web.rs         # Web server and dashboard
│   └── analysis.rs    # Report generation and analysis
├── Cargo.toml         # Dependencies
└── README.md          # This file
```

## License

MIT License
