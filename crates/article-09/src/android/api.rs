// JNI 调用: Rust → Java
//
// 关键: find_class 在原生线程无法找到应用类 (system class loader ≠ app class loader).
// 解决: 在 android_main 主线程中, 用 Activity.getClassLoader().loadClass() 加载应用类,
// 缓存为 Global<JObject>, 后续 new_local_ref 获取局部引用.

use std::sync::OnceLock;

use jni::errors::Result as JniResult;
use jni::objects::{Global, JClass, JObject};
use jni::{jni_str, jni_sig, Env};

use crate::android::{get_activity, with_env};

/// 缓存的 CameraInvoke 类全局引用
static CAMERA_INVOKE_CLASS: OnceLock<Global<JObject>> = std::sync::OnceLock::new();

/// 初始化: 用 ClassLoader 加载应用类并缓存 (必须在主线程调用)
pub fn cache_classes(env: &mut Env) -> JniResult<()> {
    let activity = get_activity(env);
    eprintln!("[article-09] cache_classes: 获取 Activity 成功");

    // 1. 获取 ClassLoader: activity.getClassLoader()
    let loader = env.call_method(
        &activity,
        jni_str!("getClassLoader"),
        jni_sig!(() -> java.lang.ClassLoader),
        &[],
    )?;
    let loader_obj = loader.l()?;
    eprintln!("[article-09] cache_classes: 获取 ClassLoader 成功");

    // 2. 用 ClassLoader.loadClass("com.example.article09.camera.CameraInvoke")
    let class_name = env.new_string("com.example.article09.camera.CameraInvoke")?;
    let cls_obj = env.call_method(
        loader_obj,
        jni_str!("loadClass"),
        jni_sig!((java.lang.String) -> java.lang.Class),
        &[jni::objects::JValue::Object(&class_name)],
    )?;
    eprintln!("[article-09] cache_classes: loadClass 成功");

    // 3. 缓存为全局引用
    let cls_jobject = cls_obj.l()?;
    let global = env.new_global_ref(cls_jobject)?;
    CAMERA_INVOKE_CLASS.set(global).ok();
    eprintln!("[article-09] cache_classes: 缓存完成");
    Ok(())
}

/// CameraInvoke API 封装
pub struct CameraInvokeApi;

impl CameraInvokeApi {
    /// 打开相机
    pub fn open() -> JniResult<()> {
        eprintln!("[article-09] CameraInvokeApi::open() 开始");
        let result = with_env(|env| {
            let cls = get_invoke_class(env)?;
            eprintln!("[article-09] 获取 CameraInvoke 类成功");
            env.call_static_method(
                &cls,
                jni_str!("open"),
                jni_sig!(() -> void),
                &[],
            )?;
            eprintln!("[article-09] CameraInvoke.open() 调用成功");
            Ok(())
        });
        match &result {
            Ok(_) => eprintln!("[article-09] CameraInvokeApi::open() 完成"),
            Err(e) => eprintln!("[article-09] CameraInvokeApi::open() 错误: {}", e),
        }
        result
    }

    /// 切换前后摄
    pub fn toggle() -> JniResult<()> {
        with_env(|env| {
            let cls = get_invoke_class(env)?;
            env.call_static_method(&cls, jni_str!("toggle"), jni_sig!(() -> void), &[])?;
            Ok(())
        })
    }

    /// 关闭相机
    pub fn close() -> JniResult<()> {
        with_env(|env| {
            let cls = get_invoke_class(env)?;
            env.call_static_method(&cls, jni_str!("close"), jni_sig!(() -> void), &[])?;
            Ok(())
        })
    }
}

/// 获取 CameraInvoke 类引用 (从缓存的 Global 创建局部引用)
fn get_invoke_class<'a>(env: &mut Env<'a>) -> JniResult<JClass<'a>> {
    let global = CAMERA_INVOKE_CLASS.get()
        .ok_or_else(|| jni::errors::Error::JniCall(jni::errors::JniError::Other(-1)))?;
    let local = env.new_local_ref(global.as_obj())?;
    env.cast_local::<JClass>(local)
        .map_err(|_| jni::errors::Error::JniCall(jni::errors::JniError::Other(-2)))
}

