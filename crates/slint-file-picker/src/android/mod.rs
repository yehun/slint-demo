// Android 实现: SAF (Storage Access Framework) 文件选择.
//
// 流程: Rust 经 JNI 调 Java 侧 SlintFilePicker.open/save 发起系统对话框 →
// demo 的 MainActivity.onActivityResult 转发给 SlintFilePicker.handle →
// static native onPicked 回到 Rust (下方 no_mangle 导出) → channel →
// 等待线程收到后执行用户回调.
//
// 回调在后台线程执行 (不能在主线程阻塞等待: onActivityResult 也要主线程).

use std::sync::{mpsc, LazyLock, Mutex, OnceLock};

use jni::objects::{Global, JClass, JObject, JString, JValue};
use jni::{jni_sig, jni_str, Env, EnvUnowned, JavaVM};

use slint_fs::PlatformPath;

use crate::{FileFilter, PickResult};

// ============================================================
// JNI 底座 (与 slint-fs 同一套模式)
// ============================================================

static JVM: OnceLock<JavaVM> = OnceLock::new();

fn java_vm() -> &'static JavaVM {
    JVM.get_or_init(|| {
        let vm_ptr = ndk_context::android_context().vm();
        unsafe { JavaVM::from_raw(vm_ptr as *mut _) }
    })
}

fn with_env_jni<F, T>(callback: F) -> jni::errors::Result<T>
where
    F: FnOnce(&mut Env) -> jni::errors::Result<T>,
{
    java_vm().attach_current_thread(callback)
}

/// 当前 Activity (android-activity 的全局引用即运行中的 Activity)
fn activity<'a>(env: &mut Env<'a>) -> jni::errors::Result<JObject<'a>> {
    let ptr = ndk_context::android_context().context();
    Ok(unsafe { JObject::from_raw(env, ptr as *mut _) })
}

// ============================================================
// 结果通道: native 回调 → 等待线程
// ============================================================

enum PickerEvent {
    Picked(String),
    Cancelled,
    Error(String),
}

static EVENTS: LazyLock<(
    mpsc::Sender<PickerEvent>,
    Mutex<mpsc::Receiver<PickerEvent>>,
)> = LazyLock::new(|| {
    let (tx, rx) = mpsc::channel();
    (tx, Mutex::new(rx))
});

// ============================================================
// Java 侧类引用缓存 (应用类必须经 ClassLoader 加载, 见第五幕)
// ============================================================

static PICKER_CLASS: OnceLock<Global<JObject>> = OnceLock::new();
const PICKER_CLASS_NAME: &str = "com.yehun.slintfs.SlintFilePicker";

fn ensure_picker_class<'a>(
    env: &mut Env<'a>,
) -> jni::errors::Result<&'static Global<JObject<'static>>> {
    if let Some(global) = PICKER_CLASS.get() {
        return Ok(global);
    }
    let activity = activity(env)?;
    let loader = env.call_method(
        &activity,
        jni_str!("getClassLoader"),
        jni_sig!(() -> java.lang.ClassLoader),
        &[],
    )?;
    let loader = loader.l()?;
    let name = env.new_string(PICKER_CLASS_NAME)?;
    let cls = env.call_method(
        &loader,
        jni_str!("loadClass"),
        jni_sig!((java.lang.String) -> java.lang.Class),
        &[JValue::Object(&name)],
    )?;
    let cls = cls.l()?;
    let global = env.new_global_ref(cls)?;
    let _ = PICKER_CLASS.set(global);
    Ok(PICKER_CLASS.get().expect("picker class just set"))
}

// ============================================================
// 发起: 调 Java 侧 open / save
// ============================================================

/// 取缓存的 picker 类局部引用 (Global → local → JClass)
fn picker_class<'a>(env: &mut Env<'a>) -> jni::errors::Result<JClass<'a>> {
    let global = ensure_picker_class(env)?;
    let local = env.new_local_ref(global.as_obj())?;
    env.cast_local::<JClass>(local)
}

fn invoke_open(mime_types: &[String]) -> Result<(), String> {
    with_env_jni(|env| {
        let cls = picker_class(env)?;
        let activity = activity(env)?;
        let str_cls: jni::objects::JClass = env.find_class(jni_str!("java/lang/String"))?;
        let j_mimes = env.new_object_array(mime_types.len() as i32, &str_cls, &JString::null())?;
        for (i, mime) in mime_types.iter().enumerate() {
            let j_mime = env.new_string(mime)?;
            j_mimes.set_element(env, i, &j_mime)?;
        }
        env.call_static_method(
            &cls,
            jni_str!("open"),
            jni_sig!((android.app.Activity, [java.lang.String]) -> void),
            &[JValue::Object(&activity), JValue::Object(&j_mimes)],
        )?;
        Ok(())
    }).map_err(|e| e.to_string())
}

fn invoke_save(default_name: &str, mime_types: &[String]) -> Result<(), String> {
    with_env_jni(|env| {
        let cls = picker_class(env)?;
        let activity = activity(env)?;
        let mime = mime_types.first().map(|s| s.as_str()).unwrap_or("*/*");
        let j_mime = env.new_string(mime)?;
        let j_name = env.new_string(default_name)?;
        env.call_static_method(
            &cls,
            jni_str!("save"),
            jni_sig!((android.app.Activity, java.lang.String, java.lang.String) -> void),
            &[JValue::Object(&activity), JValue::Object(&j_mime), JValue::Object(&j_name)],
        )?;
        Ok(())
    }).map_err(|e| e.to_string())
}

// ============================================================
// 等待线程: 收事件 → 用户回调
// ============================================================

fn wait_and_notify(callback: impl FnOnce(PickResult) + Send + 'static) {
    std::thread::spawn(move || {
        let event = EVENTS.1.lock().expect("picker event rx").recv();
        callback(match event {
            Ok(PickerEvent::Picked(uri)) => PickResult::Picked(PlatformPath::new(&uri)),
            Ok(PickerEvent::Cancelled) => PickResult::Cancelled,
            Ok(PickerEvent::Error(e)) => PickResult::Error(e),
            Err(e) => PickResult::Error(e.to_string()),
        });
    });
}

/// 打开文件选择对话框 (选择已有文件)
pub fn pick_file(filters: Vec<FileFilter>, callback: impl FnOnce(PickResult) + Send + 'static) {
    let mime_types: Vec<String> = filters
        .iter()
        .flat_map(|f| f.mime_types.iter().cloned())
        .collect();
    if let Err(e) = invoke_open(&mime_types) {
        callback(PickResult::Error(e));
        return;
    }
    wait_and_notify(callback);
}

/// 打开保存文件对话框 (创建或覆盖)
pub fn pick_file_to_save(
    default_name: impl Into<String>,
    filters: Vec<FileFilter>,
    callback: impl FnOnce(PickResult) + Send + 'static,
) {
    let mime_types: Vec<String> = filters
        .iter()
        .flat_map(|f| f.mime_types.iter().cloned())
        .collect();
    if let Err(e) = invoke_save(&default_name.into(), &mime_types) {
        callback(PickResult::Error(e));
        return;
    }
    wait_and_notify(callback);
}

/// 打开文件选择对话框 (多选) — Android 暂不支持, 直接返回空
pub fn pick_files(
    _filters: Vec<FileFilter>,
    callback: impl FnOnce(Vec<slint_fs::PlatformPath>) + Send + 'static,
) {
    callback(Vec::new());
}

// ============================================================
// JNI 回调: Java → Rust (SlintFilePicker.handle 转发而来)
// ============================================================

/// 对应 Java: com.yehun.slintfs.SlintFilePicker.onPicked(int, int, String)
/// resultCode: -1 = RESULT_OK, 0 = RESULT_CANCELED
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub fn Java_com_yehun_slintfs_SlintFilePicker_onPicked(
    mut env_unowned: EnvUnowned,
    _class: JClass,
    _request_code: i32,
    result_code: i32,
    uri: JString,
) {
    let _ = env_unowned.with_env(|env| {
        let uri = if uri.is_null() {
            None
        } else {
            let uri = env.cast_local::<JString>(uri)?;
            Some(uri.to_string())
        };
        let event = match (result_code, uri) {
            (-1, Some(uri)) => PickerEvent::Picked(uri),
            (-1, None) => PickerEvent::Error("RESULT_OK 但未返回 URI".to_string()),
            (0, _) => PickerEvent::Cancelled,
            (code, _) => PickerEvent::Error(format!("文件选择失败 (resultCode={code})")),
        };
        let _ = EVENTS.0.send(event);
        Ok::<(), jni::errors::Error>(())
    });
}
