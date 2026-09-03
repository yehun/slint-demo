// Android 设备信息获取 - 使用 bind_java_type! 绑定 Build 静态字段

use crate::android::bindings::{Build, BuildVersion};
use jni::objects::JString;

/// 获取 Android 设备信息
pub fn get_device_info() -> String {
    super::with_env(|env| {
        // bind_java_type! 生成的静态字段访问
        let model = Build::MODEL(env)?;
        let model: String = env.cast_local::<JString>(model)?.to_string();

        let mfr = Build::MANUFACTURER(env)?;
        let mfr: String = env.cast_local::<JString>(mfr)?.to_string();

        let rel = BuildVersion::RELEASE(env)?;
        let rel: String = env.cast_local::<JString>(rel)?.to_string();

        let sdk = BuildVersion::SDK_INT(env)?;

        Ok(format!(
            "Android 设备信息\n型号: {}\n制造商: {}\n版本: {} (API {})",
            model, mfr, rel, sdk
        ))
    }).unwrap_or_else(|e| {
        eprintln!("[article-05] get_device_info JNI error: {}", e);
        format!("JNI 错误: {}", e)
    })
}
