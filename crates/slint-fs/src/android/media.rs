// MediaStore 条目查询: exists / parent / join.
//
// 统一走 4 参数 query (uri, columns, null, null) + 空条件,
// 返回值不经任何字符串拼接直接转 Java 对象 (File 双参构造器做路径合并).

use jni::objects::{JClass, JObject, JString, JValue};
use jni::strings::JNIStr;
use jni::{jni_sig, jni_str, Env};

use super::{content_resolver, parse_uri, static_string_field, with_env};
use crate::error::{Error, Result};

/// 单列 columns 数组 (待查询的字段列表)
fn single_column<'a>(
    env: &mut Env<'a>,
    column_class: &JNIStr,
    column_field: &JNIStr,
) -> Result<jni::objects::JObjectArray<'a>> {
    let column = static_string_field(env, column_class, column_field)?;
    let column = env.cast_local::<JString>(column)?;
    let str_cls: JClass<'a> = env.find_class(jni_str!("java/lang/String"))?;
    let columns = env.new_object_array(1, &str_cls, &column)?;
    columns.set_element(env, 0, &column)?;
    Ok(columns)
}

/// 4 参数 query: (uri, columns, null, null) → Cursor (首行已定位)
fn query_first_row<'a>(env: &mut Env<'a>, uri: &str, columns: &JObject) -> Result<JObject<'a>> {
    let resolver = content_resolver(env)?;
    let j_uri = parse_uri(env, uri)?;
    let cursor = env.call_method(
        &resolver,
        jni_str!("query"),
        jni_sig!((android.net.Uri, [java.lang.String], android.os.Bundle, android.os.CancellationSignal) -> android.database.Cursor),
        &[
            JValue::Object(&j_uri),
            JValue::Object(columns),
            JValue::Object(&JObject::null()),
            JValue::Object(&JObject::null()),
        ],
    )?;
    let cursor = cursor.l()?;
    if cursor.is_null() {
        return Err(Error::InvalidUri);
    }
    let moved = env.call_method(&cursor, jni_str!("moveToFirst"), jni_sig!(() -> jboolean), &[])?;
    if !moved.z()? {
        let _ = env.call_method(&cursor, jni_str!("close"), jni_sig!(() -> void), &[]);
        return Err(Error::InvalidUri);
    }
    Ok(cursor)
}

/// cursor.getString(index)
fn cursor_string_at(env: &mut Env, cursor: &JObject, index: i32) -> Result<String> {
    let value = env.call_method(
        cursor,
        jni_str!("getString"),
        jni_sig!((jint) -> java.lang.String),
        &[JValue::Int(index)],
    )?;
    let value = value.l()?;
    if value.is_null() {
        return Err(Error::InvalidUri);
    }
    let value = env.cast_local::<JString>(value)?;
    Ok(value.to_string())
}

/// Uri.fromFile(file).toString()
fn file_to_uri_string(env: &mut Env, file: &JObject) -> Result<String> {
    let uri_cls: JClass = env.find_class(jni_str!("android/net/Uri"))?;
    let uri = env.call_static_method(
        &uri_cls,
        jni_str!("fromFile"),
        jni_sig!((java.io.File) -> android.net.Uri),
        &[JValue::Object(file)],
    )?;
    let uri = uri.l()?;
    let text = env.call_method(&uri, jni_str!("toString"), jni_sig!(() -> java.lang.String), &[])?;
    let text = text.l()?;
    let text = env.cast_local::<JString>(text)?;
    Ok(text.to_string())
}

pub fn jni_content_exists(uri: &str) -> bool {
    with_env(|env| {
        let columns = single_column(env, jni_str!("android/provider/MediaStore$MediaColumns"), jni_str!("_ID"))?;
        let cursor = query_first_row(env, uri, &columns)?;
        let _ = env.call_method(&cursor, jni_str!("close"), jni_sig!(() -> void), &[]);
        Ok(())
    }).is_ok()
}

pub fn jni_content_parent(uri: &str) -> Result<String> {
    with_env(|env| {
        let columns = single_column(env, jni_str!("android/provider/MediaStore$MediaColumns"), jni_str!("_DATA"))?;
        let cursor = query_first_row(env, uri, &columns)?;
        let path = cursor_string_at(env, &cursor, 0)?;
        let _ = env.call_method(&cursor, jni_str!("close"), jni_sig!(() -> void), &[]);

        let j_path = env.new_string(&path)?;
        let file_cls: JClass = env.find_class(jni_str!("java/io/File"))?;
        let file = env.new_object(
            &file_cls,
            jni_sig!((java.lang.String) -> void),
            &[JValue::Object(&j_path)],
        )?;
        let parent = env.call_method(
            &file,
            jni_str!("getParentFile"),
            jni_sig!(() -> java.io.File),
            &[],
        )?;
        let parent = parent.l()?;
        if parent.is_null() {
            return Err(Error::InvalidUri);
        }
        file_to_uri_string(env, &parent)
    })
}

pub fn jni_content_join(uri: &str, name: &str) -> Result<String> {
    with_env(|env| {
        let columns = single_column(env, jni_str!("android/provider/MediaStore$MediaColumns"), jni_str!("_DATA"))?;
        let cursor = query_first_row(env, uri, &columns)?;
        let path = cursor_string_at(env, &cursor, 0)?;
        let _ = env.call_method(&cursor, jni_str!("close"), jni_sig!(() -> void), &[]);

        // new File(parentPath, name) 双参构造器完成路径合并, 无字符串拼接
        let j_path = env.new_string(&path)?;
        let j_name = env.new_string(name)?;
        let file_cls: JClass = env.find_class(jni_str!("java/io/File"))?;
        let joined = env.new_object(
            &file_cls,
            jni_sig!((java.lang.String, java.lang.String) -> void),
            &[JValue::Object(&j_path), JValue::Object(&j_name)],
        )?;
        file_to_uri_string(env, &joined)
    })
}
