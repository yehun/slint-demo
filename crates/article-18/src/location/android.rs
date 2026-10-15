// 第十八篇(下), 定位体系 —— Android 实现(JNI 双向桥)
//
// 链路:
//
// ```text
// Rust                                     Java
// ─────────────────────────────────────────────────────────────────
// open(callback)
//   └─ LocationInvoke.open(activity) ────► 权限检查
//                                            ├─ 已授权: requestLocationUpdates
//                                            │   (GPS/NETWORK/PASSIVE, 1s, 主线程 Looper)
//                                            └─ 未授权: requestPermissions →
//                                               MainActivity.onRequestPermissionsResult
//                                               → LocationInvoke.onPermissionResult → 注册监听
// 定位变化(主线程 Looper)
//   └─ callback ◄──────────────────────── LocationCallback.onLocationChanged (static native)
//
// close()
//   └─ LocationInvoke.close(activity) ───► removeUpdates
// ```
//
// 两个 JNI 关键点(与第 9 幕摄像头同款):
// 1. 应用类(包名前缀 com.example.article18)不能在原生线程用 find_class 找
//    —— 那走的是 system classloader, 找不到 app 类。必须在 android_main 主线程
//    用 Activity.getClassLoader().loadClass() 加载并缓存成全局引用。
// 2. Java→Rust 的回调导出为 `Java_<类路径>_<方法名>` C 符号, 类路径改了要
//    与 Java 侧同步。

use std::sync::OnceLock;

use parking_lot::Mutex;

use jni::objects::{Global, JClass, JObject, JString, JValue};
use jni::{jni_str, jni_sig, Env, EnvUnowned, JavaVM};
use slint::android::AndroidApp;

use super::{Location, LocationCallback};

static ANDROID_APP: OnceLock<AndroidApp> = OnceLock::new();
static LOCATION_INVOKE_CLASS: OnceLock<Global<JObject>> = OnceLock::new();
/// 定位回调(open 时注册, close 时取走)。单消费者, 一格即够。
static CALLBACK: OnceLock<Mutex<Option<LocationCallback>>> = OnceLock::new();

fn callback_slot() -> &'static Mutex<Option<LocationCallback>> {
    CALLBACK.get_or_init(|| Mutex::new(None))
}

/// 保存 AndroidApp(JNI 一切调用的根), 在 android_main 里、slint::android::init 前调用。
pub fn init(app: AndroidApp) {
    let _ = ANDROID_APP.set(app);
}

fn android_app() -> &'static AndroidApp {
    ANDROID_APP.get().expect("AndroidApp 未初始化(需先调用 location::init)")
}

fn with_env<F, T>(f: F) -> jni::errors::Result<T>
where
    F: FnOnce(&mut Env) -> jni::errors::Result<T>,
{
    static JVM: OnceLock<JavaVM> = OnceLock::new();
    let vm = JVM.get_or_init(|| unsafe {
        JavaVM::from_raw(android_app().vm_as_ptr() as *mut _)
    });
    vm.attach_current_thread(f)
}

/// 主线程里预取应用类缓存: find_class 在原生线程找不到 app 类(见模块注释)。
/// 必须在 android_main 主线程调用。
pub fn cache_classes() -> jni::errors::Result<()> {
    with_env(|env| cache_classes_with(env))
}

fn cache_classes_with(env: &mut Env) -> jni::errors::Result<()> {
    let activity = unsafe { JObject::from_raw(env, android_app().activity_as_ptr() as *mut _) };
    let loader = env
        .call_method(
            &activity,
            jni_str!("getClassLoader"),
            jni_sig!(() -> java.lang.ClassLoader),
            &[],
        )?
        .l()?;
    let name = env.new_string("com.example.article18.location.LocationInvoke")?;
    let cls = env
        .call_method(
            &loader,
            jni_str!("loadClass"),
            jni_sig!((java.lang.String) -> java.lang.Class),
            &[JValue::Object(&name)],
        )?
        .l()?;
    LOCATION_INVOKE_CLASS.set(env.new_global_ref(cls)?).ok();
    Ok(())
}

fn invoke_class<'a>(env: &mut Env<'a>) -> jni::errors::Result<JClass<'a>> {
    let global = LOCATION_INVOKE_CLASS
        .get()
        .ok_or(jni::errors::Error::JniCall(jni::errors::JniError::Other(-1)))?;
    let local = env.new_local_ref(global.as_obj())?;
    env.cast_local::<JClass>(local)
        .map_err(|_| jni::errors::Error::JniCall(jni::errors::JniError::Other(-2)))
}

fn with_invoke<T>(
    what: &str,
    f: impl FnOnce(&mut Env, JClass) -> jni::errors::Result<T>,
) -> Option<T> {
    with_env(|env| {
        let cls = invoke_class(env)?;
        f(env, cls)
    })
    .map_err(|e| eprintln!("[location] {what} 失败: {e}"))
    .ok()
}

/// Android 定位服务: 全部能力来自系统 LocationManager, Java 侧封装, Rust 经 JNI 调。
pub struct PlatformLocationService;

impl PlatformLocationService {
    pub fn new() -> Self {
        Self
    }

    /// 系统定位总开关(GPS/网络/被动任一 provider 可用即视为可用)。
    /// 关着时注册监听也永远等不到回调, 应先提示用户去开系统定位。
    pub fn enabled(&self) -> bool {
        with_invoke("LocationInvoke.enabled", |env, cls| {
            let activity =
                unsafe { JObject::from_raw(env, android_app().activity_as_ptr() as *mut _) };
            let enabled = env.call_static_method(
                &cls,
                jni_str!("enabled"),
                jni_sig!((android.content.Context) -> boolean),
                &[JValue::Object(&activity)],
            )?;
            Ok(enabled.z()?)
        })
        .unwrap_or(true) // JNI 失败不拦路, 让 open 去试
    }

    /// 开启定位。`callback` 在 Java 定位线程上被调(内部再回投 UI 线程)。
    pub fn open(&self, callback: LocationCallback) -> bool {
        *callback_slot().lock() = Some(callback);
        let ok = with_invoke("LocationInvoke.open", |env, cls| {
            let activity =
                unsafe { JObject::from_raw(env, android_app().activity_as_ptr() as *mut _) };
            env.call_static_method(
                &cls,
                jni_str!("open"),
                jni_sig!((android.content.Context) -> void),
                &[JValue::Object(&activity)],
            )?;
            Ok(())
        })
        .is_some();
        if !ok {
            // JNI 失败: 别把回调锁在格子里
            *callback_slot().lock() = None;
        }
        ok
    }

    pub fn close(&self) {
        *callback_slot().lock() = None;
        let _ = with_invoke("LocationInvoke.close", |env, cls| {
            let activity =
                unsafe { JObject::from_raw(env, android_app().activity_as_ptr() as *mut _) };
            env.call_static_method(
                &cls,
                jni_str!("close"),
                jni_sig!((android.content.Context) -> void),
                &[JValue::Object(&activity)],
            )?;
            Ok(())
        });
    }
}

/// JNI 回调: Java 定位线程 → Rust。
///
/// 对应 Java: com.example.article18.location.LocationCallback.onLocationChanged
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub fn Java_com_example_article18_location_LocationCallback_onLocationChanged(
    mut env: EnvUnowned,
    _class: JClass,
    provider: JString,
    latitude: f64,
    longitude: f64,
    accuracy: f32,
) {
    let _ = env.with_env(|env| {
        let provider: String = provider.try_to_string(env)?;
        // Box<dyn Fn> 不可克隆: 持锁直调, 不取出
        let guard = callback_slot().lock();
        if let Some(callback) = guard.as_ref() {
            callback(Location {
                provider,
                latitude,
                longitude,
                accuracy,
            });
        }
        Ok::<(), jni::errors::Error>(())
    });
}
