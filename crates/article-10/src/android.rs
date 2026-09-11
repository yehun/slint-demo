//! Android 端剪贴板服务
//!
//! 通过 JNI 调用 Android 系统的 ClipboardManager:
//! - 写: ClipboardManager.setPrimaryClip(ClipData.newPlainText(...))
//! - 读: ClipboardManager.getPrimaryClip().getItemAt(0).getText()
//!
//! 注意: Android 剪贴板操作必须在主线程 (UI 线程) 执行.

use jni::errors::Result as JniResult;
use jni::objects::{Global, JClass, JObject, JString, JValue};
use jni::{jni_str, jni_sig, Env, JavaVM};
use slint::android::AndroidApp;
use std::sync::OnceLock;

use crate::service::ClipboardService;

// ===== 全局状态 =====

static ANDROID_APP: OnceLock<AndroidApp> = OnceLock::new();
static JVM: OnceLock<JavaVM> = OnceLock::new();
static CLIPBOARD_CLASS: OnceLock<Global<JObject>> = OnceLock::new();

/// 从 AndroidApp 初始化全局引用
pub fn init(app: AndroidApp) {
    ANDROID_APP.set(app).expect("AndroidApp 已初始化");
}

fn android_app() -> &'static AndroidApp {
    ANDROID_APP.get().expect("AndroidApp 未初始化")
}

/// 获取 JavaVM 指针
fn get_java_vm() -> &'static JavaVM {
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
fn get_activity<'local>(env: &mut Env<'local>) -> JObject<'local> {
    unsafe { JObject::from_raw(env, android_app().activity_as_ptr() as *mut _) }
}

/// 缓存 Java 类 (必须在主线程调用)
pub fn cache_classes(env: &mut Env) -> JniResult<()> {
    let activity = get_activity(env);

    // 1. 获取 ClassLoader: activity.getClassLoader()
    let loader = env.call_method(
        &activity,
        jni_str!("getClassLoader"),
        jni_sig!(() -> java.lang.ClassLoader),
        &[],
    )?;
    let loader_obj = loader.l()?;

    // 2. 用 ClassLoader.loadClass("com.example.article10.clipboard.ClipboardHelper")
    let class_name = env.new_string("com.example.article10.clipboard.ClipboardHelper")?;
    let cls_obj = env.call_method(
        loader_obj,
        jni_str!("loadClass"),
        jni_sig!((java.lang.String) -> java.lang.Class),
        &[JValue::Object(&class_name)],
    )?;

    // 3. 缓存为全局引用
    let cls_jobject = cls_obj.l()?;
    let global = env.new_global_ref(cls_jobject)?;
    CLIPBOARD_CLASS.set(global).ok();

    Ok(())
}

/// 获取 ClipboardHelper 类引用 (从缓存的 Global 创建局部引用)
fn get_clipboard_class<'local>(env: &mut Env<'local>) -> JniResult<JClass<'local>> {
    let global = CLIPBOARD_CLASS.get()
        .ok_or_else(|| jni::errors::Error::JniCall(jni::errors::JniError::Other(-1)))?;
    let local = env.new_local_ref(global.as_obj())?;
    env.cast_local::<JClass>(local)
        .map_err(|_| jni::errors::Error::JniCall(jni::errors::JniError::Other(-2)))
}

// ===== ClipboardService 实现 =====

/// Android 端剪贴板服务
pub struct PlatformClipboardService;

impl PlatformClipboardService {
    pub fn new() -> Self {
        Self
    }
}

impl ClipboardService for PlatformClipboardService {
    fn get_text(&self) -> anyhow::Result<String> {
        with_env(|env| {
            let cls = get_clipboard_class(env)?;
            let activity = get_activity(env);

            // ClipboardHelper.getTextStatic(activity) -> String
            let result = env.call_static_method(
                &cls,
                jni_str!("getTextStatic"),
                jni_sig!((android.content.Context) -> java.lang.String),
                &[JValue::Object(&activity)],
            )?;

            let jstr_obj = result.l()?;
            if jstr_obj.is_null() {
                return Ok(String::new());
            }
            let jstr = env.cast_local::<JString>(jstr_obj)?;

            Ok(jstr.to_string())
        })
        .map_err(|e| anyhow::anyhow!("JNI get_text 错误: {e}"))
    }

    fn set_text(&self, text: &str) -> anyhow::Result<()> {
        with_env(|env| {
            let cls = get_clipboard_class(env)?;
            let activity = get_activity(env);
            let text_jstring = env.new_string(text)?;

            // ClipboardHelper.setTextStatic(activity, text)
            env.call_static_method(
                &cls,
                jni_str!("setTextStatic"),
                jni_sig!((android.content.Context, java.lang.String) -> void),
                &[JValue::Object(&activity), JValue::Object(&text_jstring)],
            )?;

            Ok(())
        })
        .map_err(|e| anyhow::anyhow!("JNI set_text 错误: {e}"))
    }
}
