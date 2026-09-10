use anyhow::Result;

/// 相机状态
#[derive(Debug, Clone, PartialEq)]
pub enum CameraState {
    Opening,
    Closing,
    Opened,
    Closed,
    Error,
}

/// 帧数据 (纯 RGB 字节数组 + 宽高)
#[derive(Debug, Clone)]
pub struct CameraFrame {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// 状态回调类型
pub type StateCallback = Box<dyn Fn(CameraState) + Send + Sync>;

/// 帧回调类型
pub type FrameCallback = Box<dyn Fn(CameraFrame) + Send + Sync>;

/// 平台无关的相机服务 trait
pub trait CameraService {
    /// 打开相机
    fn open(&self) -> Result<()>;

    /// 关闭相机
    fn close(&self) -> Result<()>;

    /// 切换相机 (前后摄)
    fn switch(&self) -> Result<()>;

    /// 是否支持切换相机
    fn can_switch(&self) -> bool;

    /// 设置状态回调
    fn set_state_callback(&self, callback: StateCallback) -> Result<()>;

    /// 设置帧数据回调
    fn set_frame_callback(&self, callback: FrameCallback) -> Result<()>;
}
