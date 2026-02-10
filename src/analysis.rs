use crate::metrics::*;
use crate::storage::MetricsStore;

pub fn generate_report(store: &MetricsStore) -> anyhow::Result<String> {
    let stats = store.get_statistics(None, None)?;
    let events = store.get_events(None, None, None, None)?;
    let event_counts = store.get_event_counts_by_type(None, None)?;

    let mut report = String::new();

    // Header
    report.push_str("═══════════════════════════════════════════════════════════════════\n");
    report.push_str("                    WiFi Stability Analysis Report                   \n");
    report.push_str("═══════════════════════════════════════════════════════════════════\n\n");

    // Time range
    report.push_str(&format!("Report Period: {} to {}\n", 
        stats.start_time.format("%Y-%m-%d %H:%M:%S UTC"),
        stats.end_time.format("%Y-%m-%d %H:%M:%S UTC")
    ));
    report.push_str(&format!("Total Samples: {}\n\n", stats.sample_count));

    // Overall Health Score
    let health_score = calculate_health_score(&stats);
    report.push_str("───────────────────────────────────────────────────────────────────\n");
    report.push_str("                         OVERALL HEALTH SCORE                       \n");
    report.push_str("───────────────────────────────────────────────────────────────────\n");
    report.push_str(&format!("\n  Score: {}/100 - {}\n\n", health_score, health_rating(health_score)));

    // Connection Reliability
    report.push_str("───────────────────────────────────────────────────────────────────\n");
    report.push_str("                       CONNECTION RELIABILITY                        \n");
    report.push_str("───────────────────────────────────────────────────────────────────\n\n");
    report.push_str(&format!("  WiFi Connection Uptime:    {:>6.1}%\n", stats.connection_uptime_percent));
    report.push_str(&format!("  Internet Uptime:           {:>6.1}%\n", stats.internet_uptime_percent));
    report.push_str(&format!("  Total Disconnections:      {:>6}\n", stats.total_disconnections));
    report.push_str(&format!("  Average Packet Loss:       {:>6.2}%\n\n", stats.packet_loss_avg_percent));

    // Signal Quality
    report.push_str("───────────────────────────────────────────────────────────────────\n");
    report.push_str("                          SIGNAL QUALITY                            \n");
    report.push_str("───────────────────────────────────────────────────────────────────\n\n");
    if let Some(avg) = stats.signal_strength_avg_dbm {
        report.push_str(&format!("  Average Signal:    {:>6.1} dBm  {}\n", avg, signal_rating(avg as i32)));
    }
    if let Some(min) = stats.signal_strength_min_dbm {
        report.push_str(&format!("  Minimum Signal:    {:>6} dBm  {}\n", min, signal_rating(min)));
    }
    if let Some(max) = stats.signal_strength_max_dbm {
        report.push_str(&format!("  Maximum Signal:    {:>6} dBm  {}\n", max, signal_rating(max)));
    }
    if let Some(quality) = stats.signal_quality_avg_percent {
        report.push_str(&format!("  Average Quality:   {:>6.1}%\n", quality));
    }
    report.push('\n');

    // Latency Analysis
    report.push_str("───────────────────────────────────────────────────────────────────\n");
    report.push_str("                         LATENCY ANALYSIS                           \n");
    report.push_str("───────────────────────────────────────────────────────────────────\n\n");
    if let Some(avg) = stats.latency_avg_ms {
        report.push_str(&format!("  Average Latency:   {:>8.1} ms  {}\n", avg, latency_rating(avg)));
    }
    if let Some(min) = stats.latency_min_ms {
        report.push_str(&format!("  Minimum Latency:   {:>8.1} ms\n", min));
    }
    if let Some(max) = stats.latency_max_ms {
        report.push_str(&format!("  Maximum Latency:   {:>8.1} ms\n", max));
    }
    if let Some(p95) = stats.latency_p95_ms {
        report.push_str(&format!("  95th Percentile:   {:>8.1} ms\n", p95));
    }
    if let Some(p99) = stats.latency_p99_ms {
        report.push_str(&format!("  99th Percentile:   {:>8.1} ms\n", p99));
    }
    if let Some(jitter) = stats.jitter_avg_ms {
        report.push_str(&format!("  Average Jitter:    {:>8.1} ms  {}\n", jitter, jitter_rating(jitter)));
    }
    report.push('\n');

    // Event Summary
    report.push_str("───────────────────────────────────────────────────────────────────\n");
    report.push_str("                          EVENT SUMMARY                             \n");
    report.push_str("───────────────────────────────────────────────────────────────────\n\n");
    report.push_str(&format!("  Critical Events:   {:>6}\n", stats.critical_events));
    report.push_str(&format!("  Error Events:      {:>6}\n", stats.error_events));
    report.push_str(&format!("  Warning Events:    {:>6}\n", stats.warning_events));
    report.push('\n');

    if !event_counts.is_empty() {
        report.push_str("  Events by Type:\n");
        for (event_type, count) in &event_counts {
            report.push_str(&format!("    - {}: {}\n", event_type, count));
        }
        report.push('\n');
    }

    // Issues Detected
    report.push_str("───────────────────────────────────────────────────────────────────\n");
    report.push_str("                         ISSUES DETECTED                            \n");
    report.push_str("───────────────────────────────────────────────────────────────────\n\n");

    let issues = analyze_issues(&stats, &events, &event_counts);
    if issues.is_empty() {
        report.push_str("  No significant issues detected.\n\n");
    } else {
        for (i, issue) in issues.iter().enumerate() {
            report.push_str(&format!("  {}. {}\n", i + 1, issue));
        }
        report.push('\n');
    }

    // Recommendations
    report.push_str("───────────────────────────────────────────────────────────────────\n");
    report.push_str("                        RECOMMENDATIONS                             \n");
    report.push_str("───────────────────────────────────────────────────────────────────\n\n");

    let recommendations = generate_recommendations(&stats, &events, &event_counts);
    if recommendations.is_empty() {
        report.push_str("  Your WiFi connection appears to be stable. No immediate actions needed.\n\n");
    } else {
        for (i, rec) in recommendations.iter().enumerate() {
            report.push_str(&format!("  {}. {}\n", i + 1, rec));
        }
        report.push('\n');
    }

    // Recent Critical Events
    let critical_events: Vec<_> = events.iter()
        .filter(|e| e.severity == EventSeverity::Critical)
        .take(10)
        .collect();

    if !critical_events.is_empty() {
        report.push_str("───────────────────────────────────────────────────────────────────\n");
        report.push_str("                      RECENT CRITICAL EVENTS                       \n");
        report.push_str("───────────────────────────────────────────────────────────────────\n\n");

        for event in critical_events {
            report.push_str(&format!("  [{}] {}: {}\n",
                event.timestamp.format("%Y-%m-%d %H:%M:%S"),
                format!("{:?}", event.event_type),
                event.description
            ));
        }
        report.push('\n');
    }

    report.push_str("═══════════════════════════════════════════════════════════════════\n");
    report.push_str("                         END OF REPORT                              \n");
    report.push_str("═══════════════════════════════════════════════════════════════════\n");

    Ok(report)
}

fn calculate_health_score(stats: &PeriodStatistics) -> u32 {
    let mut score = 100u32;

    // Deduct for uptime issues
    if stats.connection_uptime_percent < 100.0 {
        score = score.saturating_sub(((100.0 - stats.connection_uptime_percent) * 2.0) as u32);
    }
    if stats.internet_uptime_percent < 100.0 {
        score = score.saturating_sub(((100.0 - stats.internet_uptime_percent) * 1.5) as u32);
    }

    // Deduct for signal issues
    if let Some(avg_signal) = stats.signal_strength_avg_dbm {
        if avg_signal < -80.0 {
            score = score.saturating_sub(20);
        } else if avg_signal < -70.0 {
            score = score.saturating_sub(10);
        } else if avg_signal < -60.0 {
            score = score.saturating_sub(5);
        }
    }

    // Deduct for latency issues
    if let Some(avg_latency) = stats.latency_avg_ms {
        if avg_latency > 200.0 {
            score = score.saturating_sub(20);
        } else if avg_latency > 100.0 {
            score = score.saturating_sub(10);
        } else if avg_latency > 50.0 {
            score = score.saturating_sub(5);
        }
    }

    // Deduct for jitter
    if let Some(jitter) = stats.jitter_avg_ms {
        if jitter > 50.0 {
            score = score.saturating_sub(15);
        } else if jitter > 30.0 {
            score = score.saturating_sub(10);
        } else if jitter > 15.0 {
            score = score.saturating_sub(5);
        }
    }

    // Deduct for packet loss
    if stats.packet_loss_avg_percent > 5.0 {
        score = score.saturating_sub(20);
    } else if stats.packet_loss_avg_percent > 1.0 {
        score = score.saturating_sub(10);
    } else if stats.packet_loss_avg_percent > 0.1 {
        score = score.saturating_sub(5);
    }

    // Deduct for events
    score = score.saturating_sub(stats.critical_events * 5);
    score = score.saturating_sub(stats.error_events * 2);
    score = score.saturating_sub(stats.warning_events);

    score.min(100)
}

fn health_rating(score: u32) -> &'static str {
    match score {
        90..=100 => "Excellent",
        75..=89 => "Good",
        60..=74 => "Fair",
        40..=59 => "Poor",
        _ => "Critical",
    }
}

fn signal_rating(dbm: i32) -> &'static str {
    match dbm {
        -50..=0 => "(Excellent)",
        -60..=-51 => "(Good)",
        -70..=-61 => "(Fair)",
        -80..=-71 => "(Poor)",
        _ => "(Very Poor)",
    }
}

fn latency_rating(ms: f64) -> &'static str {
    match ms as i32 {
        0..=20 => "(Excellent)",
        21..=50 => "(Good)",
        51..=100 => "(Fair)",
        101..=200 => "(Poor)",
        _ => "(Very Poor)",
    }
}

fn jitter_rating(ms: f64) -> &'static str {
    match ms as i32 {
        0..=10 => "(Excellent)",
        11..=20 => "(Good)",
        21..=30 => "(Fair)",
        31..=50 => "(Poor)",
        _ => "(Very Poor)",
    }
}

fn analyze_issues(
    stats: &PeriodStatistics,
    _events: &[NetworkEvent],
    event_counts: &[(String, i64)],
) -> Vec<String> {
    let mut issues = Vec::new();

    // Connection issues
    if stats.total_disconnections > 0 {
        issues.push(format!(
            "WiFi connection dropped {} time(s) during the monitoring period",
            stats.total_disconnections
        ));
    }

    if stats.connection_uptime_percent < 99.0 {
        issues.push(format!(
            "WiFi connection uptime is only {:.1}% (expected >99%)",
            stats.connection_uptime_percent
        ));
    }

    if stats.internet_uptime_percent < 99.0 {
        issues.push(format!(
            "Internet connectivity uptime is only {:.1}% (expected >99%)",
            stats.internet_uptime_percent
        ));
    }

    // Signal issues
    if let Some(avg_signal) = stats.signal_strength_avg_dbm {
        if avg_signal < -75.0 {
            issues.push(format!(
                "Average signal strength is weak at {:.0} dBm (should be above -70 dBm)",
                avg_signal
            ));
        }
    }

    if let Some(min_signal) = stats.signal_strength_min_dbm {
        if min_signal < -85 {
            issues.push(format!(
                "Signal strength dropped to critically low levels ({} dBm)",
                min_signal
            ));
        }
    }

    // Latency issues
    if let Some(avg_latency) = stats.latency_avg_ms {
        if avg_latency > 100.0 {
            issues.push(format!(
                "Average latency is high at {:.1}ms (should be below 50ms for good performance)",
                avg_latency
            ));
        }
    }

    if let Some(p95) = stats.latency_p95_ms {
        if p95 > 200.0 {
            issues.push(format!(
                "95th percentile latency is very high at {:.1}ms indicating frequent spikes",
                p95
            ));
        }
    }

    // Jitter issues
    if let Some(jitter) = stats.jitter_avg_ms {
        if jitter > 30.0 {
            issues.push(format!(
                "High jitter detected ({:.1}ms) - this can cause issues with real-time applications",
                jitter
            ));
        }
    }

    // Packet loss issues
    if stats.packet_loss_avg_percent > 1.0 {
        issues.push(format!(
            "Significant packet loss detected ({:.2}%) - this can cause connection issues",
            stats.packet_loss_avg_percent
        ));
    }

    // Event-based issues
    for (event_type, count) in event_counts {
        if *count > 5 {
            match event_type.as_str() {
                "BssidChange" => issues.push(format!(
                    "Frequent BSSID changes ({} times) - your device may be roaming between access points",
                    count
                )),
                "ChannelChange" => issues.push(format!(
                    "Frequent channel changes ({} times) - possible interference or router auto-channel issues",
                    count
                )),
                "BandSwitch" => issues.push(format!(
                    "Frequent band switching ({} times) - unstable 5GHz connection or band steering issues",
                    count
                )),
                "DnsFailure" => issues.push(format!(
                    "Multiple DNS failures ({} times) - DNS server issues detected",
                    count
                )),
                _ => {}
            }
        }
    }

    issues
}

fn generate_recommendations(
    stats: &PeriodStatistics,
    _events: &[NetworkEvent],
    event_counts: &[(String, i64)],
) -> Vec<String> {
    let mut recommendations = Vec::new();

    // Signal-based recommendations
    if let Some(avg_signal) = stats.signal_strength_avg_dbm {
        if avg_signal < -75.0 {
            recommendations.push(
                "Move closer to your WiFi router or access point".to_string()
            );
            recommendations.push(
                "Consider adding a WiFi extender or mesh network node".to_string()
            );
            recommendations.push(
                "Check for physical obstructions between your device and the router".to_string()
            );
        }
    }

    // Band-related recommendations
    let band_switches = event_counts.iter()
        .find(|(t, _)| t == "BandSwitch")
        .map(|(_, c)| *c)
        .unwrap_or(0);

    if band_switches > 3 {
        recommendations.push(
            "Consider disabling band steering on your router and manually selecting 5GHz".to_string()
        );
        recommendations.push(
            "If 5GHz is unstable, try using 2.4GHz for better range at lower speeds".to_string()
        );
    }

    // Channel-related recommendations
    let channel_changes = event_counts.iter()
        .find(|(t, _)| t == "ChannelChange")
        .map(|(_, c)| *c)
        .unwrap_or(0);

    if channel_changes > 5 {
        recommendations.push(
            "Use a WiFi analyzer app to find the least congested channel".to_string()
        );
        recommendations.push(
            "Manually set your router to a specific channel instead of auto".to_string()
        );
    }

    // BSSID-related recommendations
    let bssid_changes = event_counts.iter()
        .find(|(t, _)| t == "BssidChange")
        .map(|(_, c)| *c)
        .unwrap_or(0);

    if bssid_changes > 5 {
        recommendations.push(
            "If you have multiple access points, ensure they have different SSIDs or configure proper roaming".to_string()
        );
        recommendations.push(
            "Check if your router's roaming aggressiveness settings can be adjusted".to_string()
        );
    }

    // Latency recommendations
    if let Some(avg_latency) = stats.latency_avg_ms {
        if avg_latency > 100.0 {
            recommendations.push(
                "Check for bandwidth-heavy applications running in the background".to_string()
            );
            recommendations.push(
                "Consider enabling QoS (Quality of Service) on your router".to_string()
            );
            recommendations.push(
                "Test with a wired connection to determine if the issue is WiFi-specific".to_string()
            );
        }
    }

    // Jitter recommendations
    if let Some(jitter) = stats.jitter_avg_ms {
        if jitter > 30.0 {
            recommendations.push(
                "High jitter often indicates network congestion - check for other devices using bandwidth".to_string()
            );
            recommendations.push(
                "Update your router's firmware to the latest version".to_string()
            );
        }
    }

    // Packet loss recommendations
    if stats.packet_loss_avg_percent > 1.0 {
        recommendations.push(
            "Packet loss can be caused by interference - check for nearby electronics (microwaves, cordless phones)".to_string()
        );
        recommendations.push(
            "Try changing your WiFi channel to reduce interference".to_string()
        );
        recommendations.push(
            "Check your router and modem for overheating issues".to_string()
        );
    }

    // DNS recommendations
    let dns_failures = event_counts.iter()
        .find(|(t, _)| t == "DnsFailure")
        .map(|(_, c)| *c)
        .unwrap_or(0);

    if dns_failures > 3 {
        recommendations.push(
            "Consider using alternative DNS servers like 8.8.8.8 (Google) or 1.1.1.1 (Cloudflare)".to_string()
        );
    }

    // Disconnection recommendations
    if stats.total_disconnections > 2 {
        recommendations.push(
            "Frequent disconnections may indicate driver issues - update your WiFi adapter drivers".to_string()
        );
        recommendations.push(
            "Check your router's logs for any error messages".to_string()
        );
        recommendations.push(
            "Disable WiFi power saving mode in your adapter settings".to_string()
        );
    }

    // General recommendations if issues exist
    if !recommendations.is_empty() {
        recommendations.push(
            "Consider restarting your router if you haven't done so recently".to_string()
        );
    }

    recommendations
}
