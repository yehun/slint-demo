// Android 电池信息获取

use jni::{jni_str, jni_sig, objects::{JObject, JValue}};

/// 获取 Android 电池信息
pub fn get_battery_info() -> String {
    super::with_env(|env| {
        let ctx = super::get_context(env);

        // 1. 获取 BatteryManager (Context.getSystemService)
        let name = env.new_string("batterymanager")?;
        let bm = env.call_method(&ctx,
            jni_str!("getSystemService"),
            jni_sig!((java.lang.String) -> java.lang.Object),
            &[JValue::Object(&name)],
        )?.l()?;

        // 2. 获取电量 (BatteryManager.getIntProperty(BATTERY_PROPERTY_CAPACITY=4))
        let level = env.call_method(&bm,
            jni_str!("getIntProperty"),
            jni_sig!((int) -> int),
            &[JValue::Int(4)],
        )?.i()?;

        // 3. 获取充电状态 (registerReceiver sticky broadcast)
        let action = env.new_string("android.intent.action.BATTERY_CHANGED")?;
        let if_class = env.find_class(jni_str!("android/content/IntentFilter"))?;
        let if_obj = env.new_object(&if_class,
            jni_sig!((java.lang.String) -> void),
            &[JValue::Object(&action)],
        )?;

        let intent = env.call_method(&ctx,
            jni_str!("registerReceiver"),
            jni_sig!((android.content.BroadcastReceiver, android.content.IntentFilter) -> android.content.Intent),
            &[JValue::Object(&JObject::null()), JValue::Object(&if_obj)],
        )?.l()?;

        let status: String = if intent.is_null() {
            "未知".into()
        } else {
            let sn = env.new_string("status")?;
            let s = env.call_method(&intent,
                jni_str!("getIntExtra"),
                jni_sig!((java.lang.String, int) -> int),
                &[JValue::Object(&sn), JValue::Int(-1)],
            )?.i()?;
            match s { 2 => "充电中", 5 => "已充满", _ => "未充电" }.to_string()
        };

        Ok(format!("Android 电池信息\n电量: {}%\n状态: {}", level, status))
    }).unwrap_or_else(|e| {
        eprintln!("[article-05] get_battery_info JNI error: {}", e);
        format!("JNI 错误: {}", e)
    })
}
