// Android 平台实现 — 摄像头模块
//
// 核心知识点:
// 1. JNI 双向通路: Rust→Java (CameraInvoke) + Java→Rust (CameraCallback)
// 2. 全局回调存储: 帧/状态回调注册与分发
// 3. #[unsafe(no_mangle)]: 导出 C 符号供 Java native 方法调用
// 4. EnvUnowned::with_env: 在任意线程获取 JNI 环境

use jni::objects::JObject;
use jni::{Env, JavaVM};
use slint::android::AndroidApp;
use std::sync::OnceLock;

/// JNI 回调模块 (Java → Rust)
mod callback;

/// JNI 调用模块 (Rust → Java)
mod api;

/// Android 平台相机服务实现
mod platform;

// 导出类型
pub use api::{CameraInvokeApi, cache_classes};
pub use platform::PlatformCameraService;

use jni::errors::Result as JniResult;

static ANDROID_APP: OnceLock<AndroidApp> = OnceLock::new();

/// 初始化 AndroidApp 引用 (在 android_main 中调用)
pub fn init(app: AndroidApp) {
    ANDROID_APP.set(app).expect("AndroidApp 已初始化");
}

fn android_app() -> &'static AndroidApp {
    ANDROID_APP.get().expect("AndroidApp 未初始化, 是否在 android_main 中调用了 init()?")
}

/// 获取 JavaVM 指针
fn get_java_vm() -> &'static JavaVM {
    static JVM: OnceLock<JavaVM> = OnceLock::new();
    JVM.get_or_init(|| unsafe {
        JavaVM::from_raw(android_app().vm_as_ptr() as *mut _)
    })
}

/// 执行 JNI 操作 (自动处理线程 attach)
pub fn with_env<F, T>(callback: F) -> JniResult<T>
where
    F: FnOnce(&mut Env) -> JniResult<T>,
{
    let vm = get_java_vm();
    vm.attach_current_thread(callback)
}

/// 获取当前 Activity 作为 JObject
pub fn get_activity<'local>(env: &mut Env<'local>) -> JObject<'local> {
    unsafe { JObject::from_raw(env, android_app().activity_as_ptr() as *mut _) }
}
