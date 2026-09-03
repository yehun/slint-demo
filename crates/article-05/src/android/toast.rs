// Android Toast 弹出 - 通过 Activity.showToast() 在主线程执行
// Toast 构造需要 Looper, 必须通过 runOnUiThread 在主线程调用

use jni::{jni_str, jni_sig, objects::JValue};

/// 弹出 Android Toast
/// 调用 Activity.showToast(String) → 内部 runOnUiThread → Toast.makeText().show()
pub fn show_toast(message: &str) -> String {
    super::with_env(|env| {
        let ctx = super::get_context(env);
        let j_message = env.new_string(message)?;

        // 调用 MainActivity.showToast(String)
        env.call_method(
            &ctx,
            jni_str!("showToast"),
            jni_sig!((java.lang.String) -> void),
            &[JValue::Object(&j_message)],
        )?;

        Ok(format!("Toast 已弹出: {}", message))
    }).unwrap_or_else(|e| {
        eprintln!("[article-05] show_toast JNI error: {}", e);
        format!("Toast 失败: {}", e)
    })
}
