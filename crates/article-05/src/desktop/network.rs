// Desktop 网络信息模拟 — 尝试读取真实网络信息

/// 获取模拟网络信息 (Desktop 端模拟 ConnectivityManager)
pub fn get_network_info() -> String {
    #[cfg(target_os = "linux")]
    {
        // 尝试读取默认网关接口
        if let Ok(output) = std::process::Command::new("ip")
            .args(["route", "show", "default"])
            .output()
        {
            let route = String::from_utf8_lossy(&output.stdout);
            let iface = route
                .split_whitespace()
                .position(|w| w == "dev")
                .and_then(|i| route.split_whitespace().nth(i + 1))
                .unwrap_or("unknown");

            // 检查是否是无线接口
            let is_wifi = iface.starts_with("wlan") || iface.starts_with("wlp");

            // 尝试获取 IP 地址
            let ip = if let Ok(output) = std::process::Command::new("ip")
                .args(["addr", "show", iface])
                .output()
            {
                let addr_info = String::from_utf8_lossy(&output.stdout);
                addr_info
                    .split_whitespace()
                    .position(|w| w == "inet")
                    .and_then(|i| {
                        addr_info
                            .split_whitespace()
                            .nth(i + 1)
                            .map(|s| s.split('/').next().unwrap_or(s).to_string())
                    })
                    .unwrap_or_else(|| "无 IP".into())
            } else {
                "未知".into()
            };

            let conn_type = if is_wifi { "WiFi" } else { "以太网" };
            return format!(
                "模拟 Android 网络信息 (Desktop)\n\
                 类型: {}\n\
                 接口: {}\n\
                 IP 地址: {}\n\
                 状态: 已连接\n\
                 \n\
                 真实数据请在 Android 设备上运行",
                conn_type, iface, ip
            );
        }
    }

    "模拟 Android 网络信息 (Desktop)\n\
     类型: WiFi\n\
     状态: 已连接\n\
     \n\
     真实数据请在 Android 设备上运行"
        .to_string()
}
