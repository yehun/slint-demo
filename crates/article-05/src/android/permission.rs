// Android 权限管理 - 使用jni_str!/jni_sig! 宏
// 注意: 所有 JNI 调用必须在主线程 (有 Looper 的线程) 执行

use jni::{jni_str, jni_sig, objects::{JObject, JValue}};

/// 检查权限是否已授予
pub fn check_permission(permission: &str) -> bool {
    super::with_env(|env| {
        let ctx = super::get_context(env);
        let j_permission = env.new_string(permission)?;

        // PackageManager.PERMISSION_GRANTED = 0
        let result = env.call_method(
            &ctx,
            jni_str!("checkSelfPermission"),
            jni_sig!((java.lang.String) -> int),
            &[JValue::Object(&j_permission)],
        )?.i()?;

        Ok(result == 0) // PERMISSION_GRANTED = 0
    }).unwrap_or(false)
}

/// 请求权限
pub fn request_permissions(permissions: &[&str]) -> bool {
    if permissions.is_empty() {
        return false;
    }

    super::with_env(|env| {
        let ctx = super::get_context(env);

        // 创建字符串数组
        let permissions_array = env.new_object_array(
            permissions.len() as i32,
            jni_str!("java/lang/String"),
            JObject::null(),
        )?;

        for (i, perm) in permissions.iter().enumerate() {
            let jstr = env.new_string(perm)?;
            permissions_array.set_element(env, i, jstr)?;
        }

        // 请求权限 (REQUEST_CODE = 1001)
        env.call_method(
            &ctx,
            jni_str!("requestPermissions"),
            jni_sig!((java.lang.String[], int) -> void),
            &[JValue::Object(&permissions_array), JValue::Int(1001)],
        )?;

        Ok(true)
    }).unwrap_or(false)
}

/// 检查并请求所有必要权限
/// 警告: 此函数必须在主线程 (有 Looper) 调用
pub fn ensure_permissions() {
    let permissions = [
        "android.permission.ACCESS_NETWORK_STATE",
        "android.permission.INTERNET",
        "android.permission.ACCESS_WIFI_STATE",
    ];

    let mut need_request = false;
    for perm in &permissions {
        if !check_permission(perm) {
            need_request = true;
            break;
        }
    }

    if need_request {
        eprintln!("[article-05] 请求运行时权限...");
        request_permissions(&permissions);
    } else {
        eprintln!("[article-05] 所有必要权限已授予");
    }
}
