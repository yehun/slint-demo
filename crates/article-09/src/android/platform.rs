// Android 平台相机服务实现
//
// 通过 JNI 调用 Java 侧的 CameraInvoke 控制相机,
// 通过全局回调接收 Java 侧的帧/状态数据.
//
// 注意: set_state_callback 的闭包需要 'static 生命周期,
// 所以 opened 标志用 Arc<AtomicBool>, 而不是 &self 引用.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::android::api::CameraInvokeApi;
use crate::android::callback::{set_frame_callback, set_state_callback};
use crate::service::{CameraService, CameraState, FrameCallback, StateCallback};

/// Android 端相机服务
pub struct PlatformCameraService {
    /// 相机打开状态 (由状态回调维护, Arc 用于 'static 闭包)
    opened: Arc<AtomicBool>,
}

impl PlatformCameraService {
    pub fn new() -> Self {
        Self {
            opened: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl CameraService for PlatformCameraService {
    fn open(&self) -> anyhow::Result<()> {
        CameraInvokeApi::open().map_err(|e| anyhow::anyhow!("JNI open 错误: {}", e))
    }

    fn close(&self) -> anyhow::Result<()> {
        self.opened.store(false, Ordering::SeqCst);
        CameraInvokeApi::close().map_err(|e| anyhow::anyhow!("JNI close 错误: {}", e))
    }

    fn switch(&self) -> anyhow::Result<()> {
        CameraInvokeApi::toggle().map_err(|e| anyhow::anyhow!("JNI toggle 错误: {}", e))
    }

    fn can_switch(&self) -> bool {
        true // Android 支持前后摄切换
    }

    fn set_state_callback(&self, callback: StateCallback) -> anyhow::Result<()> {
        // 包装回调: 维护 opened 状态 + 转发原始回调
        let opened = Arc::clone(&self.opened);
        set_state_callback(Box::new(move |state| {
            match state {
                CameraState::Opened => opened.store(true, Ordering::SeqCst),
                CameraState::Closed | CameraState::Error => opened.store(false, Ordering::SeqCst),
                _ => {}
            };
            callback(state);
        }));
        Ok(())
    }

    fn set_frame_callback(&self, callback: FrameCallback) -> anyhow::Result<()> {
        set_frame_callback(callback);
        Ok(())
    }
}
