// JNI 回调: Java → Rust
//
// Java 侧 CameraCallback.onCameraFrame/onCameraState 是 native 方法,
// 通过 #[unsafe(no_mangle)] 导出 C 符号, 供 JNI 运行时调用.

use std::sync::OnceLock;

use jni::objects::{JByteArray, JClass};
use jni::{EnvUnowned};

use crate::service::{CameraFrame, CameraState};

/// 帧数据类型 (跨线程传递)
type FrameCallback = Box<dyn Fn(CameraFrame) + Send + Sync>;
type StateCallback = Box<dyn Fn(CameraState) + Send + Sync>;

/// 全局回调存储 (简化版: 单例模式, 只保存一个回调)
struct CallbackStorage {
    frame_callback: Option<FrameCallback>,
    state_callback: Option<StateCallback>,
}

impl CallbackStorage {
    fn new() -> Self {
        Self {
            frame_callback: None,
            state_callback: None,
        }
    }
}

static CALLBACK_STORAGE: OnceLock<std::sync::Mutex<CallbackStorage>> = OnceLock::new();

fn storage() -> &'static std::sync::Mutex<CallbackStorage> {
    CALLBACK_STORAGE.get_or_init(|| std::sync::Mutex::new(CallbackStorage::new()))
}

/// 注册帧回调 (由 PlatformCameraService 调用)
pub fn set_frame_callback(callback: FrameCallback) {
    eprintln!("[article-09] set_frame_callback 注册");
    storage().lock().unwrap().frame_callback = Some(callback);
}

/// 注册状态回调 (由 PlatformCameraService 调用)
pub fn set_state_callback(callback: StateCallback) {
    eprintln!("[article-09] set_state_callback 注册");
    storage().lock().unwrap().state_callback = Some(callback);
}

/// JNI 回调: Java 侧帧数据到达
///
/// 对应 Java: com.example.article_09.camera.CameraCallback.onCameraFrame(byte[], int, int)
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub fn Java_com_example_article09_camera_CameraCallback_onCameraFrame(
    mut env_unowned: EnvUnowned,
    _class: JClass,
    data: JByteArray,
    width: i32,
    height: i32,
) {
    eprintln!("[article-09] JNI onCameraFrame 被调用: {}x{}", width, height);
    let _ = env_unowned.with_env(|env| {
        let rgb_data = env.convert_byte_array(data).unwrap_or_default();
        eprintln!("[article-09] 帧数据大小: {} bytes", rgb_data.len());
        let frame = CameraFrame {
            data: rgb_data,
            width: width as u32,
            height: height as u32,
        };
        let guard = storage().lock().unwrap();
        if let Some(ref cb) = guard.frame_callback {
            cb(frame);
        } else {
            eprintln!("[article-09] 警告: 帧回调未注册!");
        }
        Ok::<(), jni::errors::Error>(())
    });
}

/// JNI 回调: Java 侧状态变化
///
/// 对应 Java: com.example.article_09.camera.CameraCallback.onCameraState(int)
/// state: 1=打开, 0=关闭, -1=错误
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub fn Java_com_example_article09_camera_CameraCallback_onCameraState(
    _env: EnvUnowned,
    _class: JClass,
    state: i32,
) {
    eprintln!("[article-09] JNI onCameraState 被调用: state={}", state);
    let new_state = match state {
        1 => CameraState::Opened,
        0 => CameraState::Closed,
        _ => CameraState::Error,
    };
    let guard = storage().lock().unwrap();
    if let Some(ref cb) = guard.state_callback {
        cb(new_state);
    } else {
        eprintln!("[article-09] 警告: 状态回调未注册!");
    }
}
