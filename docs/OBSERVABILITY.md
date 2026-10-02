# Mandelbrot Observability Assessment & Stack Guidelines

## Overview

`mandelbrot` is a GNOME-native Matrix client written in Rust (using GTK 4, Libadwaita, and `matrix-sdk`).

This document summarizes the current telemetry, logging, and profiling subsystem for `mandelbrot`, documents environment variables for diagnostic execution, specifies local data flow boundaries, and outlines future OpenTelemetry integration options under telemetry policy.

---

## 1. Existing Telemetry & Logging Subsystem

### 1.1 Logging Library
- **`tracing` crate**: Used throughout `src/` for structured log events (`tracing::info!`, `tracing::warn!`, `tracing::error!`, `tracing::debug!`, `tracing::trace!`).
- **`tracing-subscriber`**: Controls filter levels, default formatting, and log capture.

### 1.2 Environment Variables for Diagnostics
Execution log levels can be adjusted using standard Rust and GTK log filtering environment variables:

- **`RUST_LOG`**: Controls `tracing-subscriber` filter levels (e.g. `RUST_LOG=mandelbrot=debug,matrix_sdk=info`).
- **`G_MESSAGES_DEBUG`**: Controls GLib/GTK log filtering (e.g., `G_MESSAGES_DEBUG=all` or `G_MESSAGES_DEBUG=mandelbrot`).
- **`GST_DEBUG`**: Controls GStreamer log output (e.g., `GST_DEBUG=3` or `GST_DEBUG=webrtc:5` for call/media diagnostic tracing).

---

## 2. Telemetry & Data Flow Boundaries

In compliance with operator policy:
- **No Remote Telemetry Exporters**: No telemetry, analytics, or metrics data is pushed to external remote servers or third-party analytical endpoints.
- **Local Diagnostic Output Only**: Diagnostic logs remain strictly local (stdout/stderr or local system logs via journald/Flatpak logging facilities).
- **Sensitive Data Protections**: Matrix user credentials, E2EE encryption keys, session tokens, and raw message payloads MUST NOT be emitted in non-debug/trace log statements.

---

## 3. Future OpenTelemetry Roadmap & Stack Guidelines

If an operator explicitly enables remote telemetry in a future release, the following architectural guidelines MUST be followed:

1. **`tracing-opentelemetry` Integration**:
   - Utilize `tracing-opentelemetry` subscriber layers to export `tracing` spans and events seamlessly without modifying application business logic.
2. **OpenTelemetry SDK Wiring**:
   - Use `opentelemetry` and `opentelemetry-otlp` crates behind a conditional build feature flag (e.g., `otel`).
3. **Explicit Opt-in Configuration**:
   - Telemetry collection must remain standard opt-in unless explicitly configured by the system operator.
