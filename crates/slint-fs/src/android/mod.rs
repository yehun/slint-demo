// Android 平台实现: PlatformPath 的 content URI 侧.
//
// 底座: android-activity 在 android_main 启动时会初始化 ndk-context,
// 因此本模块无需显式 init, 任意线程经 with_env 即可拿 (JavaVM, Env).
//
// 全部 JNI 调用手写 (不依赖任何生成层), 与系列文章第五幕的讲解保持同一风格:
// 类引用走 find_class, 方法/字段签名走 jni_sig! 编译期校验.

use jni::errors::Result as JniResult;
use jni::objects::{JClass, JObject, JString, JValue};
use jni::{jni_sig, jni_str, Env, JavaVM};
use std::sync::OnceLock;

use crate::error::{Error, Result};
use crate::file_type::PlatformFileFormat;

mod media;

pub use media::{
    jni_content_exists,
    jni_content_join,
    jni_content_parent,
};

static JVM: OnceLock<JavaVM> = OnceLock::new();

fn java_vm() -> &'static JavaVM {
    JVM.get_or_init(|| {
        let vm_ptr = ndk_context::android_context().vm();
        unsafe { JavaVM::from_raw(vm_ptr as *mut _) }
    })
}

/// 在任意线程执行 JNI 操作 (自动 attach), 错误统一收敛到 crate::Error
pub(crate) fn with_env<F, T>(callback: F) -> Result<T>
where
    F: FnOnce(&mut Env) -> Result<T>,
{
    let mut captured: Option<Error> = None;
    let jni_result = java_vm().attach_current_thread(|env| {
        match callback(env) {
            Ok(value) => Ok(value),
            Err(e) => {
                captured = Some(e);
                Err(jni::errors::Error::JniCall(jni::errors::JniError::Other(-100)))
            }
        }
    });
    match (jni_result, captured.take()) {
        (_, Some(e)) => Err(e),
        (Ok(value), None) => Ok(value),
        (Err(e), None) => Err(Error::Jni(e)),
    }
}

/// 当前应用 Context (android-activity 的全局引用)
fn context<'a>(env: &mut Env<'a>) -> JniResult<JObject<'a>> {
    let ptr = ndk_context::android_context().context();
    Ok(unsafe { JObject::from_raw(env, ptr as *mut _) })
}

/// context.getContentResolver()
fn content_resolver<'a>(env: &mut Env<'a>) -> JniResult<JObject<'a>> {
    let ctx = context(env)?;
    let resolver = env.call_method(
        &ctx,
        jni_str!("getContentResolver"),
        jni_sig!(() -> android.content.ContentResolver),
        &[],
    )?;
    Ok(resolver.l()?)
}

/// Uri.parse(s)
fn parse_uri<'a>(env: &mut Env<'a>, uri: &str) -> JniResult<JObject<'a>> {
    let cls: JClass<'a> = env.find_class(jni_str!("android/net/Uri"))?;
    let j_uri_str = env.new_string(uri)?;
    let parsed = env.call_static_method(
        &cls,
        jni_str!("parse"),
        jni_sig!((java.lang.String) -> android.net.Uri),
        &[JValue::Object(&j_uri_str)],
    )?;
    Ok(parsed.l()?)
}

/// 读取系统类上的 static String 常量 (如 MediaStore$MediaColumns.DISPLAY_NAME)
fn static_string_field<'a>(
    env: &mut Env<'a>,
    class_path: &jni::strings::JNIStr,
    field: &jni::strings::JNIStr,
) -> JniResult<JObject<'a>> {
    let cls: JClass<'a> = env.find_class(class_path)?;
    let value = env.get_static_field(&cls, field, jni_sig!(java.lang.String))?;
    Ok(value.l()?)
}

/// ContentValues.put(key, value) — key 来自系统类常量
fn put_string_column(
    env: &mut Env,
    values: &JObject,
    column_class: &jni::strings::JNIStr,
    column_field: &jni::strings::JNIStr,
    value: &str,
) -> JniResult<()> {
    let key = static_string_field(env, column_class, column_field)?;
    let val = env.new_string(value)?;
    env.call_method(
        values,
        jni_str!("put"),
        jni_sig!((java.lang.String, java.lang.String) -> void),
        &[JValue::Object(&key), JValue::Object(&val)],
    )?;
    Ok(())
}

/// ContentValues.put(key, new Short(value))
fn put_short_column(
    env: &mut Env,
    values: &JObject,
    column_class: &jni::strings::JNIStr,
    column_field: &jni::strings::JNIStr,
    value: i16,
) -> JniResult<()> {
    let key = static_string_field(env, column_class, column_field)?;
    let short_cls: JClass = env.find_class(jni_str!("java/lang/Short"))?;
    let val = env.new_object(&short_cls, jni_sig!((i16) -> void), &[JValue::Short(value)])?;
    env.call_method(
        values,
        jni_str!("put"),
        jni_sig!((java.lang.String, java.lang.Short) -> void),
        &[JValue::Object(&key), JValue::Object(&val)],
    )?;
    Ok(())
}

/// MediaStore 外置存储 collection URI 是 API 29 起的稳定契约,
/// 这里映射到对应的标准公共目录名 (RELATIVE_PATH 前缀).
pub(crate) fn media_dir_prefix(uri: &str) -> Option<&'static str> {
    if uri.starts_with("content://media/external/images") {
        Some("DCIM")
    } else if uri.starts_with("content://media/external/audio") {
        Some("Music")
    } else if uri.starts_with("content://media/external/video") {
        Some("Movies")
    } else if uri.starts_with("content://media/external/downloads") {
        Some("Download")
    } else {
        None
    }
}

/// 在指定 MediaStore collection 下创建文件条目, 返回 content URI.
pub fn jni_content_create_uri(
    uri: &str,
    relative_path: &str,
    file_name: &str,
    file_format: PlatformFileFormat,
) -> Result<String> {
    with_env(|env| {
        let resolver = content_resolver(env)?;
        let j_uri = parse_uri(env, uri)?;

        let values_cls: JClass = env.find_class(jni_str!("android/content/ContentValues"))?;
        let values = env.new_object(&values_cls, jni_sig!(() -> void), &[])?;
        put_string_column(env, &values, jni_str!("android/provider/MediaStore$MediaColumns"), jni_str!("DISPLAY_NAME"), file_name)?;
        put_string_column(env, &values, jni_str!("android/provider/MediaStore$MediaColumns"), jni_str!("MIME_TYPE"), file_format.mime_type())?;

        // RELATIVE_PATH = 公共目录 + 相对路径 (MediaStore 目录约定以 / 结尾)
        let relative_path = match media_dir_prefix(uri) {
            Some(dir) => [dir, "/", relative_path, "/"].concat(),
            None => [relative_path, "/"].concat(),
        };
        put_string_column(env, &values, jni_str!("android/provider/MediaStore$MediaColumns"), jni_str!("RELATIVE_PATH"), &relative_path)?;

        // IS_PENDING = 1 (Short): 写完 flush 时置 0
        put_short_column(env, &values, jni_str!("android/provider/MediaStore$MediaColumns"), jni_str!("IS_PENDING"), 1)?;

        let inserted = env.call_method(
            &resolver,
            jni_str!("insert"),
            jni_sig!((android.net.Uri, android.content.ContentValues) -> android.net.Uri),
            &[JValue::Object(&j_uri), JValue::Object(&values)],
        )?;
        let inserted = inserted.l()?;
        if inserted.is_null() {
            return Err(Error::InvalidUri);
        }
        let text = env.call_method(
            &inserted,
            jni_str!("toString"),
            jni_sig!(() -> java.lang.String),
            &[],
        )?;
        let text = text.l()?;
        let text = env.cast_local::<JString>(text)?;
        Ok(text.to_string())
    })
}

/// 打开 content URI 对应的文件描述符 (mode: r / wt / wa / rw...)
pub fn jni_content_open_fd(uri: &str, mode: &str) -> Result<i32> {
    let fd = with_env(|env| {
        let resolver = content_resolver(env)?;
        let j_uri = parse_uri(env, uri)?;
        let j_mode = env.new_string(mode)?;
        let pfd = env.call_method(
            &resolver,
            jni_str!("openFileDescriptor"),
            jni_sig!((android.net.Uri, java.lang.String) -> android.os.ParcelFileDescriptor),
            &[JValue::Object(&j_uri), JValue::Object(&j_mode)],
        )?;
        let pfd = pfd.l()?;
        if pfd.is_null() {
            return Err(Error::InvalidUri);
        }
        let fd = env.call_method(&pfd, jni_str!("detachFd"), jni_sig!(() -> jint), &[])?;
        Ok(fd.i()?)
    })?;
    if fd < 0 {
        return Err(Error::InvalidUri);
    }
    Ok(fd)
}

/// 写结束时把 IS_PENDING 置回 0, 文件才对其他应用可见
pub fn jni_content_flush(uri: &str, is_pending: bool) -> Result<bool> {
    with_env(|env| {
        let resolver = content_resolver(env)?;
        let j_uri = parse_uri(env, uri)?;

        let values_cls: JClass = env.find_class(jni_str!("android/content/ContentValues"))?;
        let values = env.new_object(&values_cls, jni_sig!(() -> void), &[])?;
        let pending: i16 = if is_pending { 1 } else { 0 };
        put_short_column(env, &values, jni_str!("android/provider/MediaStore$MediaColumns"), jni_str!("IS_PENDING"), pending)?;

        let row = env.call_method(
            &resolver,
            jni_str!("update"),
            jni_sig!((android.net.Uri, android.content.ContentValues, java.lang.String, [java.lang.String]) -> jint),
            &[
                JValue::Object(&j_uri),
                JValue::Object(&values),
                JValue::Object(&JObject::null()),
                JValue::Object(&JObject::null()),
            ],
        )?;
        Ok(row.i()? > 0)
    })
}

/// 删除 content URI 对应的条目
pub fn jni_content_delete(uri: &str) -> Result<()> {
    let n = with_env(|env| {
        let resolver = content_resolver(env)?;
        let j_uri = parse_uri(env, uri)?;
        let deleted = env.call_method(
            &resolver,
            jni_str!("delete"),
            jni_sig!((android.net.Uri, java.lang.String, [java.lang.String]) -> jint),
            &[
                JValue::Object(&j_uri),
                JValue::Object(&JObject::null()),
                JValue::Object(&JObject::null()),
            ],
        )?;
        Ok(deleted.i()?)
    })?;
    if n < 0 {
        return Err(Error::InvalidUri);
    }
    Ok(())
}

/// 查询 content URI 的 (文件名, 大小) — 4 参数 query + 空条件
pub fn jni_content_query_file_info(uri: &str) -> Result<(String, u64)> {
    with_env(|env| {
        let resolver = content_resolver(env)?;
        let j_uri = parse_uri(env, uri)?;
        let cursor = env.call_method(
            &resolver,
            jni_str!("query"),
            jni_sig!((android.net.Uri, [java.lang.String], android.os.Bundle, android.os.CancellationSignal) -> android.database.Cursor),
            &[
                JValue::Object(&j_uri),
                JValue::Object(&JObject::null()),
                JValue::Object(&JObject::null()),
                JValue::Object(&JObject::null()),
            ],
        )?;
        let cursor = cursor.l()?;
        let mut file_name = String::new();
        let mut file_size = 0u64;
        if cursor.is_null() {
            return Ok((file_name, file_size));
        }

        let moved = env.call_method(&cursor, jni_str!("moveToFirst"), jni_sig!(() -> jboolean), &[])?;
        if moved.z()? {
            let name_key = static_string_field(env, jni_str!("android/provider/OpenableColumns"), jni_str!("DISPLAY_NAME"))?;
            let name_idx = env.call_method(
                &cursor,
                jni_str!("getColumnIndex"),
                jni_sig!((java.lang.String) -> jint),
                &[JValue::Object(&name_key)],
            )?.i()?;
            if name_idx != -1 {
                let name = env.call_method(
                    &cursor,
                    jni_str!("getString"),
                    jni_sig!((jint) -> java.lang.String),
                    &[JValue::Int(name_idx)],
                )?;
                let name = name.l()?;
                if !name.is_null() {
                    let name = env.cast_local::<JString>(name)?;
                    file_name = name.to_string();
                }
            }

            let size_key = static_string_field(env, jni_str!("android/provider/OpenableColumns"), jni_str!("SIZE"))?;
            let size_idx = env.call_method(
                &cursor,
                jni_str!("getColumnIndex"),
                jni_sig!((java.lang.String) -> jint),
                &[JValue::Object(&size_key)],
            )?.i()?;
            if size_idx != -1 {
                let size = env.call_method(
                    &cursor,
                    jni_str!("getLong"),
                    jni_sig!((jint) -> jlong),
                    &[JValue::Int(size_idx)],
                )?;
                file_size = size.j()? as u64;
            }
        }
        let _ = env.call_method(&cursor, jni_str!("close"), jni_sig!(() -> void), &[]);
        Ok((file_name, file_size))
    })
}

/// 查询 content URI 的 MIME 类型: 先 getType, 空则按扩展名走 MimeTypeMap
pub fn jni_content_query_mime_type(uri: &str) -> Result<String> {
    with_env(|env| {
        let resolver = content_resolver(env)?;
        let j_uri = parse_uri(env, uri)?;
        let mime = env.call_method(
            &resolver,
            jni_str!("getType"),
            jni_sig!((android.net.Uri) -> java.lang.String),
            &[JValue::Object(&j_uri)],
        )?;
        let mut mime_obj = mime.l()?;
        if mime_obj.is_null() {
            // 取最后一段 '.' 之后的扩展名
            let ext = uri.rsplit('.').next().unwrap_or("");
            let ext = ext.split(&['/', '?', '#'][..]).next().unwrap_or("");
            let j_ext = env.new_string(ext)?;
            let map_cls: JClass = env.find_class(jni_str!("android/webkit/MimeTypeMap"))?;
            let map = env.call_static_method(
                &map_cls,
                jni_str!("getSingleton"),
                jni_sig!(() -> android.webkit.MimeTypeMap),
                &[],
            )?;
            let map = map.l()?;
            mime_obj = env.call_method(
                &map,
                jni_str!("getMimeTypeFromExtension"),
                jni_sig!((java.lang.String) -> java.lang.String),
                &[JValue::Object(&j_ext)],
            )?.l()?;
        }
        if mime_obj.is_null() {
            return Ok(String::new());
        }
        let mime_obj = env.cast_local::<JString>(mime_obj)?;
        Ok(mime_obj.to_string())
    })
}
