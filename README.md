# vnekey

Bộ gõ tiếng Việt cho Windows viết bằng Rust 2024, kết hợp tính năng của UniKey và EVKey.

## Cấu trúc

| Crate | Vai trò | Chạy test được trên |
|---|---|---|
| `vnekey-core` | Bộ gõ thuần logic: Telex/VNI, đặt dấu, kiểm tra chính tả, gõ tắt, tự viết hoa, bảng mã (Unicode, Unicode tổ hợp, TCVN3, VNI Windows, VIQR) | mọi nền tảng |
| `vnekey-config` | Cấu hình `config.toml`, phím chuyển V/E, đọc ghi bảng gõ tắt | mọi nền tảng |
| `vnekey-win` | Hook bàn phím/chuột, khay hệ thống, theo dõi cửa sổ focus, gửi phím (SendInput/Clipboard), khởi động cùng Windows | logic thuần: mọi nền tảng; còn lại: Windows |
| `vnekey-app` | `vnekey.exe`: tiến trình khay và cửa sổ cài đặt (egui) | Windows |

## Cách chạy

`vnekey.exe` có hai chế độ:

- Mặc định: chạy ở khay hệ thống, giữ hook bàn phím. Bấm trái biểu tượng để đổi V/E, bấm đúp để mở cài đặt, bấm phải để mở menu.
- `vnekey.exe --settings`: cửa sổ cài đặt. Bấm **Lưu** ghi `%APPDATA%\vnekey\config.toml`, tiến trình khay tự nạp lại trong khoảng một giây.

Lần chạy đầu tiên (chưa có `config.toml`) cửa sổ cài đặt tự mở.

## Build

Cần Rust 1.85 trở lên và MSVC (Visual Studio Build Tools) trên Windows.

```powershell
cargo test --workspace
cargo build --release -p vnekey      # target\release\vnekey.exe
```

CI (`.github/workflows/ci.yml`) chạy test trên Linux và Windows, build `vnekey.exe` làm artifact, và chạy rustfmt + clippy.
Đẩy tag `v0.1.0` để `release.yml` tạo bản phát hành kèm `vnekey.exe` và mã SHA-256.

## Ghi chú

- Quyền admin: manifest dùng `asInvoker`. Tuỳ chọn **Chạy với quyền admin** tự chạy lại ứng dụng với UAC; khi bật cùng **Khởi động cùng Windows** thì dùng Task Scheduler (tạo tác vụ cần quyền admin).
- Bảng mã TCVN3 và VNI Windows lấy dữ liệu từ [bamboo-core](https://github.com/bamboo-vi/bamboo-core) (MIT), xem đầu file `crates/vnekey-core/src/charset_tables.rs`.
- Phím dấu câu giả định bố cục QWERTY của Mỹ.
