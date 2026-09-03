// Android 平台实现 - 使用 bind_java_type! 类型安全绑定

mod bindings;  // bind_java_type! 类型绑定
mod device;
mod battery;
mod network;
mod toast;
pub mod permission;

pub use device::get_device_info;
pub use battery::get_battery_info;
pub use network::get_network_info;
pub use toast::show_toast;

use jni::objects::JObject;
use jni::{Env, JavaVM};
use slint::android::AndroidApp;
use std::sync::OnceLock;

static ANDROID_APP: OnceLock<AndroidApp> = OnceLock::new();

pub fn init(app: AndroidApp) {
    ANDROID_APP.set(app).expect("AndroidApp already initialized");
}

fn android_app() -> &'static AndroidApp {
    ANDROID_APP.get().expect("AndroidApp not initialized - was android_main() called?")
}

fn get_java_vm() -> &'static JavaVM {
    static JVM: OnceLock<JavaVM> = OnceLock::new();
    JVM.get_or_init(|| {
        let app = android_app();
        unsafe { JavaVM::from_raw(app.vm_as_ptr() as *mut _) }
    })
}

/// 执行 JNI 操作
/// 使用 attach_current_thread 闭包模式, 自动处理已附加/未附加线程
pub fn with_env<F, T>(callback: F) -> jni::errors::Result<T>
where
    F: FnOnce(&mut Env) -> jni::errors::Result<T>,
{
    let vm = get_java_vm();
    vm.attach_current_thread(callback)
}

/// 获取当前 Activity 作为 Context (用于 Android API 调用)
pub fn get_context<'local>(env: &mut Env<'local>) -> JObject<'local> {
    let app = android_app();
    unsafe { JObject::from_raw(env, app.activity_as_ptr() as *mut _) }
}
