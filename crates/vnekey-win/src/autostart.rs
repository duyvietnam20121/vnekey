//! Khởi động cùng Windows.
//!
//! - Thường: giá trị `vnekey` trong `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`.
//! - Chạy với quyền admin: Windows không tự mở ứng dụng yêu cầu nâng quyền từ khoá Run, nên
//!   dùng Task Scheduler (`schtasks`, mức chạy cao nhất, khi đăng nhập). Việc tạo tác vụ
//!   cần quyền admin, vì vậy chỉ thành công khi vnekey đang chạy ở chế độ nâng quyền.
//!
//! Dùng `reg.exe` và `schtasks.exe` có sẵn thay vì gọi trực tiếp API để mã đơn giản và dễ kiểm thử.

pub const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
pub const VALUE_NAME: &str = "vnekey";
pub const TASK_NAME: &str = "vnekey";

/// Tham số cho `reg.exe`.
pub fn run_key_args(exe: &str, enable: bool) -> Vec<String> {
    let s = |v: &[&str]| v.iter().map(|x| (*x).to_owned()).collect::<Vec<_>>();
    if enable {
        let mut args = s(&["add", RUN_KEY, "/v", VALUE_NAME, "/t", "REG_SZ", "/d"]);
        args.push(format!("\"{exe}\""));
        args.push("/f".to_owned());
        args
    } else {
        s(&["delete", RUN_KEY, "/v", VALUE_NAME, "/f"])
    }
}

/// Tham số cho `schtasks.exe`. Tác vụ chạy lại vnekey với cờ `--elevated`.
pub fn task_args(exe: &str, enable: bool) -> Vec<String> {
    let s = |v: &[&str]| v.iter().map(|x| (*x).to_owned()).collect::<Vec<_>>();
    if enable {
        let mut args = s(&["/Create", "/TN", TASK_NAME, "/TR"]);
        args.push(format!("\"{exe}\" --elevated"));
        args.extend(s(&["/SC", "ONLOGON", "/RL", "HIGHEST", "/F"]));
        args
    } else {
        s(&["/Delete", "/TN", TASK_NAME, "/F"])
    }
}

/// Bật/tắt khởi động cùng Windows. `as_admin`: dùng Task Scheduler với quyền cao nhất.
#[cfg(windows)]
pub fn set(enable: bool, as_admin: bool) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe = exe.to_string_lossy().into_owned();

    if enable && as_admin {
        run("schtasks", &task_args(&exe, true)).map_err(|e| {
            format!("Không tạo được tác vụ khởi động với quyền admin (cần chạy vnekey bằng quyền admin): {e}")
        })?;
        let _ = run("reg", &run_key_args(&exe, false)); // bỏ mục thường nếu có
    } else if enable {
        run("reg", &run_key_args(&exe, true))
            .map_err(|e| format!("Không ghi được khoá Run: {e}"))?;
        let _ = run("schtasks", &task_args(&exe, false));
    } else {
        let _ = run("reg", &run_key_args(&exe, false));
        let _ = run("schtasks", &task_args(&exe, false));
    }
    Ok(())
}

#[cfg(windows)]
fn run(program: &str, args: &[String]) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let output = std::process::Command::new(program)
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        let text = String::from_utf8_lossy(&output.stderr);
        Err(text.trim().to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_key_add_quotes_path() {
        let args = run_key_args(r"C:\Program Files\vnekey\vnekey.exe", true);
        assert_eq!(args[0], "add");
        assert_eq!(args[1], RUN_KEY);
        assert!(args.contains(&r#""C:\Program Files\vnekey\vnekey.exe""#.to_owned()));
        assert_eq!(args.last().unwrap(), "/f");
    }

    #[test]
    fn run_key_delete() {
        assert_eq!(run_key_args("x", false), ["delete", RUN_KEY, "/v", "vnekey", "/f"]);
    }

    #[test]
    fn task_runs_elevated_at_logon() {
        let args = task_args(r"C:\a b\vnekey.exe", true);
        assert!(args.windows(2).any(|w| w == ["/RL", "HIGHEST"]));
        assert!(args.windows(2).any(|w| w == ["/SC", "ONLOGON"]));
        assert!(args.contains(&r#""C:\a b\vnekey.exe" --elevated"#.to_owned()));
        assert_eq!(task_args("x", false), ["/Delete", "/TN", "vnekey", "/F"]);
    }
}
