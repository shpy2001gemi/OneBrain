# OneBrain Seed — Arduino / Embedded (Future)

> **Status: 🔴 Planning — chưa implement**

## Mục tiêu

Chạy lightweight seed node trên embedded hardware:
- **ESP32** — WiFi + Bluetooth, ~520KB RAM
- **Raspberry Pi Pico W** — WiFi, ~264KB RAM
- **Arduino Nano 33 IoT** — WiFi, ~256KB RAM

## Thách thức kỹ thuật

| Thách thức | Giải pháp đề xuất |
|-----------|-------------------|
| RAM rất nhỏ (256-520KB) | Giới hạn max_peers (10-50), không lưu history |
| Không có OS | Rust `no_std` + `embassy` async runtime |
| Không có TCP stack đầy đủ | `smoltcp` embedded TCP/IP stack |
| Flash storage nhỏ | Binary size tối ưu: `opt-level = "z"`, LTO, strip |
| Không có JSON parser nặng | `serde_json_core` (no_alloc) hoặc custom binary protocol |

## Kiến trúc đề xuất

```
onebrain-seed-embedded/
├── Cargo.toml              # no_std, embassy runtime
├── src/
│   ├── main.rs             # Entry point (embassy)
│   ├── seed_lite.rs        # Simplified seed logic (max 50 peers)
│   ├── protocol_lite.rs    # Binary protocol (không JSON)
│   └── wifi.rs             # WiFi connection manager
├── boards/
│   ├── esp32/              # ESP32 config
│   ├── rpi_pico_w/         # Raspberry Pi Pico W config
│   └── nano_33_iot/        # Arduino Nano 33 IoT config
```

## Giới hạn so với seed tiêu chuẩn

| Feature | Standard Seed | Embedded Seed |
|---------|:-------------|:-------------|
| Max peers | 10,000 | 50 |
| Relay messages | ✅ | ✅ |
| Heartbeat | ✅ | ✅ |
| Stats logging | ✅ | ❌ (không có stdout) |
| Multi-thread | ✅ | ❌ (single-thread async) |
| JSON protocol | ✅ | ❌ (binary protocol) |
| TLS/QUIC | ✅ | ❌ (plain TCP) |

## Use case

- **Home seed**: ESP32 cắm router nhà, luôn online, giúp discovery cho devices trong nhà
- **Community seed**: Raspberry Pi đặt tại quán cà phê, trường học
- **Mesh seed**: Nhiều ESP32 tạo mesh network cho khu vực không có internet ổn định
