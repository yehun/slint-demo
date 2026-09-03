// Desktop 平台模拟实现

mod device;
mod battery;
mod network;
mod toast;

pub use device::get_device_info;
pub use battery::get_battery_info;
pub use network::get_network_info;
pub use toast::show_toast;