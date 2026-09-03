// Desktop 电池信息模拟

/// 获取模拟电池信息 (Linux 下会尝试读取真实数据)
pub fn get_battery_info() -> String {
    // 尝试读取 Linux 电池信息
    #[cfg(target_os = "linux")]
    {
        if let Ok(capacity) = std::fs::read_to_string("/sys/class/power_supply/BAT0/capacity") {
            let level = capacity.trim();
            if let Ok(status) = std::fs::read_to_string("/sys/class/power_supply/BAT0/status") {
                let status = status.trim();
                return format!(
                    "模拟电池信息 (Linux)\n\
                     电量: {}%\n\
                     状态: {}",
                    level, status
                );
            }
            return format!("模拟电池信息 (Linux)\n电量: {}%", level);
        }
    }

    "模拟电池信息 (Desktop)\n\
     电量: 85%\n\
     状态: 充电中\n\
     注意: 真实电池信息需要在 Android 上运行"
        .to_string()
}