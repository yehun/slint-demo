// Android 网络信息获取

use jni::{jni_str, jni_sig, objects::{JString, JValue}};

/// 获取 Android 网络信息
pub fn get_network_info() -> String {
    super::with_env(|env| {
        let ctx = super::get_context(env);

        // 1. 获取 ConnectivityManager
        let cn = env.new_string("connectivity")?;
        let cm = env.call_method(&ctx,
            jni_str!("getSystemService"),
            jni_sig!((java.lang.String) -> java.lang.Object),
            &[JValue::Object(&cn)],
        )?.l()?;

        // 2. 获取活动网络信息
        let an = env.call_method(&cm,
            jni_str!("getActiveNetworkInfo"),
            jni_sig!(() -> android.net.NetworkInfo),
            &[],
        )?.l()?;

        if an.is_null() {
            return Ok("Android 网络信息\n状态: 无网络连接".into());
        }

        // 3. 检查是否连接
        let ic = env.call_method(&an,
            jni_str!("isConnected"),
            jni_sig!(() -> boolean),
            &[],
        )?.z()?;
        if !ic {
            return Ok("Android 网络信息\n状态: 网络未连接".into());
        }

        // 4. 获取网络类型名称
        let tn = env.call_method(&an,
            jni_str!("getTypeName"),
            jni_sig!(() -> java.lang.String),
            &[],
        )?.l()?;
        let jstr = env.cast_local::<JString>(tn)?;
        let ts = jstr.to_string();
        let label = match ts.as_str() {
            "WIFI" => "WiFi",
            "MOBILE" => "移动数据",
            _ => &ts,
        };

        Ok(format!("Android 网络信息\n类型: {}\n状态: 已连接", label))
    }).unwrap_or_else(|e| {
        eprintln!("[article-05] get_network_info JNI error: {}", e);
        format!("JNI 错误: {}", e)
    })
}
