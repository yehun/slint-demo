// Desktop 设备信息模拟 — 尝试读取真实系统信息

/// 获取模拟设备信息 (Desktop 端模拟 Android Build 类)
pub fn get_device_info() -> String {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    let family = std::env::consts::FAMILY;

    // Linux 下尝试读取更多系统信息
    #[cfg(target_os = "linux")]
    {
        let model = std::fs::read_to_string("/sys/devices/virtual/dmi/id/product_name")
            .unwrap_or_default()
            .trim()
            .to_string();
        let manufacturer = std::fs::read_to_string("/sys/devices/virtual/dmi/id/sys_vendor")
            .unwrap_or_default()
            .trim()
            .to_string();
        if !model.is_empty() {
            return format!(
                "模拟 Android 设备信息 (Desktop)\n\
                 型号: {}\n\
                 制造商: {}\n\
                 硬件: {}\n\
                 系统: {} ({})\n\
                 架构: {}\n\
                 \n\
                 真实数据请在 Android 设备上运行",
                model,
                manufacturer,
                arch,
                os,
                family,
                arch
            );
        }
    }

    format!(
        "模拟 Android 设备信息 (Desktop)\n\
         型号: Unknown (Desktop)\n\
         制造商: Rust Developer\n\
         系统: {} ({})\n\
         架构: {}\n\
         \n\
         真实数据请在 Android 设备上运行",
        os, family, arch
    )
}
