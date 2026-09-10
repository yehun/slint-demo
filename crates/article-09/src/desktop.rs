use anyhow::Result;
use log::{error, info};
use nokhwa::pixel_format::RgbFormat;
use nokhwa::utils::{CameraIndex, RequestedFormat, RequestedFormatType};
use parking_lot::RwLock;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crate::service::{CameraFrame, CameraService, CameraState, FrameCallback, StateCallback};
use crate::utils;

#[derive(Clone)]
pub struct PlatformCameraService {
    state_callback: Arc<RwLock<Option<StateCallback>>>,
    frame_callback: Arc<RwLock<Option<FrameCallback>>>,
    streaming: Arc<RwLock<bool>>,
    thread_handle: Arc<RwLock<Option<thread::JoinHandle<()>>>>,
}

impl PlatformCameraService {
    pub fn new() -> Self {
        Self {
            state_callback: Arc::new(RwLock::new(None)),
            frame_callback: Arc::new(RwLock::new(None)),
            streaming: Arc::new(RwLock::new(false)),
            thread_handle: Arc::new(RwLock::new(None)),
        }
    }

    fn notify_state(&self, state: CameraState) {
        let guard = self.state_callback.read();
        if let Some(callback) = &*guard {
            callback(state);
        }
    }

    fn start_camera_thread(&self) -> Result<()> {
        let state_callback = Arc::clone(&self.state_callback);
        let frame_callback = Arc::clone(&self.frame_callback);
        let streaming = Arc::clone(&self.streaming);

        // 确保之前没有运行的线程
        self.stop_streaming()?;
        Self::join_previous_thread(&self.thread_handle);
        {
            *streaming.write() = true;
        }
        let handle = thread::spawn(move || {
            if let Err(e) = Self::run_camera_loop(state_callback, frame_callback, streaming) {
                error!("相机线程错误: {}", e);
            }
        });
        *self.thread_handle.write() = Some(handle);

        Ok(())
    }

    /// 有界等待相机线程退出 (最长 1s)
    fn join_previous_thread(thread_handle: &Arc<RwLock<Option<thread::JoinHandle<()>>>>) {
        let Some(handle) = thread_handle.write().take() else {
            return;
        };
        let deadline = std::time::Instant::now() + Duration::from_secs(1);
        while !handle.is_finished() && std::time::Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        if !handle.is_finished() {
            log::warn!("相机线程 1s 内未退出");
        }
    }

    fn run_camera_loop(
        state_callback: Arc<RwLock<Option<StateCallback>>>,
        frame_callback: Arc<RwLock<Option<FrameCallback>>>,
        streaming: Arc<RwLock<bool>>,
    ) -> Result<()> {
        // 通知正在打开
        if let Some(callback) = state_callback.read().as_ref() {
            callback(CameraState::Opening);
        }

        // 使用第一个摄像头, RGB 格式, 最高帧率 30fps
        let index = CameraIndex::Index(0);
        let requested =
            RequestedFormat::new::<RgbFormat>(RequestedFormatType::HighestFrameRate(30));

        // 创建相机
        let mut camera = match nokhwa::Camera::new(index, requested) {
            Ok(cam) => cam,
            Err(e) => {
                error!("创建相机失败: {}", e);
                if let Some(callback) = state_callback.read().as_ref() {
                    callback(CameraState::Error);
                }
                return Err(anyhow::anyhow!("创建相机失败: {}", e));
            }
        };

        // 打开流
        if let Err(e) = camera.open_stream() {
            error!("打开流失败: {}", e);
            if let Some(callback) = state_callback.read().as_ref() {
                callback(CameraState::Error);
            }
            return Err(anyhow::anyhow!("打开流失败: {}", e));
        }

        // 通知已打开
        if let Some(callback) = state_callback.read().as_ref() {
            callback(CameraState::Opened);
        }
        info!("相机启动成功");

        // 主循环
        while *streaming.read() {
            if !camera.is_stream_open() {
                break;
            }
            match camera.frame() {
                Ok(frame) => {
                    let (img_data, width, height) = match frame.decode_image::<RgbFormat>() {
                        Ok(img) => (img.to_vec(), img.width(), img.height()),
                        Err(_) => {
                            return Err(anyhow::anyhow!("解码帧失败"));
                        }
                    };
                    // 水平翻转 (镜像效果)
                    let img_data = utils::flip_rgb_horizontal(&img_data, width, height);
                    let frame_data = CameraFrame {
                        data: img_data,
                        width,
                        height,
                    };
                    if let Some(callback) = frame_callback.read().as_ref() {
                        callback(frame_data);
                    }
                }
                Err(e) => {
                    error!("帧采集错误: {}", e);
                }
            }
            thread::sleep(Duration::from_millis(33)); // ~30 FPS
        }

        // 清理
        info!("正在停止相机...");
        camera.stop_stream()?;

        if let Some(callback) = state_callback.read().as_ref() {
            callback(CameraState::Closed);
        }
        info!("相机已停止");
        Ok(())
    }

    fn stop_streaming(&self) -> Result<()> {
        *self.streaming.write() = false;
        Ok(())
    }
}

impl CameraService for PlatformCameraService {
    fn open(&self) -> Result<()> {
        info!("正在打开相机...");
        self.start_camera_thread()
    }

    fn close(&self) -> Result<()> {
        info!("正在关闭相机...");
        self.notify_state(CameraState::Closing);
        self.stop_streaming()?;
        Self::join_previous_thread(&self.thread_handle);
        Ok(())
    }

    fn switch(&self) -> Result<()> {
        Err(anyhow::anyhow!("Desktop 端暂不支持切换相机"))
    }

    fn can_switch(&self) -> bool {
        false
    }

    fn set_state_callback(&self, callback: StateCallback) -> Result<()> {
        *self.state_callback.write() = Some(callback);
        Ok(())
    }

    fn set_frame_callback(&self, callback: FrameCallback) -> Result<()> {
        *self.frame_callback.write() = Some(callback);
        Ok(())
    }
}

impl Drop for PlatformCameraService {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
